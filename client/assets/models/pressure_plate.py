"""Build the pressure plate GLB; -- --preview renders states and color variants."""

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from modelkit import ModelMaterials, plain_material, preview, primitives

MODELS = Path(__file__).resolve().parent
FPS = 30
ACTIVATION_FRAMES = 9
PANEL_TRAVEL = 0.03
PREVIEW_COLORS = (
    ("BLUE", (0.025, 0.32, 0.5)),
    ("AMBER", (0.8, 0.31, 0.025)),
    ("VIOLET", (0.35, 0.09, 0.55)),
)


def box(name, pos, size, mat, parent, bevel=0.003):
    return primitives.box(name, pos, size, mat, primitives.child_of(parent), bevel, 2, "all")


def outline(side, cut):
    h = side / 2
    return [
        (-h + cut, -h),
        (h - cut, -h),
        (h, -h + cut),
        (h, h - cut),
        (h - cut, h),
        (-h + cut, h),
        (-h, h - cut),
        (-h, -h + cut),
    ]


def shell(name, rings, mat, parent, bevel=0.003, closed=True):
    vertices = [(x, y, z) for side, cut, z in rings for x, y in outline(side, cut)]
    faces = []
    for ring in range(len(rings) if closed else len(rings) - 1):
        following = (ring + 1) % len(rings)
        for i in range(8):
            j = (i + 1) % 8
            faces.append((ring * 8 + i, ring * 8 + j, following * 8 + j, following * 8 + i))
    if not closed:
        faces.extend((tuple(reversed(range(8))), tuple(range(len(vertices) - 8, len(vertices)))))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return primitives.finish(obj, name, mat, primitives.child_of(parent), bevel, 2, "all")


def puck(name, x, y, z, radius, depth, mat, parent, vertices=24):
    return primitives.cylinder(
        name,
        (x, y, z),
        radius,
        depth,
        mat,
        primitives.child_of(parent),
        "Z",
        vertices,
        0.001,
        2,
        "quads",
    )


def stroke(name, points, width, z, mat, parent, cyclic=False):
    curve = bpy.data.curves.new(name, "CURVE")
    curve.dimensions = "3D"
    curve.resolution_u = 1
    curve.bevel_depth = width / 2
    curve.bevel_resolution = 1
    spline = curve.splines.new("POLY")
    spline.points.add(len(points) - 1)
    for point, (x, y) in zip(spline.points, points):
        point.co = (x, y, z, 1)
    spline.use_cyclic_u = cyclic
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.convert(target="MESH")
    obj = bpy.context.object
    obj.data.materials.append(mat)
    obj.parent = parent
    return obj


def join_parts(parent, name):
    objects = [obj for obj in parent.children if obj.type == "MESH"]
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    obj = bpy.context.object
    obj.name = name
    obj.data.name = name
    modifier = obj.modifiers.new("Triangles", "TRIANGULATE")
    bpy.ops.object.modifier_apply(modifier=modifier.name)
    return obj


def indicator(index, root, dark, edge, light, marking):
    angle = index * math.pi / 2
    mount = primitives.empty(
        f"PressurePlateIndicator{index + 1}",
        (0.444 * math.sin(angle), -0.444 * math.cos(angle), 0.066),
        root,
    )
    mount.rotation_euler.z = angle
    status = primitives.empty(f"PressurePlateStatus{index + 1}", parent=mount)
    for sign in (-1, 1):
        box(
            "Status bezel end",
            (sign * 0.069, 0, 0.002),
            (0.015, 0.071, 0.021),
            edge,
            mount,
            0.004,
        )
        box(
            "Status bezel rail",
            (0, sign * 0.032, 0.002),
            (0.137, 0.009, 0.018),
            dark,
            mount,
            0.003,
        )
    box("Status drum", (0, 0, 0), (0.117, 0.047, 0.024), dark, status, 0.005)
    stroke(
        "Inactive O",
        [(0.012 * math.cos(t), 0.014 * math.sin(t)) for t in [math.tau * i / 32 for i in range(32)]],
        0.0035,
        0.014,
        marking,
        status,
        True,
    )
    box("Active I", (0, 0, -0.014), (0.007, 0.028, 0.003), light, status, 0.001)
    for x in (-0.043, 0.043):
        box(
            "Active status segment",
            (x, 0, -0.014),
            (0.012, 0.026, 0.003),
            light,
            status,
            0.002,
        )
    join_parts(mount, f"IndicatorFrame{index + 1}")
    join_parts(status, f"StatusDrum{index + 1}")
    return status


