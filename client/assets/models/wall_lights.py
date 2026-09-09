"""Build both wall lights in Blender; add -- --preview for a comparison still in /tmp."""

import math
import sys
from pathlib import Path

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from modelkit import ModelMaterials, plain_material, preview, primitives

MODELS = Path(__file__).resolve().parent


def loose(obj):
    """Fixtures export as separate unparented meshes."""


def finish(obj, name, mat, bevel):
    return primitives.finish(obj, name, mat, loose, bevel, 3, "all")


def box(name, pos, size, mat, bevel=0.015):
    return primitives.box(name, pos, size, mat, loose, bevel, 3, "all")


def rod(name, start, end, radius, mat, vertices=24):
    return primitives.rod(
        name, start, end, radius, mat, loose, vertices, 0.003, 3, "all"
    )


def decorative():
    palette = ModelMaterials(MODELS / "wall_light_decorative.json")
    metal = palette["metal"]
    dark = palette["dark"]
    porcelain = palette["porcelain"]
    glow = palette["glow"]
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
    palette = ModelMaterials(MODELS / "wall_light_utility.json")
    metal = palette["metal"]
    dark = palette["dark"]
    guard_metal = palette["guard_metal"]
    glow = palette["glow"]
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
    preview.clear_scene()
    build()
    model = MODELS / f"wall_light_{kind}.glb"
    bpy.ops.export_scene.gltf(
        filepath=str(model),
        export_format="GLB",
        export_animations=False,
        export_cameras=False,
        export_lights=False,
    )
    print(f"Exported {model.name}:", model.stat().st_size, "bytes")

if "--preview" in sys.argv:
    preview.clear_scene()
    for kind, x in [("decorative", -0.65), ("utility", 0.65)]:
        bpy.ops.import_scene.gltf(filepath=str(MODELS / f"wall_light_{kind}.glb"))
        for obj in bpy.context.selected_objects:
            if obj.parent is None:
                obj.location.x += x
    scene = bpy.context.scene
    low, high = preview.visible_bounds(scene)
    camera = preview.studio(scene, (0, 0, 0), max(high - low))
    wall = plain_material("Preview wall", (0.12, 0.14, 0.16), roughness=0.9)
    box("Studio wall", (0, 0.075, 0), (200, 0.15, 200), wall, 0)
    preview.aim(camera, (1.2, -4.5, 1.4), (0, 0, 0))
    camera.data.ortho_scale = 2.8
    scene.render.resolution_x, scene.render.resolution_y = 1400, 750
    preview.render(scene, "/tmp/wall-lights-preview.png")
