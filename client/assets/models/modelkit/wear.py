"""Bake material-appropriate surface wear into a model-specific UV atlas."""

import math
from pathlib import Path

import bpy
from io_scene_gltf2.blender.imp.pbrMetallicRoughness import get_settings_group
from mathutils import Vector
from mathutils.bvhtree import BVHTree

RESOLUTION = 2048
STYLES = ("painted", "plastic", "metal")
PANEL_ATTRIBUTES = ("PanelPosition", "PanelHalfSize")


class Graph:
    """Shader-node builders over one material's node tree."""

    def __init__(self, material):
        self.nodes = material.node_tree.nodes
        self.links = material.node_tree.links

    def connect(self, value, socket):
        if isinstance(value, (float, int, tuple)):
            socket.default_value = value
        else:
            self.links.new(value, socket)

    def calculate(self, operation, a, b=0):
        node = self.nodes.new("ShaderNodeMath")
        node.operation = operation
        self.connect(a, node.inputs[0])
        self.connect(b, node.inputs[1])
        return node.outputs[0]

    def vector(self, operation, a, b=(0, 0, 0)):
        node = self.nodes.new("ShaderNodeVectorMath")
        node.operation = operation
        self.connect(a, node.inputs[0])
        self.connect(b, node.inputs[1])
        return node.outputs[0]

    def noise(self, coordinates, scale):
        node = self.nodes.new("ShaderNodeTexNoise")
        self.connect(coordinates, node.inputs["Vector"])
        node.inputs["Scale"].default_value = scale
        node.inputs["Detail"].default_value = 2
        node.inputs["Roughness"].default_value = 0.6
        return node.outputs["Fac"]

    def mix(self, factor, a, b):
        node = self.nodes.new("ShaderNodeMixRGB")
        self.connect(factor, node.inputs[0])
        self.connect(a, node.inputs[1])
        self.connect(b, node.inputs[2])
        return node.outputs[0]


def panel_coordinates(obj):
    """Store each vertex's offset from its part's centre and the part's half size."""
    low = [min(vertex.co[i] for vertex in obj.data.vertices) for i in range(3)]
    high = [max(vertex.co[i] for vertex in obj.data.vertices) for i in range(3)]
    center = [(a + b) / 2 for a, b in zip(low, high)]
    half = [(b - a) / 2 for a, b in zip(low, high)]
    for name, values in (
        (
            "PanelPosition",
            [tuple(vertex.co[i] - center[i] for i in range(3)) for vertex in obj.data.vertices],
        ),
        ("PanelHalfSize", [half] * len(obj.data.vertices)),
    ):
        attribute = obj.data.attributes.new(name, "FLOAT_VECTOR", "POINT")
        for item, value in zip(attribute.data, values):
            item.vector = value


def validate_painted_wear(settings, name):
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
    if settings.keys() != scalar_fields | color_fields | {"style", "chip_sites"}:
        raise ValueError(f"{name}: wear fields missing or unknown")
    for key in scalar_fields:
        value = settings[key]
        if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) or value < 0:
            raise ValueError(f"{name}: wear.{key} must be nonnegative and finite")
    for key in color_fields:
        value = settings[key]
        if len(value) != 3 or any(not math.isfinite(v) or not 0 <= v <= 1 for v in value):
            raise ValueError(f"{name}: wear.{key} must be an RGB color")
    for site in settings["chip_sites"]:
        if site.keys() != {"center", "extent"} or any(len(site[k]) != 3 for k in site):
            raise ValueError(f"{name}: each chip site needs center and extent vectors")
        if any(not math.isfinite(v) for k in site for v in site[k]) or any(v <= 0 for v in site["extent"]):
            raise ValueError(f"{name}: chip coordinates must be finite and extents positive")


