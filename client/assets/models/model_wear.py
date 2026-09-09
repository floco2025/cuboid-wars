"""Bake material-appropriate surface wear into a model-specific UV atlas."""

import math
from pathlib import Path

import bpy
from io_scene_gltf2.blender.imp.pbrMetallicRoughness import get_settings_group
from mathutils import Vector
from mathutils.bvhtree import BVHTree

RESOLUTION = 2048


def remember_panel_coordinates(obj):
    low = [min(vertex.co[i] for vertex in obj.data.vertices) for i in range(3)]
    high = [max(vertex.co[i] for vertex in obj.data.vertices) for i in range(3)]
    center = [(a + b) / 2 for a, b in zip(low, high)]
    half = [(b - a) / 2 for a, b in zip(low, high)]
    for name, values in (
        (
            "PanelPosition",
            [
                tuple(vertex.co[i] - center[i] for i in range(3))
                for vertex in obj.data.vertices
            ],
        ),
        ("PanelHalfSize", [half] * len(obj.data.vertices)),
    ):
        attribute = obj.data.attributes.new(name, "FLOAT_VECTOR", "POINT")
        for item, value in zip(attribute.data, values):
            item.vector = value


def validate_surface_settings(settings, name):
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
            and all(
                isinstance(v, (int, float))
                and not isinstance(v, bool)
                and math.isfinite(v)
                for v in value
            )
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
                raise ValueError(
                    f"{name}: wear.{field} requires centers and positive extents"
                )
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
            raise ValueError(
                f"{name}: scratches require distinct endpoints and positive widths"
            )


