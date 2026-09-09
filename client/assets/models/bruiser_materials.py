"""Bake geometry-aligned steel wear into the bruiser's own UV atlas."""

import math
from pathlib import Path

import bpy
from io_scene_gltf2.blender.imp.pbrMetallicRoughness import get_settings_group

RESOLUTION = 2048
OUTPUT = Path(__file__).with_name("bruiser_textures")


def remember_panel_coordinates(obj):
    half = [max(abs(vertex.co[i]) for vertex in obj.data.vertices) for i in range(3)]
    for name, values in (
        ("PanelPosition", [tuple(vertex.co) for vertex in obj.data.vertices]),
        ("PanelHalfSize", [half] * len(obj.data.vertices)),
    ):
        attribute = obj.data.attributes.new(name, "FLOAT_VECTOR", "POINT")
        for item, value in zip(attribute.data, values):
            item.vector = value


def bake_armour(objects, material, settings):
    scalar_fields = {
        "edge_width",
        "edge_chip_threshold",
        "roughness_variation",
        "wear_roughness_reduction",
        "exposed_metallic",
        "scratch_depth",
        "normal_strength",
    }
    color_fields = {"paint_variation_color", "exposed_metal_color"}
    if settings.keys() != scalar_fields | color_fields | {"chip_sites"}:
        raise ValueError("bruiser.materials.json: wear fields missing or unknown")
    for key in scalar_fields:
        value = settings[key]
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(value)
            or value < 0
        ):
            raise ValueError(
                f"bruiser.materials.json: wear.{key} must be nonnegative and finite"
            )
    for key in color_fields:
        value = settings[key]
        if len(value) != 3 or any(
            not math.isfinite(v) or not 0 <= v <= 1 for v in value
        ):
            raise ValueError(f"bruiser.materials.json: wear.{key} must be an RGB color")
    for site in settings["chip_sites"]:
        if site.keys() != {"center", "extent"} or any(len(site[k]) != 3 for k in site):
            raise ValueError(
                "bruiser.materials.json: each chip site needs center and extent vectors"
            )
        if any(not math.isfinite(v) for k in site for v in site[k]) or any(
            v <= 0 for v in site["extent"]
        ):
            raise ValueError(
                "bruiser.materials.json: chip coordinates must be finite and extents positive"
            )
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    mesh = bpy.context.object
    mesh.name = "Baked armour panels"
    mesh.data.materials.clear()
    mesh.data.materials.append(material)
    for face in mesh.data.polygons:
        face.material_index = 0
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(70), island_margin=0.012)
    bpy.ops.object.mode_set(mode="OBJECT")

    nodes, links = material.node_tree.nodes, material.node_tree.links
    base_shader = nodes.get("Principled BSDF")
    paint_color = tuple(base_shader.inputs["Base Color"].default_value)
    paint_roughness = base_shader.inputs["Roughness"].default_value
    paint_metallic = base_shader.inputs["Metallic"].default_value
    nodes.clear()

    def connect(value, socket):
        if isinstance(value, (float, int, tuple)):
            socket.default_value = value
        else:
            links.new(value, socket)

    def calculate(operation, a, b=0):
        node = nodes.new("ShaderNodeMath")
        node.operation = operation
        connect(a, node.inputs[0])
        connect(b, node.inputs[1])
        return node.outputs[0]

    def vector(operation, a, b=(0, 0, 0)):
        node = nodes.new("ShaderNodeVectorMath")
        node.operation = operation
        connect(a, node.inputs[0])
        connect(b, node.inputs[1])
        return node.outputs[0]

    def noise(coordinates, scale):
        node = nodes.new("ShaderNodeTexNoise")
        connect(coordinates, node.inputs["Vector"])
        node.inputs["Scale"].default_value = scale
        node.inputs["Detail"].default_value = 2
        node.inputs["Roughness"].default_value = 0.6
        return node.outputs["Fac"]

    def mix(factor, a, b):
        node = nodes.new("ShaderNodeMixRGB")
        connect(factor, node.inputs[0])
        connect(a, node.inputs[1])
        connect(b, node.inputs[2])
        return node.outputs[0]

    position = nodes.new("ShaderNodeAttribute")
    position.attribute_name = "PanelPosition"
    half = nodes.new("ShaderNodeAttribute")
    half.attribute_name = "PanelHalfSize"
    distances = nodes.new("ShaderNodeSeparateXYZ")
    links.new(
        vector(
            "SUBTRACT",
            half.outputs["Vector"],
            vector("ABSOLUTE", position.outputs["Vector"]),
        ),
        distances.inputs[0],
    )
    x, y, z = distances.outputs
    # The second-smallest distance measures proximity to an edge, not a face.
    edge_distance = calculate(
        "MAXIMUM",
        calculate("MINIMUM", x, y),
        calculate("MINIMUM", calculate("MAXIMUM", x, y), z),
    )
    geometry = nodes.new("ShaderNodeNewGeometry")
    coordinate = geometry.outputs["Position"]
    mottling = noise(coordinate, 22)
    chips = noise(coordinate, 85)
    width = calculate(
        "MULTIPLY",
        calculate("ADD", calculate("MULTIPLY", chips, 0.8), 0.2),
        settings["edge_width"],
    )
    edge_wear = calculate(
        "MULTIPLY",
        calculate("LESS_THAN", edge_distance, width),
        calculate(
            "GREATER_THAN", noise(coordinate, 24), settings["edge_chip_threshold"]
        ),
    )
    impact_chips = 0.0
    for site in settings["chip_sites"]:
        centre, extent = tuple(site["center"]), site["extent"]
        radial = nodes.new("ShaderNodeVectorMath")
        radial.operation = "LENGTH"
        links.new(
            vector(
                "MULTIPLY",
                vector("SUBTRACT", coordinate, centre),
                tuple(1 / value for value in extent),
            ),
            radial.inputs[0],
        )
        boundary = calculate("ADD", 0.55, calculate("MULTIPLY", chips, 0.65))
        impact_chips = calculate(
            "MAXIMUM",
            impact_chips,
            calculate("LESS_THAN", radial.outputs["Value"], boundary),
        )
    scratches = calculate(
        "GREATER_THAN", noise(vector("MULTIPLY", coordinate, (65, 5, 22)), 1), 0.71
    )
    scratches = calculate(
        "MULTIPLY", scratches, calculate("GREATER_THAN", mottling, 0.53)
    )
    wear = calculate("MAXIMUM", edge_wear, impact_chips)
    colour = mix(mottling, paint_color, (*settings["paint_variation_color"], 1))
    colour = mix(wear, colour, (*settings["exposed_metal_color"], 1))
    roughness = calculate(
        "ADD",
        paint_roughness,
        calculate("MULTIPLY", mottling, settings["roughness_variation"]),
    )
    roughness = calculate(
        "SUBTRACT",
        roughness,
        calculate("MULTIPLY", wear, settings["wear_roughness_reduction"]),
    )
    metallic = calculate(
        "ADD",
        paint_metallic,
        calculate("MULTIPLY", wear, settings["exposed_metallic"] - paint_metallic),
    )
    ao = nodes.new("ShaderNodeAmbientOcclusion")
    ao.inputs["Distance"].default_value = 0.065
    ao.samples = 16
    packed = nodes.new("ShaderNodeCombineColor")
    links.new(ao.outputs["AO"], packed.inputs["Red"])
    links.new(roughness, packed.inputs["Green"])
    links.new(metallic, packed.inputs["Blue"])

    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Distance"].default_value = settings["scratch_depth"]
    bump.inputs["Strength"].default_value = settings["normal_strength"]
    bump.invert = True
    links.new(
        calculate(
            "MAXIMUM", scratches, calculate("MULTIPLY", noise(coordinate, 650), 0.12)
        ),
        bump.inputs["Height"],
    )
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(bump.outputs["Normal"], shader.inputs["Normal"])
    emission = nodes.new("ShaderNodeEmission")
    output = nodes.new("ShaderNodeOutputMaterial")
    target = nodes.new("ShaderNodeTexImage")
    nodes.active = target

    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 16
    scene.render.bake.margin = 12
    scene.render.bake.use_selected_to_active = False
    scene.render.bake.use_clear = True
    OUTPUT.mkdir(exist_ok=True)
    images = {}
    for channel, source in (
        ("albedo", colour),
        ("metallic-roughness", packed.outputs[0]),
        ("normal-gl", None),
    ):
        image = bpy.data.images.new(
            f"bruiser-{channel}", width=RESOLUTION, height=RESOLUTION, alpha=False
        )
        image.colorspace_settings.name = "sRGB" if channel == "albedo" else "Non-Color"
        target.image = image
        if source is None:
            links.new(shader.outputs["BSDF"], output.inputs["Surface"])
            bpy.ops.object.bake(type="NORMAL")
        else:
            links.new(source, emission.inputs["Color"])
            links.new(emission.outputs[0], output.inputs["Surface"])
            bpy.ops.object.bake(type="EMIT")
        image.filepath_raw = str(OUTPUT / f"bruiser-{channel}.png")
        image.file_format = "PNG"
        image.save()
        image.pack()
        images[channel] = image

    nodes.clear()
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    output = nodes.new("ShaderNodeOutputMaterial")
    links.new(shader.outputs[0], output.inputs["Surface"])
    textures = {}
    for channel, image in images.items():
        node = nodes.new("ShaderNodeTexImage")
        node.image = image
        node.extension = "EXTEND"
        textures[channel] = node.outputs["Color"]
    links.new(textures["albedo"], shader.inputs["Base Color"])
    separate = nodes.new("ShaderNodeSeparateColor")
    links.new(textures["metallic-roughness"], separate.inputs[0])
    links.new(separate.outputs["Green"], shader.inputs["Roughness"])
    links.new(separate.outputs["Blue"], shader.inputs["Metallic"])
    occlusion = nodes.new("ShaderNodeGroup")
    occlusion.node_tree = get_settings_group()
    links.new(separate.outputs["Red"], occlusion.inputs["Occlusion"])
    normal = nodes.new("ShaderNodeNormalMap")
    links.new(textures["normal-gl"], normal.inputs["Color"])
    links.new(normal.outputs[0], shader.inputs["Normal"])
    for name in ("PanelPosition", "PanelHalfSize"):
        mesh.data.attributes.remove(mesh.data.attributes[name])
    return mesh