def validate_surface_wear(settings, name):
    colors = {"scuff_color", "scratch_color"}
    scalars = {"scuff_roughness", "scratch_depth", "dent_depth", "grain_depth"}
    if settings.keys() != colors | scalars | {
        "style",
        "scuff_sites",
        "scratches",
        "dents",
    }:
        raise ValueError(f"{name}: surface wear fields missing or unknown")

    def vector(value):
        return (
            isinstance(value, list)
            and len(value) == 3
            and all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) for v in value)
        )

    for field in colors:
        if not vector(settings[field]) or any(not 0 <= v <= 1 for v in settings[field]):
            raise ValueError(f"{name}: wear.{field} must be an RGB color")
    for field in scalars:
        value = settings[field]
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(value)
            or value < 0
            or (field == "scuff_roughness" and value > 1)
        ):
            raise ValueError(f"{name}: wear.{field} is invalid")
    for field in ("scuff_sites", "dents"):
        for site in settings[field]:
            if (
                site.keys() != {"center", "extent"}
                or not all(vector(site[k]) for k in site)
                or any(v <= 0 for v in site["extent"])
            ):
                raise ValueError(f"{name}: wear.{field} requires centers and positive extents")
    for stroke in settings["scratches"]:
        if (
            stroke.keys() != {"start", "end", "width"}
            or not vector(stroke["start"])
            or not vector(stroke["end"])
            or stroke["start"] == stroke["end"]
            or not isinstance(stroke["width"], (int, float))
            or not math.isfinite(stroke["width"])
            or stroke["width"] <= 0
        ):
            raise ValueError(f"{name}: scratches require distinct endpoints and positive widths")


def painted_wear_graph(graph, settings, base):
    """Edge chips, impact chips, and scratches over the base paint."""
    base_color, base_roughness, base_metallic = base
    position = graph.nodes.new("ShaderNodeAttribute")
    position.attribute_name = "PanelPosition"
    half = graph.nodes.new("ShaderNodeAttribute")
    half.attribute_name = "PanelHalfSize"
    distances = graph.nodes.new("ShaderNodeSeparateXYZ")
    graph.links.new(
        graph.vector(
            "SUBTRACT",
            half.outputs["Vector"],
            graph.vector("ABSOLUTE", position.outputs["Vector"]),
        ),
        distances.inputs[0],
    )
    x, y, z = distances.outputs
    # The second-smallest distance measures proximity to an edge, not a face.
    edge_distance = graph.calculate(
        "MAXIMUM",
        graph.calculate("MINIMUM", x, y),
        graph.calculate("MINIMUM", graph.calculate("MAXIMUM", x, y), z),
    )
    geometry = graph.nodes.new("ShaderNodeNewGeometry")
    coordinate = geometry.outputs["Position"]
    mottling = graph.noise(coordinate, 22)
    chips = graph.noise(coordinate, 85)
    width = graph.calculate(
        "MULTIPLY",
        graph.calculate("ADD", graph.calculate("MULTIPLY", chips, 0.8), 0.2),
        settings["edge_width"],
    )
    edge_wear = graph.calculate(
        "MULTIPLY",
        graph.calculate("LESS_THAN", edge_distance, width),
        graph.calculate("GREATER_THAN", graph.noise(coordinate, 24), settings["edge_chip_threshold"]),
    )
    impact_chips = 0.0
    for site in settings["chip_sites"]:
        centre, extent = tuple(site["center"]), site["extent"]
        radial = graph.nodes.new("ShaderNodeVectorMath")
        radial.operation = "LENGTH"
        graph.links.new(
            graph.vector(
                "MULTIPLY",
                graph.vector("SUBTRACT", coordinate, centre),
                tuple(1 / value for value in extent),
            ),
            radial.inputs[0],
        )
        boundary = graph.calculate("ADD", 0.55, graph.calculate("MULTIPLY", chips, 0.65))
        impact_chips = graph.calculate(
            "MAXIMUM",
            impact_chips,
            graph.calculate("LESS_THAN", radial.outputs["Value"], boundary),
        )
    scratches = graph.calculate(
        "GREATER_THAN",
        graph.noise(graph.vector("MULTIPLY", coordinate, (65, 5, 22)), 1),
        0.71,
    )
    scratches = graph.calculate("MULTIPLY", scratches, graph.calculate("GREATER_THAN", mottling, 0.53))
    wear = graph.calculate("MAXIMUM", edge_wear, impact_chips)
    colour = graph.mix(mottling, base_color, (*settings["paint_variation_color"], 1))
    colour = graph.mix(wear, colour, (*settings["exposed_metal_color"], 1))
    roughness = graph.calculate(
        "ADD",
        base_roughness,
        graph.calculate("MULTIPLY", mottling, settings["roughness_variation"]),
    )
    roughness = graph.calculate(
        "SUBTRACT",
        roughness,
        graph.calculate("MULTIPLY", wear, settings["wear_roughness_reduction"]),
    )
    metallic = graph.calculate(
        "ADD",
        base_metallic,
        graph.calculate("MULTIPLY", wear, settings["exposed_metallic"] - base_metallic),
    )
    bump = graph.nodes.new("ShaderNodeBump")
    bump.inputs["Distance"].default_value = settings["scratch_depth"]
    bump.inputs["Strength"].default_value = settings["normal_strength"]
    bump.invert = True
    graph.links.new(
        graph.calculate(
            "MAXIMUM",
            scratches,
            graph.calculate("MULTIPLY", graph.noise(coordinate, 650), 0.12),
        ),
        bump.inputs["Height"],
    )
    return colour, roughness, metallic, bump.outputs["Normal"]