def build():
    palette = ModelMaterials(MODELS / "pressure_plate.json")
    root = primitives.empty("PressurePlate")
    panel = primitives.empty("PressurePlatePanel", parent=root)
    shutter = primitives.empty("PressurePlateShutters", parent=root)
    dark, metal, edge = palette["dark"], palette["housing"], palette["edge"]
    accent, light, marking = palette["accent"], palette["light"], palette["marking"]

    shell("Base gasket", [(0.97, 0.105, 0), (0.98, 0.11, 0.018)], dark, root, closed=False)
    shell(
        "Cast housing",
        [
            (1, 0.11, 0.018),
            (1, 0.11, 0.034),
            (0.968, 0.104, 0.064),
            (0.79, 0.08, 0.064),
            (0.79, 0.08, 0.027),
        ],
        metal,
        root,
    )
    shell(
        "Machined inner lip",
        [
            (0.818, 0.085, 0.063),
            (0.818, 0.085, 0.066),
            (0.788, 0.078, 0.066),
            (0.788, 0.078, 0.055),
        ],
        edge,
        root,
        0.001,
    )
    shell(
        "Panel well",
        [(0.787, 0.078, 0.018), (0.787, 0.078, 0.04)],
        dark,
        root,
        closed=False,
    )
    shell(
        "Floating platen edge",
        [
            (0.756, 0.075, 0.048),
            (0.756, 0.075, 0.085),
            (0.748, 0.073, 0.091),
        ],
        edge,
        panel,
        closed=False,
    )
    shell(
        "Colored tread plate",
        [
            (0.739, 0.072, 0.089),
            (0.739, 0.072, 0.098),
            (0.715, 0.068, 0.103),
        ],
        accent,
        panel,
        closed=False,
    )

    for side in (0.63, 0.51, 0.39, 0.27):
        cut = side * 0.14
        shell(
            "Recessed grip channel",
            [
                (side, cut, 0.102),
                (side, cut, 0.104),
                (side - 0.018, cut - 0.004, 0.104),
                (side - 0.018, cut - 0.004, 0.102),
            ],
            dark,
            panel,
            0.0005,
        )
        shell(
            "Grip rib",
            [
                (side - 0.003, cut, 0.103),
                (side - 0.003, cut, 0.106),
                (side - 0.009, cut - 0.001, 0.106),
                (side - 0.009, cut - 0.001, 0.103),
            ],
            metal,
            panel,
            0.0005,
        )

    for x in (-0.405, 0.405):
        for y in (-0.405, 0.405):
            puck("Fastener seat", x, y, 0.064, 0.021, 0.006, dark, root)
            puck("Mounting screw", x, y, 0.068, 0.014, 0.006, edge, root)
            for size in ((0.015, 0.003, 0.001), (0.003, 0.015, 0.001)):
                box("Cross recess", (x, y, 0.0715), size, dark, root, 0)

    for axis in (0, 1):
        for sign in (-1, 1):
            length = 0.19
            for center in (-0.235, 0.235):
                pos = [center, center, 0.065]
                pos[axis] = sign * 0.443
                pos[1 - axis] = center
                size = [length, length, 0.008]
                size[axis] = 0.04
                box("Light socket", pos, size, dark, root, 0.005)
                size[axis], size[1 - axis], size[2] = 0.016, length - 0.02, 0.003
                pos[2] = 0.070
                box("Active strip", pos, size, light, root, 0.003)
                size[axis], size[1 - axis], size[2] = 0.033, length - 0.008, 0.003
                pos[2] = 0.074
                box("Indicator cover", pos, size, dark, shutter, 0.002)

    indicators = [indicator(i, root, dark, edge, light, marking) for i in range(4)]

    for parent, name in (
        (root, "Housing"),
        (panel, "Platen"),
        (shutter, "LightCovers"),
    ):
        join_parts(parent, name)
    return root, panel, shutter, indicators


