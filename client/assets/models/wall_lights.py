"""Build both wall lights: blender --background --python client/assets/models/wall_lights.py."""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model_materials import catalog_material, project_uv

MODELS = Path(__file__).resolve().parent


def material(name, color, metallic=0.0, roughness=0.4, emission=0.0):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    shader = mat.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = (*color, 1)
    shader.inputs["Metallic"].default_value = metallic
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Emission Color"].default_value = (*color, 1)
    shader.inputs["Emission Strength"].default_value = emission
    return mat


def finish(obj, name, mat, bevel):
    obj.name = name
    obj.data.materials.append(mat)
    if bevel:
        modifier = obj.modifiers.new("Edge radii", "BEVEL")
        modifier.width = bevel
        modifier.segments = 3
        obj.modifiers.new("Surface normals", "WEIGHTED_NORMAL")
    project_uv(obj, mat)
    for face in obj.data.polygons:
        face.use_smooth = True
    return obj


def box(name, pos, size, mat, bevel=0.015):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    obj = bpy.context.object
    obj.dimensions = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, name, mat, bevel)


def rod(name, start, end, radius, mat, vertices=24):
    direction = Vector(end) - Vector(start)
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices,
        radius=radius,
        depth=direction.length,
        location=(Vector(start) + Vector(end)) / 2,
    )
    obj = bpy.context.object
    obj.rotation_euler = direction.to_track_quat("Z", "Y").to_euler()
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, name, mat, 0.003)


def decorative():
    metal = catalog_material(
        "brushed-metal",
        "Champagne brushed alloy",
        tuning={
            "tint": [0.85, 0.64, 0.35],
            "normal_strength": 0.25,
            "color_contrast": 0.3,
        },
    )
    dark = material("Graphite mounting plate", (0.025, 0.035, 0.045), 0.7)
    porcelain = material("Ivory end caps", (0.77, 0.74, 0.65), 0.15, 0.27)
    glow = material("Warm opal diffuser", (1.0, 0.86, 0.65), roughness=0.3, emission=8)
    box("Wall mounting plate", (0, -0.025, 0), (0.22, 0.05, 0.74), dark, 0.05)
    box("Champagne backplate", (0, -0.06, 0), (0.19, 0.035, 0.58), metal, 0.035)
    box("Opal lantern", (0, -0.155, 0), (0.21, 0.16, 0.48), glow, 0.065)
    for sign in (-1, 1):
        box(
            "Porcelain crown",
            (0, -0.153, sign * 0.275),
            (0.26, 0.20, 0.09),
            porcelain,
            0.035,
        )
        box(
            "Crown metal trim",
            (0, -0.153, sign * 0.235),
            (0.265, 0.205, 0.026),
            metal,
            0.012,
        )
        for index in range(3):
            x = sign * (0.135 + index * 0.032)
            height = 0.59 - index * 0.08
            fin = box(
                "Stepped deco wing",
                (x, -0.10 + index * 0.012, 0.025),
                (0.022, 0.095, height),
                metal,
                0.008,
            )
            fin.rotation_euler.y = -sign * 0.10
    box("Central lantern spine", (0, -0.242, 0), (0.018, 0.018, 0.50), metal, 0.006)
    for z in (-0.347, 0.347):
        rod("Mount fastener", (0, -0.03, z), (0, -0.067, z), 0.019, metal)


def capsule_outline(width, height):
    radius = width / 2
    straight = height / 2 - radius
    return [
        (radius * math.cos(angle), sign * straight + radius * math.sin(angle))
        for sign, offset in [(1, 0), (-1, math.pi)]
        for angle in [offset + math.pi * i / 24 for i in range(25)]
    ]


def capsule_body(name, width, height, rings, mat):
    outline = capsule_outline(width, height)
    vertices = [(x * scale, y, z * scale) for scale, y in rings for x, z in outline]
    n = len(outline)
    faces = [tuple(reversed(range(n)))]
    for ring in range(len(rings) - 1):
        for i in range(n):
            j = (i + 1) % n
            faces.append(
                (ring * n + i, ring * n + j, (ring + 1) * n + j, (ring + 1) * n + i)
            )
    faces.append(tuple(range((len(rings) - 1) * n, len(rings) * n)))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return finish(obj, name, mat, 0.004)


def guard(name, points, radius, mat, cyclic=False):
    curve = bpy.data.curves.new(name, "CURVE")
    curve.dimensions = "3D"
    curve.bevel_depth = radius
    curve.bevel_resolution = 3
    spline = curve.splines.new("POLY")
    spline.points.add(len(points) - 1)
    for point, pos in zip(spline.points, points):
        point.co = (*pos, 1)
    spline.use_cyclic_u = cyclic
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.convert(target="MESH")
    finish(bpy.context.object, name, mat, 0)