def surface_wear_graph(graph, mesh, settings, base, style, model):
    """Scuffs, projected scratch strokes, dents, and micrograin on a solid shell.

    Returns the four channels and the scratch masks baked along the way."""
    base_color, base_roughness, base_metallic = base
    settings_name = model.with_suffix(".json").name
    scratch_images = []
    geometry = graph.nodes.new("ShaderNodeNewGeometry")
    coordinate = geometry.outputs["Position"]
    surface = BVHTree.FromPolygons(
        [v.co for v in mesh.data.vertices],
        [p.vertices for p in mesh.data.polygons],
    )

    def patch(site):
        center, _, _, _ = surface.find_nearest(Vector(site["center"]))
        if center is None:
            raise ValueError(f"{settings_name}: wear patch has no target surface")
        radial = graph.nodes.new("ShaderNodeVectorMath")
        radial.operation = "LENGTH"
        graph.links.new(
            graph.vector(
                "MULTIPLY",
                graph.vector("SUBTRACT", coordinate, tuple(center)),
                tuple(1 / v for v in site["extent"]),
            ),
            radial.inputs[0],
        )
        return graph.calculate("MAXIMUM", graph.calculate("SUBTRACT", 1, radial.outputs["Value"]), 0)

    scuffs = 0.0
    for site in settings["scuff_sites"]:
        scuffs = graph.calculate("MAXIMUM", scuffs, graph.calculate("MULTIPLY", patch(site), 2.5))
    scuffs = graph.calculate("MINIMUM", scuffs, 1)
    scuffs = graph.calculate(
        "MULTIPLY",
        scuffs,
        graph.calculate("ADD", 0.4, graph.calculate("MULTIPLY", graph.noise(coordinate, 160), 0.6)),
    )
    scratch_wander = graph.noise(coordinate, 115)
    scratch_width = graph.noise(coordinate, 210)
    scratch_breaks = graph.noise(coordinate, 130)
    dent_grain = graph.noise(coordinate, 55)
    scratches, lips = 0.0, 0.0
    batch_base = set(graph.nodes)
    for stroke_index, stroke in enumerate(settings["scratches"]):
        start, end = Vector(stroke["start"]), Vector(stroke["end"])
        center, normal, _, _ = surface.find_nearest((start + end) / 2)
        if center is None:
            raise ValueError(f"{settings_name}: scratch has no target surface")
        tangent = end - start
        tangent -= normal * tangent.dot(normal)
        if tangent.length < 1e-6:
            raise ValueError(f"{settings_name}: scratch runs perpendicular to its surface")
        start = center - tangent / 2
        direction = tuple(tangent)
        length_squared = sum(v * v for v in direction)
        dot = graph.nodes.new("ShaderNodeVectorMath")
        dot.operation = "DOT_PRODUCT"
        graph.links.new(graph.vector("SUBTRACT", coordinate, tuple(start)), dot.inputs[0])
        dot.inputs[1].default_value = direction
        t = graph.calculate(
            "MINIMUM",
            graph.calculate(
                "MAXIMUM",
                graph.calculate("DIVIDE", dot.outputs["Value"], length_squared),
                0,
            ),
            1,
        )
        scaled = graph.nodes.new("ShaderNodeVectorMath")
        scaled.operation = "SCALE"
        scaled.inputs[0].default_value = direction
        graph.links.new(t, scaled.inputs[3])
        distance = graph.nodes.new("ShaderNodeVectorMath")
        distance.operation = "LENGTH"
        residual = graph.vector(
            "SUBTRACT",
            graph.vector("SUBTRACT", coordinate, tuple(start)),
            scaled.outputs[0],
        )
        depth = graph.nodes.new("ShaderNodeVectorMath")
        depth.operation = "DOT_PRODUCT"
        graph.links.new(residual, depth.inputs[0])
        depth.inputs[1].default_value = normal
        normal_offset = graph.nodes.new("ShaderNodeVectorMath")
        normal_offset.operation = "SCALE"
        normal_offset.inputs[0].default_value = normal
        graph.links.new(depth.outputs["Value"], normal_offset.inputs[3])
        sideways = normal.cross(tangent).normalized()
        wander = graph.nodes.new("ShaderNodeVectorMath")
        wander.operation = "SCALE"
        wander.inputs[0].default_value = sideways
        graph.links.new(
            graph.calculate(
                "MULTIPLY",
                graph.calculate("SUBTRACT", scratch_wander, 0.5),
                stroke["width"] * 1.6,
            ),
            wander.inputs[3],
        )
        graph.links.new(
            graph.vector(
                "SUBTRACT",
                graph.vector("SUBTRACT", residual, normal_offset.outputs[0]),
                wander.outputs[0],
            ),
            distance.inputs[0],
        )
        depth_mask = graph.calculate(
            "MAXIMUM",
            graph.calculate(
                "SUBTRACT",
                1,
                graph.calculate(
                    "DIVIDE",
                    graph.calculate("ABSOLUTE", depth.outputs["Value"]),
                    tangent.length * 0.4 + 0.004,
                ),
            ),
            0,
        )

        width = graph.calculate(
            "MULTIPLY",
            stroke["width"],
            graph.calculate("ADD", 0.18, graph.calculate("MULTIPLY", scratch_width, 1.35)),
        )
        taper = graph.calculate(
            "MAXIMUM",
            graph.calculate("SINE", graph.calculate("MULTIPLY", t, math.pi)),
            0.025,
        )
        width = graph.calculate(
            "MULTIPLY",
            width,
            graph.calculate(
                "MULTIPLY",
                graph.calculate("POWER", taper, 0.45),
                graph.calculate("SUBTRACT", 1, graph.calculate("MULTIPLY", t, 0.38)),
            ),
        )
        broken = graph.calculate(
            "MINIMUM",
            graph.calculate(
                "MAXIMUM",
                graph.calculate(
                    "MULTIPLY",
                    graph.calculate("SUBTRACT", scratch_breaks, 0.34),
                    5,
                ),
                0,
            ),
            1,
        )
        depth_mask = graph.calculate("MULTIPLY", depth_mask, broken)
        groove = graph.calculate(
            "MAXIMUM",
            graph.calculate(
                "SUBTRACT",
                1,
                graph.calculate("DIVIDE", distance.outputs["Value"], width),
            ),
            0,
        )
        lip = graph.calculate(
            "MAXIMUM",
            graph.calculate(
                "SUBTRACT",
                1,
                graph.calculate(
                    "DIVIDE",
                    distance.outputs["Value"],
                    graph.calculate("MULTIPLY", width, 2.1),
                ),
            ),
            0,
        )
        scratches = graph.calculate(
            "MAXIMUM",
            scratches,
            graph.calculate("MULTIPLY", graph.calculate("POWER", groove, 0.65), depth_mask),
        )
        lips = graph.calculate("MAXIMUM", lips, graph.calculate("MULTIPLY", lip, depth_mask))
        if (stroke_index + 1) % 20 == 0 or stroke_index + 1 == len(settings["scratches"]):
            # Small mask passes stay within Cycles' shader stack limit.
            mask = bpy.data.images.new(
                f"{model.stem}-scratch-mask-{len(scratch_images)}",
                width=RESOLUTION,
                height=RESOLUTION,
                alpha=False,
                float_buffer=True,
            )
            mask.colorspace_settings.name = "Non-Color"
            target = graph.nodes.new("ShaderNodeTexImage")
            target.image = mask
            graph.nodes.active = target
            channels = graph.nodes.new("ShaderNodeCombineColor")
            graph.connect(scratches, channels.inputs["Red"])
            graph.connect(lips, channels.inputs["Green"])
            emission = graph.nodes.new("ShaderNodeEmission")
            output = graph.nodes.new("ShaderNodeOutputMaterial")
            graph.links.new(channels.outputs[0], emission.inputs["Color"])
            graph.links.new(emission.outputs[0], output.inputs["Surface"])
            scene = bpy.context.scene
            scene.render.engine = "CYCLES"
            scene.cycles.samples = 1
            scene.render.bake.margin = 12
            scene.render.bake.use_selected_to_active = False
            scene.render.bake.use_clear = True
            print(
                f"Baking {model.stem} scratch mask {len(scratch_images) + 1}",
                flush=True,
            )
            bpy.ops.object.bake(type="EMIT")
            scratch_images.append(mask)
            for node in set(graph.nodes) - batch_base:
                graph.nodes.remove(node)
            scratches, lips = 0.0, 0.0
    for mask in scratch_images:
        texture = graph.nodes.new("ShaderNodeTexImage")
        texture.image = mask
        texture.extension = "EXTEND"
        channels = graph.nodes.new("ShaderNodeSeparateColor")
        graph.links.new(texture.outputs["Color"], channels.inputs[0])
        scratches = graph.calculate("MAXIMUM", scratches, channels.outputs["Red"])
        lips = graph.calculate("MAXIMUM", lips, channels.outputs["Green"])
    dents = 0.0
    for site in settings["dents"]:
        radius = graph.calculate("SUBTRACT", 1, patch(site))
        bowl = graph.calculate(
            "POWER",
            graph.calculate("SUBTRACT", 1, graph.calculate("MULTIPLY", radius, radius)),
            2,
        )
        dents = graph.calculate(
            "MAXIMUM",
            dents,
            graph.calculate(
                "MULTIPLY",
                bowl,
                graph.calculate("ADD", 0.8, graph.calculate("MULTIPLY", dent_grain, 0.2)),
            ),
        )
    colour = graph.mix(scuffs, base_color, (*settings["scuff_color"], 1))
    if style == "plastic":
        colour = graph.mix(graph.calculate("MULTIPLY", lips, 0.55), colour, (0.91, 0.9, 0.86, 1))
    colour = graph.mix(scratches, colour, (*settings["scratch_color"], 1))
    damage = graph.calculate("MAXIMUM", scuffs, scratches)
    roughness = graph.calculate(
        "ADD",
        base_roughness,
        graph.calculate("MULTIPLY", damage, settings["scuff_roughness"] - base_roughness),
    )
    metallic = base_metallic
    micrograin = graph.noise(
        graph.vector("MULTIPLY", coordinate, (1, 1, 0.035) if style == "metal" else (1, 1, 1)),
        1300,
    )
    relief = graph.calculate(
        "ADD",
        graph.calculate("MULTIPLY", scratches, settings["scratch_depth"]),
        graph.calculate("MULTIPLY", dents, settings["dent_depth"]),
    )
    relief = graph.calculate("ADD", relief, graph.calculate("MULTIPLY", micrograin, settings["grain_depth"]))
    bump = graph.nodes.new("ShaderNodeBump")
    bump.inputs["Distance"].default_value = 1
    bump.invert = True
    graph.links.new(relief, bump.inputs["Height"])
    surface_normal = bump.outputs["Normal"]
    return (colour, roughness, metallic, surface_normal), scratch_images