def animate(panel, shutter, indicators):
    scene = bpy.context.scene
    scene.name = "Activate"
    scene.render.fps = FPS
    scene.frame_start = 0
    scene.frame_end = ACTIVATION_FRAMES
    for frame in range(ACTIVATION_FRAMES + 1):
        t = frame / ACTIVATION_FRAMES
        ease = t * t * (3 - 2 * t)
        panel.location.z = -PANEL_TRAVEL * ease
        shutter.location.z = -0.014 * ease
        panel.keyframe_insert(data_path="location", frame=frame)
        shutter.keyframe_insert(data_path="location", frame=frame)
        for status in indicators:
            status.rotation_euler.x = math.pi * ease
            status.keyframe_insert(data_path="rotation_euler", frame=frame)
    scene.frame_set(0)


def export(path):
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        export_animations=True,
        export_animation_mode="SCENE",
        export_anim_scene_split_object=False,
        export_frame_range=True,
        export_cameras=False,
        export_lights=False,
    )
    print(f"Exported {path.name}: {path.stat().st_size:,} bytes", flush=True)


def pose_import(path, frame, offset, color=None):
    before = set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    imported = set(bpy.context.scene.objects) - before
    bpy.context.scene.frame_set(frame)
    for obj in imported:
        if obj.animation_data:
            transform = obj.matrix_basis.copy()
            obj.animation_data_clear()
            obj.matrix_basis = transform
        if obj.parent is None:
            obj.location += Vector(offset)
    if color is not None:
        accents = {
            slot.material
            for obj in imported
            if obj.type == "MESH"
            for slot in obj.material_slots
            if slot.material.name.startswith("PressurePlateAccent")
        }
        for mat in accents:
            mat.diffuse_color = (*color, 1)
            mat.node_tree.nodes.get("Principled BSDF").inputs["Base Color"].default_value = (*color, 1)
    return imported


def caption(text, pos, size, material):
    return primitives.label(text, pos, size, (0, 0, 0), material, lambda obj: None, 0)


def render_preview(path):
    preview.clear_scene()
    scene = bpy.context.scene
    scene.render.fps = FPS
    ink = plain_material("Preview ink", (0.5, 0.6, 0.66), roughness=1)
    for column, (name, color) in enumerate(PREVIEW_COLORS):
        x = (column - 1) * 1.38
        for state, y, frame in (
            ("INACTIVE", 0.75, 0),
            ("ACTIVE", -0.85, ACTIVATION_FRAMES),
        ):
            pose_import(path, frame, (x, y, 0), color)
            caption(f"{name} / {state}", (x, y - 0.70, 0.002), 0.085, ink)
    floor = plain_material("Preview ground", (0.035, 0.047, 0.057), roughness=0.87)
    preview.floor(floor)
    camera = preview.studio(scene, (0, 0, 0), 3.7)
    preview.aim(camera, (0.5, -5.5, 7.7), (0, -0.12, 0))
    camera.data.ortho_scale = 4.65
    scene.render.resolution_x, scene.render.resolution_y = 1700, 1400
    preview.render(scene, "/tmp/pressure-plate-preview.png")

    preview.clear_scene()
    for state, x, frame in (
        ("INACTIVE", -0.66, 0),
        ("ACTIVE", 0.66, ACTIVATION_FRAMES),
    ):
        pose_import(path, frame, (x, 0, 0))
        caption(state, (x, -0.72, 0.002), 0.11, ink)
    preview.floor(floor)
    camera = preview.studio(scene, (0, 0, 0), 2.3)
    preview.aim(camera, (1.0, -3.4, 2.5), (0, -0.05, 0))
    camera.data.ortho_scale = 2.85
    scene.render.resolution_x, scene.render.resolution_y = 1600, 900
    preview.render(scene, "/tmp/pressure-plate-detail.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", action="store_true")
    arguments = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else [])
    preview.clear_scene()
    _, panel, shutter, indicators = build()
    animate(panel, shutter, indicators)
    path = MODELS / "pressure_plate.glb"
    export(path)
    if arguments.preview:
        render_preview(path)


if __name__ == "__main__":
    main()