def utility():
    metal = catalog_material(
        "brushed-metal",
        "Cast alloy bulkhead",
        tuning={
            "tint": [0.40, 0.43, 0.44],
            "normal_strength": 0.35,
            "color_contrast": 0.5,
        },
    )
    dark = material("Neoprene lens seal", (0.018, 0.023, 0.027), roughness=0.65)
    guard_metal = material("Steel guard", (0.19, 0.22, 0.23), 0.8, 0.33)
    glow = material(
        "Opal bulkhead diffuser", (0.80, 0.91, 1.0), roughness=0.4, emission=8
    )
    capsule_body(
        "Oval cast housing", 0.44, 0.72, [(1, 0), (1, -0.095), (0.96, -0.125)], metal
    )
    capsule_body("Weather seal", 0.385, 0.655, [(1, -0.115), (1, -0.143)], dark)
    capsule_body(
        "Domed opal lens",
        0.35,
        0.62,
        [(1, -0.14), (0.97, -0.185), (0.78, -0.245), (0.4, -0.278), (0.03, -0.287)],
        glow,
    )
    guard(
        "Cast retaining rim",
        [(x, -0.135, z) for x, z in capsule_outline(0.40, 0.675)],
        0.018,
        metal,
        True,
    )
    for x in (-0.105, 0.105):
        guard(
            "Bowed vertical guard",
            [
                (x, -0.15, -0.315),
                (x, -0.245, -0.27),
                (x, -0.30, -0.17),
                (x, -0.30, 0.17),
                (x, -0.245, 0.27),
                (x, -0.15, 0.315),
            ],
            0.010,
            guard_metal,
        )
    for z in (-0.17, 0.17):
        points = [
            (0.205 * math.cos(angle), -0.14 - 0.17 * math.sin(angle), z)
            for angle in [math.pi * i / 32 for i in range(33)]
        ]
        guard("Curved cross guard", points, 0.010, guard_metal)
    for z in (-0.385, 0.385):
        box("Mounting lug", (0, -0.029, z), (0.10, 0.058, 0.11), metal, 0.025)
        rod("Recessed mount socket", (0, -0.059, z), (0, -0.064, z), 0.021, dark)
        rod("Mounting bolt", (0, -0.065, z), (0, -0.072, z), 0.013, guard_metal, 6)
    for x in (-0.235, 0.235):
        box("Housing latch", (x, -0.07, 0), (0.065, 0.075, 0.10), metal, 0.012)
        rod("Latch screw", (x, -0.11, 0), (x, -0.12, 0), 0.014, guard_metal, 6)


for kind, build in [("decorative", decorative), ("utility", utility)]:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    build()
    bpy.ops.export_scene.gltf(
        filepath=str(MODELS / f"wall_light_{kind}.glb"),
        export_format="GLB",
        export_animations=False,
        export_cameras=False,
        export_lights=False,
    )

bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
for kind, x in [("decorative", -0.65), ("utility", 0.65)]:
    bpy.ops.import_scene.gltf(filepath=str(MODELS / f"wall_light_{kind}.glb"))
    for obj in bpy.context.selected_objects:
        if obj.parent is None:
            obj.location.x += x
wall = material("Preview wall", (0.12, 0.14, 0.16), roughness=0.9)
box("Studio wall", (0, 0.075, 0), (200, 0.15, 200), wall, 0)
scene = bpy.context.scene
scene.render.engine = "CYCLES"
scene.cycles.samples = 48
scene.world.color = (0.20, 0.20, 0.20)
for pos, power, size in [((-2, -3, 3), 350, 3), ((2, -2, 1), 180, 2)]:
    bpy.ops.object.light_add(type="AREA", location=pos)
    light = bpy.context.object
    light.data.energy = power
    light.data.shape = "DISK"
    light.data.size = size
    light.rotation_euler = (-light.location).to_track_quat("-Z", "Y").to_euler()
bpy.ops.object.camera_add(location=(1.2, -4.5, 1.4))
scene.camera = bpy.context.object
scene.camera.rotation_euler = (
    (-scene.camera.location).to_track_quat("-Z", "Y").to_euler()
)
scene.camera.data.type = "ORTHO"
scene.camera.data.ortho_scale = 2.8
scene.render.resolution_x = 1400
scene.render.resolution_y = 750
scene.render.resolution_percentage = 100
scene.render.filepath = "/tmp/wall-lights-preview.png"
bpy.ops.render.render(write_still=True)