def bake_maps(mesh, graph, channels, model, style):
    """Bake the albedo, packed AO/roughness/metallic, and tangent normal maps
    beside the model."""
    colour, roughness, metallic, surface_normal = channels
    output_dir = model.with_name(f"{model.stem}_textures")
    ao = graph.nodes.new("ShaderNodeAmbientOcclusion")
    ao.inputs["Distance"].default_value = 0.065
    ao.samples = 16
    packed = graph.nodes.new("ShaderNodeCombineColor")
    graph.links.new(ao.outputs["AO"], packed.inputs["Red"])
    graph.connect(roughness, packed.inputs["Green"])
    graph.connect(metallic, packed.inputs["Blue"])

    shader = graph.nodes.new("ShaderNodeBsdfPrincipled")
    graph.links.new(surface_normal, shader.inputs["Normal"])
    emission = graph.nodes.new("ShaderNodeEmission")
    output = graph.nodes.new("ShaderNodeOutputMaterial")
    target = graph.nodes.new("ShaderNodeTexImage")
    graph.nodes.active = target

    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 16
    scene.render.bake.margin = 12
    scene.render.bake.use_selected_to_active = False
    scene.render.bake.use_clear = True
    output_dir.mkdir(exist_ok=True)
    images = {}
    for channel, source in (
        ("albedo", colour),
        ("metallic-roughness", packed.outputs[0]),
        ("normal-gl", None),
    ):
        if style != "painted":
            scene.cycles.samples = 16 if channel == "metallic-roughness" else 4
        print(f"Baking {model.stem} {channel}", flush=True)
        image = bpy.data.images.new(f"{model.stem}-{channel}", width=RESOLUTION, height=RESOLUTION, alpha=False)
        image.colorspace_settings.name = "sRGB" if channel == "albedo" else "Non-Color"
        target.image = image
        if source is None:
            graph.links.new(shader.outputs["BSDF"], output.inputs["Surface"])
            bpy.ops.object.bake(type="NORMAL")
        else:
            graph.links.new(source, emission.inputs["Color"])
            graph.links.new(emission.outputs[0], output.inputs["Surface"])
            bpy.ops.object.bake(type="EMIT")
        image.filepath_raw = str(output_dir / f"{model.stem}-{channel}.png")
        image.file_format = "PNG"
        image.save()
        image.pack()
        images[channel] = image
    return images