def bake_armour(objects, material, settings, model):
    model = Path(model)
    output_dir = model.with_name(f"{model.stem}_textures")
    settings_name = model.with_suffix(".json").name
    style = settings.get("style", "painted")
    if style not in {"painted", "plastic", "metal"}:
        raise ValueError(f"{settings_name}: unknown wear style {style}")
    if style == "painted":
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
        optional_fields = {"chip_depth", "style"}
        if settings.keys() - optional_fields != scalar_fields | color_fields | {
            "chip_sites"
        }:
            raise ValueError(f"{settings_name}: wear fields missing or unknown")
        for key in scalar_fields | ({"chip_depth"} & settings.keys()):
            value = settings[key]
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or not math.isfinite(value)
                or value < 0
            ):
                raise ValueError(
                    f"{settings_name}: wear.{key} must be nonnegative and finite"
                )
        for key in color_fields:
            value = settings[key]
            if len(value) != 3 or any(
                not math.isfinite(v) or not 0 <= v <= 1 for v in value
            ):
                raise ValueError(f"{settings_name}: wear.{key} must be an RGB color")
        for site in settings["chip_sites"]:
            if site.keys() != {"center", "extent"} or any(
                len(site[k]) != 3 for k in site
            ):
                raise ValueError(
                    f"{settings_name}: each chip site needs center and extent vectors"
                )
            if any(not math.isfinite(v) for k in site for v in site[k]) or any(
                v <= 0 for v in site["extent"]
            ):
                raise ValueError(
                    f"{settings_name}: chip coordinates must be finite and extents positive"
                )
    else:
        validate_surface_settings(settings, settings_name)
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
    if style != "painted":
        # Normal baking and export need matching triangles without losing bevel normals.
        triangulate = mesh.modifiers.new("Bake triangles", "TRIANGULATE")
        triangulate.keep_custom_normals = True
        bpy.ops.object.modifier_apply(modifier=triangulate.name)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(70), island_margin=0.012)
    bpy.ops.object.mode_set(mode="OBJECT")

    nodes, links = material.node_tree.nodes, material.node_tree.links
    base_shader = nodes.get("Principled BSDF")
    base_color = tuple(base_shader.inputs["Base Color"].default_value)
    base_roughness = base_shader.inputs["Roughness"].default_value
    base_metallic = base_shader.inputs["Metallic"].default_value
    nodes.clear()
    scratch_images = []

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

    if style == "painted":
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
        colour = mix(mottling, base_color, (*settings["paint_variation_color"], 1))
        colour = mix(wear, colour, (*settings["exposed_metal_color"], 1))
        roughness = calculate(
            "ADD",
            base_roughness,
            calculate("MULTIPLY", mottling, settings["roughness_variation"]),
        )
        roughness = calculate(
            "SUBTRACT",
            roughness,
            calculate("MULTIPLY", wear, settings["wear_roughness_reduction"]),
        )
        metallic = calculate(
            "ADD",
            base_metallic,
            calculate("MULTIPLY", wear, settings["exposed_metallic"] - base_metallic),
        )
        bump = nodes.new("ShaderNodeBump")
        bump.inputs["Distance"].default_value = settings["scratch_depth"]
        bump.inputs["Strength"].default_value = settings["normal_strength"]
        bump.invert = True
        links.new(
            calculate(
                "MAXIMUM",
                scratches,
                calculate("MULTIPLY", noise(coordinate, 650), 0.12),
            ),
            bump.inputs["Height"],
        )
        paint_edge = nodes.new("ShaderNodeBump")
        paint_edge.inputs["Distance"].default_value = settings.get("chip_depth", 0)
        paint_edge.invert = True
        links.new(wear, paint_edge.inputs["Height"])
        links.new(bump.outputs["Normal"], paint_edge.inputs["Normal"])
        surface_normal = paint_edge.outputs["Normal"]
    else:
        geometry = nodes.new("ShaderNodeNewGeometry")
        coordinate = geometry.outputs["Position"]
        surface = BVHTree.FromPolygons(
            [v.co for v in mesh.data.vertices],
            [p.vertices for p in mesh.data.polygons],
        )

        def patch(site):
            center, _, _, _ = surface.find_nearest(Vector(site["center"]))
            if center is None:
                raise ValueError(f"{settings_name}: wear patch has no target surface")
            radial = nodes.new("ShaderNodeVectorMath")
            radial.operation = "LENGTH"
            links.new(
                vector(
                    "MULTIPLY",
                    vector("SUBTRACT", coordinate, tuple(center)),
                    tuple(1 / v for v in site["extent"]),
                ),
                radial.inputs[0],
            )
            return calculate(
                "MAXIMUM", calculate("SUBTRACT", 1, radial.outputs["Value"]), 0
            )

        scuffs = 0.0
        for site in settings["scuff_sites"]:
            scuffs = calculate(
                "MAXIMUM", scuffs, calculate("MULTIPLY", patch(site), 2.5)
            )
        scuffs = calculate("MINIMUM", scuffs, 1)
        scuffs = calculate(
            "MULTIPLY",
            scuffs,
            calculate("ADD", 0.4, calculate("MULTIPLY", noise(coordinate, 160), 0.6)),
        )
        scratch_wander = noise(coordinate, 115)
        scratch_width = noise(coordinate, 210)
        scratch_breaks = noise(coordinate, 130)
        dent_grain = noise(coordinate, 55)
        scratches, lips = 0.0, 0.0
        batch_base = set(nodes)
        for stroke_index, stroke in enumerate(settings["scratches"]):
            start, end = Vector(stroke["start"]), Vector(stroke["end"])
            center, normal, _, _ = surface.find_nearest((start + end) / 2)
            if center is None:
                raise ValueError(f"{settings_name}: scratch has no target surface")
            tangent = end - start
            tangent -= normal * tangent.dot(normal)
            if tangent.length < 1e-6:
                raise ValueError(
                    f"{settings_name}: scratch runs perpendicular to its surface"
                )
            start = center - tangent / 2
            direction = tuple(tangent)
            length_squared = sum(v * v for v in direction)
            dot = nodes.new("ShaderNodeVectorMath")
            dot.operation = "DOT_PRODUCT"
            links.new(vector("SUBTRACT", coordinate, tuple(start)), dot.inputs[0])
            dot.inputs[1].default_value = direction
            t = calculate(
                "MINIMUM",
                calculate(
                    "MAXIMUM",
                    calculate("DIVIDE", dot.outputs["Value"], length_squared),
                    0,
                ),
                1,
            )
            scaled = nodes.new("ShaderNodeVectorMath")
            scaled.operation = "SCALE"
            scaled.inputs[0].default_value = direction
            links.new(t, scaled.inputs[3])
            distance = nodes.new("ShaderNodeVectorMath")
            distance.operation = "LENGTH"
            residual = vector(
                "SUBTRACT",
                vector("SUBTRACT", coordinate, tuple(start)),
                scaled.outputs[0],
            )
            depth = nodes.new("ShaderNodeVectorMath")
            depth.operation = "DOT_PRODUCT"
            links.new(residual, depth.inputs[0])
            depth.inputs[1].default_value = normal
            normal_offset = nodes.new("ShaderNodeVectorMath")
            normal_offset.operation = "SCALE"
            normal_offset.inputs[0].default_value = normal
            links.new(depth.outputs["Value"], normal_offset.inputs[3])
            sideways = normal.cross(tangent).normalized()
            wander = nodes.new("ShaderNodeVectorMath")
            wander.operation = "SCALE"
            wander.inputs[0].default_value = sideways
            links.new(
                calculate(
                    "MULTIPLY",
                    calculate("SUBTRACT", scratch_wander, 0.5),
                    stroke["width"] * 1.6,
                ),
                wander.inputs[3],
            )
            links.new(
                vector(
                    "SUBTRACT",
                    vector("SUBTRACT", residual, normal_offset.outputs[0]),
                    wander.outputs[0],
                ),
                distance.inputs[0],
            )
            depth_mask = calculate(
                "MAXIMUM",
                calculate(
                    "SUBTRACT",
                    1,
                    calculate(
                        "DIVIDE",
                        calculate("ABSOLUTE", depth.outputs["Value"]),
                        tangent.length * 0.4 + 0.004,
                    ),
                ),
                0,
            )

            width = calculate(
                "MULTIPLY",
                stroke["width"],
                calculate("ADD", 0.18, calculate("MULTIPLY", scratch_width, 1.35)),
            )
            taper = calculate(
                "MAXIMUM", calculate("SINE", calculate("MULTIPLY", t, math.pi)), 0.025
            )
            width = calculate(
                "MULTIPLY",
                width,
                calculate(
                    "MULTIPLY",
                    calculate("POWER", taper, 0.45),
                    calculate("SUBTRACT", 1, calculate("MULTIPLY", t, 0.38)),
                ),
            )
            broken = calculate(
                "MINIMUM",
                calculate(
                    "MAXIMUM",
                    calculate(
                        "MULTIPLY",
                        calculate("SUBTRACT", scratch_breaks, 0.34),
                        5,
                    ),
                    0,
                ),
                1,
            )
            depth_mask = calculate("MULTIPLY", depth_mask, broken)
            groove = calculate(
                "MAXIMUM",
                calculate(
                    "SUBTRACT", 1, calculate("DIVIDE", distance.outputs["Value"], width)
                ),
                0,
            )
            lip = calculate(
                "MAXIMUM",
                calculate(
                    "SUBTRACT",
                    1,
                    calculate(
                        "DIVIDE",
                        distance.outputs["Value"],
                        calculate("MULTIPLY", width, 2.1),
                    ),
                ),
                0,
            )
            scratches = calculate(
                "MAXIMUM",
                scratches,
                calculate("MULTIPLY", calculate("POWER", groove, 0.65), depth_mask),
            )
            lips = calculate("MAXIMUM", lips, calculate("MULTIPLY", lip, depth_mask))
            if (stroke_index + 1) % 20 == 0 or stroke_index + 1 == len(
                settings["scratches"]
            ):
                # Small mask passes stay within Cycles' shader stack limit.
                mask = bpy.data.images.new(
                    f"{model.stem}-scratch-mask-{len(scratch_images)}",
                    width=RESOLUTION,
                    height=RESOLUTION,
                    alpha=False,
                    float_buffer=True,
                )
                mask.colorspace_settings.name = "Non-Color"
                target = nodes.new("ShaderNodeTexImage")
                target.image = mask
                nodes.active = target
                channels = nodes.new("ShaderNodeCombineColor")
                connect(scratches, channels.inputs["Red"])
                connect(lips, channels.inputs["Green"])
                emission = nodes.new("ShaderNodeEmission")
                output = nodes.new("ShaderNodeOutputMaterial")
                links.new(channels.outputs[0], emission.inputs["Color"])
                links.new(emission.outputs[0], output.inputs["Surface"])
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
                for node in set(nodes) - batch_base:
                    nodes.remove(node)
                scratches, lips = 0.0, 0.0
        for mask in scratch_images:
            texture = nodes.new("ShaderNodeTexImage")
            texture.image = mask
            texture.extension = "EXTEND"
            channels = nodes.new("ShaderNodeSeparateColor")
            links.new(texture.outputs["Color"], channels.inputs[0])
            scratches = calculate("MAXIMUM", scratches, channels.outputs["Red"])
            lips = calculate("MAXIMUM", lips, channels.outputs["Green"])
        dents = 0.0
        for site in settings["dents"]:
            radius = calculate("SUBTRACT", 1, patch(site))
            bowl = calculate(
                "POWER",
                calculate("SUBTRACT", 1, calculate("MULTIPLY", radius, radius)),
                2,
            )
            dents = calculate(
                "MAXIMUM",
                dents,
                calculate(
                    "MULTIPLY",
                    bowl,
                    calculate("ADD", 0.8, calculate("MULTIPLY", dent_grain, 0.2)),
                ),
            )
        colour = mix(scuffs, base_color, (*settings["scuff_color"], 1))
        if style == "plastic":
            colour = mix(
                calculate("MULTIPLY", lips, 0.55), colour, (0.91, 0.9, 0.86, 1)
            )
        colour = mix(scratches, colour, (*settings["scratch_color"], 1))
        damage = calculate("MAXIMUM", scuffs, scratches)
        roughness = calculate(
            "ADD",
            base_roughness,
            calculate("MULTIPLY", damage, settings["scuff_roughness"] - base_roughness),
        )
        metallic = base_metallic
        micrograin = noise(
            vector(
                "MULTIPLY", coordinate, (1, 1, 0.035) if style == "metal" else (1, 1, 1)
            ),
            1300,
        )
        relief = calculate(
            "ADD",
            calculate("MULTIPLY", scratches, settings["scratch_depth"]),
            calculate("MULTIPLY", dents, settings["dent_depth"]),
        )
        relief = calculate(
            "ADD", relief, calculate("MULTIPLY", micrograin, settings["grain_depth"])
        )
        bump = nodes.new("ShaderNodeBump")
        bump.inputs["Distance"].default_value = 1
        bump.invert = True
        links.new(relief, bump.inputs["Height"])
        surface_normal = bump.outputs["Normal"]

    ao = nodes.new("ShaderNodeAmbientOcclusion")
    ao.inputs["Distance"].default_value = 0.065
    ao.samples = 16
    packed = nodes.new("ShaderNodeCombineColor")
    links.new(ao.outputs["AO"], packed.inputs["Red"])
    connect(roughness, packed.inputs["Green"])
    connect(metallic, packed.inputs["Blue"])

    shader = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(surface_normal, shader.inputs["Normal"])
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
        image = bpy.data.images.new(
            f"{model.stem}-{channel}", width=RESOLUTION, height=RESOLUTION, alpha=False
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
        image.filepath_raw = str(output_dir / f"{model.stem}-{channel}.png")
        image.file_format = "PNG"
        image.save()
        image.pack()
        images[channel] = image

    nodes.clear()
    for mask in scratch_images:
        bpy.data.images.remove(mask)
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


def bake_articulated_armour(objects, material, settings, model):
    # Bake in the assembled rest pose without merging independent aim/animation joints.
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
        mesh = bake_armour(copies, material, settings, model)
    finally:
        for obj, hidden in visibility:
            obj.hide_render = hidden
    for loop, identity in zip(
        mesh.data.uv_layers.active.data, mesh.data.attributes["BakeCorner"].data
    ):
        obj, index = corners[identity.value]
        obj.data.uv_layers.active.data[index].uv = loop.uv
    for obj in objects:
        for name in ("PanelPosition", "PanelHalfSize"):
            obj.data.attributes.remove(obj.data.attributes[name])
    data = mesh.data
    bpy.data.objects.remove(mesh, do_unlink=True)
    bpy.data.meshes.remove(data)