def bind_baked_maps(material, images):
    """Replace the material's node tree with the baked maps."""
    graph = Graph(material)
    graph.nodes.clear()
    shader = graph.nodes.new("ShaderNodeBsdfPrincipled")
    output = graph.nodes.new("ShaderNodeOutputMaterial")
    graph.links.new(shader.outputs[0], output.inputs["Surface"])
    textures = {}
    for channel, image in images.items():
        node = graph.nodes.new("ShaderNodeTexImage")
        node.image = image
        node.extension = "EXTEND"
        textures[channel] = node.outputs["Color"]
    graph.links.new(textures["albedo"], shader.inputs["Base Color"])
    separate = graph.nodes.new("ShaderNodeSeparateColor")
    graph.links.new(textures["metallic-roughness"], separate.inputs[0])
    graph.links.new(separate.outputs["Green"], shader.inputs["Roughness"])
    graph.links.new(separate.outputs["Blue"], shader.inputs["Metallic"])
    occlusion = graph.nodes.new("ShaderNodeGroup")
    occlusion.node_tree = get_settings_group()
    graph.links.new(separate.outputs["Red"], occlusion.inputs["Occlusion"])
    normal = graph.nodes.new("ShaderNodeNormalMap")
    graph.links.new(textures["normal-gl"], normal.inputs["Color"])
    graph.links.new(normal.outputs[0], shader.inputs["Normal"])


def bake_wear(objects, material, settings, model):
    """Join `objects`, bake the configured wear of `material` into its atlas, and
    return the joined mesh."""
    model = Path(model)
    settings_name = model.with_suffix(".json").name
    style = settings.get("style")
    if style not in STYLES:
        raise ValueError(f"{settings_name}: wear.style must be one of {STYLES}")
    if style == "painted":
        validate_painted_wear(settings, settings_name)
        for obj in objects:
            panel_coordinates(obj)
    else:
        validate_surface_wear(settings, settings_name)
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    mesh = bpy.context.object
    mesh.name = "Baked wear surface"
    mesh.data.materials.clear()
    mesh.data.materials.append(material)
    for face in mesh.data.polygons:
        face.material_index = 0
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    if style != "painted":
        # Normal baking and export need matching triangles without losing bevel normals.
        triangulate = mesh.modifiers.new("Bake triangles", "TRIANGULATE")
        triangulate.keep_custom_normals = True
        bpy.ops.object.modifier_apply(modifier=triangulate.name)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(70), island_margin=0.012)
    bpy.ops.object.mode_set(mode="OBJECT")

    shader = material.node_tree.nodes.get("Principled BSDF")
    base = (
        tuple(shader.inputs["Base Color"].default_value),
        shader.inputs["Roughness"].default_value,
        shader.inputs["Metallic"].default_value,
    )
    graph = Graph(material)
    graph.nodes.clear()
    if style == "painted":
        channels, masks = painted_wear_graph(graph, settings, base), []
    else:
        channels, masks = surface_wear_graph(graph, mesh, settings, base, style, model)
    images = bake_maps(mesh, graph, channels, model, style)
    bind_baked_maps(material, images)
    for mask in masks:
        bpy.data.images.remove(mask)
    if style == "painted":
        for name in PANEL_ATTRIBUTES:
            mesh.data.attributes.remove(mesh.data.attributes[name])
    return mesh


def bake_articulated_wear(objects, material, settings, model):
    """Bake `objects` in their assembled rest pose and write the atlas UVs back to
    them."""
    # Copies are baked so independent aim/animation joints never merge.
    bpy.context.view_layer.update()
    copies, corners = [], []
    for obj in objects:
        bpy.context.view_layer.objects.active = obj
        triangulate = obj.modifiers.new("Export triangles", "TRIANGULATE")
        triangulate.keep_custom_normals = True
        bpy.ops.object.modifier_apply(modifier=triangulate.name)
        copied = obj.copy()
        copied.data = obj.data.copy()
        copied.parent = None
        copied.matrix_world = obj.matrix_world.copy()
        bpy.context.collection.objects.link(copied)
        ids = copied.data.attributes.new("BakeCorner", "INT", "CORNER")
        for index, item in enumerate(ids.data):
            item.value = len(corners)
            corners.append((obj, index))
        copies.append(copied)
    visibility = [(obj, obj.hide_render) for obj in objects]
    for obj in objects:
        obj.hide_render = True
    try:
        mesh = bake_wear(copies, material, settings, model)
    finally:
        for obj, hidden in visibility:
            obj.hide_render = hidden
    for obj in objects:
        if obj.data.uv_layers.active is None:
            obj.data.uv_layers.new()
    for loop, identity in zip(mesh.data.uv_layers.active.data, mesh.data.attributes["BakeCorner"].data):
        obj, index = corners[identity.value]
        obj.data.uv_layers.active.data[index].uv = loop.uv
    data = mesh.data
    bpy.data.objects.remove(mesh, do_unlink=True)
    bpy.data.meshes.remove(data)
