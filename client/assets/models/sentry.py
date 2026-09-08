"""Build the sentry GLB in Blender; add -- --preview [--motion] for previews in /tmp."""

import json
import math
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Euler, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model_materials import catalog_material, project_uv

MODEL = Path(__file__).resolve().with_suffix(".glb")
FPS = 30
WHEEL_RADIUS = 0.27
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
bpy.ops.import_scene.gltf(filepath=str(MODEL.with_name("player_robot.glb")))
palette = {
    name: next(m for m in bpy.data.materials if m.name.startswith(name))
    for name in (
        "Satin ceramic-white polymer",
        "Fine matte elastomer",
        "Brushed titanium mechanisms",
        "Graphite structural composite",
        "Muted ochre identification",
        "Smoked optical visor",
        "Ice-blue sensor",
        "Status diode",
        "Graphite service stencil",
    )
}
for image in bpy.data.images:
    if image.type == "IMAGE" and image.size[0]:
        image.pack()
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)
ivory, rubber, steel, graphite, ochre, glass, blue, amber, ink = palette.values()
TEXTURES = MODEL.parent.parent / "textures"


armour = catalog_material("brushed-metal")
steel = armour
edge = steel
rubber = catalog_material("synth-rubber")
ink.node_tree.nodes.get("Principled BSDF").inputs["Base Color"].default_value = (
    0.59,
    0.55,
    0.42,
    1,
)
optic = bpy.data.materials.new("Ember warning optics")
optic.use_nodes = True
shader = optic.node_tree.nodes.get("Principled BSDF")
shader.inputs["Base Color"].default_value = (0.8, 0.055, 0.006, 1)
shader.inputs["Emission Color"].default_value = (1.0, 0.085, 0.008, 1)
shader.inputs["Emission Strength"].default_value = 3.0
optic.diffuse_color = (0.8, 0.055, 0.006, 1)

parts = []


def finish(obj, name, mat, bone, bevel=0, cylindrical=False):
    obj.name = name
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        mod = obj.modifiers.new("Rounded armour", "BEVEL")
        mod.width, mod.segments = bevel, 3 if bevel >= 0.018 else 1
        bpy.ops.object.modifier_apply(modifier=mod.name)
        mod = obj.modifiers.new("Corner normals", "WEIGHTED_NORMAL")
        bpy.ops.object.modifier_apply(modifier=mod.name)
    project_uv(obj, mat, cylindrical)
    group = obj.vertex_groups.new(name=bone)
    group.add(list(range(len(obj.data.vertices))), 1, "REPLACE")
    parts.append(obj)
    return obj


def box(name, pos, size, mat, bone="Hull", bevel=0.018):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    obj = bpy.context.object
    obj.dimensions = size
    return finish(obj, name, mat, bone, bevel)


def cylinder(name, pos, radius, depth, mat, bone="Hull", axis="Z", vertices=32):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=pos
    )
    obj = bpy.context.object
    if axis == "X":
        obj.rotation_euler.y = math.pi / 2
    elif axis == "Y":
        obj.rotation_euler.x = math.pi / 2
    for face in obj.data.polygons:
        face.use_smooth = len(face.vertices) == 4
    return finish(obj, name, mat, bone, 0.006, cylindrical=True)


def sphere(name, pos, size, mat, bone="Hull"):
    bpy.ops.mesh.primitive_uv_sphere_add(
        segments=32, ring_count=16, radius=1, location=pos
    )
    obj = bpy.context.object
    obj.scale = size
    for face in obj.data.polygons:
        face.use_smooth = True
    return finish(obj, name, mat, bone)


def label(text, pos, size, rotation, bone="Hull"):
    bpy.ops.object.text_add(location=pos)
    obj = bpy.context.object
    obj.data.body = text
    obj.data.size = size
    obj.data.align_x = "CENTER"
    obj.data.extrude = 0.0005
    obj.rotation_euler = rotation
    bpy.ops.object.convert(target="MESH")
    return finish(bpy.context.object, text, ink, bone)


bones = [
    ("Root", (0, 0, 0), None),
    ("Hull", (0, 0, 0.67), "Root"),
    ("Sensor", (0, -0.17, 1.13), "Hull"),
]
box("Armoured belly", (0, 0, 0.32), (1.05, 1.14, 0.24), graphite, "Root", 0.055)
box(
    "Structural shoulder frame",
    (0, 0.015, 0.65),
    (0.99, 1.11, 0.39),
    graphite,
    bevel=0.075,
)
box("Gunmetal main glacis", (0, -0.03, 0.84), (0.99, 1.0, 0.38), armour, bevel=0.045)
box("Upper service deck", (0, 0.13, 1.02), (0.79, 0.66, 0.09), armour, bevel=0.025)

for sign in (-1, 1):
    for index, y in enumerate((-0.56, 0, 0.56)):
        bone = ("WheelL" if sign < 0 else "WheelR") + str(index)
        bones.append((bone, (sign * 0.635, y, WHEEL_RADIUS), "Root"))
        cylinder("Drive axle", (sign * 0.50, y, 0.27), 0.085, 0.30, steel, "Root", "X")
        cylinder(
            "Heavy elastomer tyre",
            (sign * 0.635, y, 0.27),
            WHEEL_RADIUS,
            0.215,
            rubber,
            bone,
            "X",
            48,
        )
        cylinder(
            "Hub recess", (sign * 0.749, y, 0.27), 0.18, 0.017, graphite, bone, "X"
        )
        cylinder(
            "Armoured hub", (sign * 0.761, y, 0.27), 0.135, 0.022, armour, bone, "X"
        )
        cylinder("Axle cap", (sign * 0.78, y, 0.27), 0.064, 0.024, steel, bone, "X", 12)
        for angle_index in range(8):
            a = math.tau * angle_index / 8
            cylinder(
                "Hub fastener",
                (sign * 0.78, y + math.sin(a) * 0.104, 0.27 + math.cos(a) * 0.104),
                0.012,
                0.008,
                steel,
                bone,
                "X",
                8,
            )
        for tread in range(32):
            a = math.tau * tread / 32
            for lane in (-1, 1):
                block = box(
                    "Chevron traction cleat",
                    (
                        sign * 0.635 + lane * 0.056,
                        y + math.sin(a) * 0.269,
                        0.27 + math.cos(a) * 0.269,
                    ),
                    (0.104, 0.039, 0.015),
                    rubber,
                    bone,
                    0.003,
                )
                block.rotation_mode = "QUATERNION"
                block.rotation_quaternion = (
                    Euler((-a, 0, 0)).to_quaternion()
                    @ Euler((0, 0, lane * 0.20)).to_quaternion()
                )
                outward = block.rotation_quaternion @ Vector((0, 0, 1))
                assert outward.dot(Vector((0, math.sin(a), math.cos(a)))) > 0.9999
        for z in (0.43, 0.49):
            box(
                "Suspension sleeve",
                (sign * 0.48, y, z),
                (0.05, 0.14, 0.028),
                steel,
                "Root",
                0.006,
            )
    box(
        "Side armour underlay",
        (sign * 0.61, 0.015, 0.65),
        (0.32, 1.36, 0.16),
        armour,
        bevel=0.045,
    )
    box(
        "Steel track guard",
        (sign * 0.61, 0.0, 0.76),
        (0.35, 1.36, 0.19),
        armour,
        bevel=0.055,
    )
    box(
        "Shoulder seam",
        (sign * 0.445, 0.02, 0.794),
        (0.012, 0.96, 0.008),
        graphite,
        bevel=0.002,
    )
    box(
        "Ochre flanking plate",
        (sign * 0.793, -0.40, 0.74),
        (0.012, 0.26, 0.10),
        ochre,
        bevel=0.015,
    )
    for y in (0.13, 0.20, 0.27, 0.34):
        box(
            "Cooling outlet",
            (sign * 0.793, y, 0.73),
            (0.013, 0.034, 0.065),
            graphite,
            bevel=0.006,
        )
    label(
        "S-08", (sign * 0.786, -0.05, 0.73), 0.066, (math.pi / 2, 0, sign * math.pi / 2)
    )
    for y in (-0.49, 0.48):
        cylinder(
            "Shoulder captive bolt",
            (sign * 0.63, y, 0.858),
            0.023,
            0.013,
            steel,
            vertices=12,
        )
    cylinder("Impact piston", (sign * 0.40, -0.58, 0.49), 0.096, 0.25, steel, axis="Y")
    cylinder(
        "Piston dust boot", (sign * 0.40, -0.60, 0.49), 0.12, 0.12, rubber, axis="Y"
    )
    box(
        "Ram shoulder",
        (sign * 0.33, -0.71, 0.51),
        (0.32, 0.18, 0.34),
        graphite,
        bevel=0.04,
    )
    box(
        "Steel impact plate",
        (sign * 0.33, -0.815, 0.54),
        (0.32, 0.045, 0.30),
        armour,
        bevel=0.024,
    )
    box(
        "Replaceable bumper pad",
        (sign * 0.33, -0.846, 0.42),
        (0.31, 0.034, 0.073),
        rubber,
        bevel=0.012,
    )
    for x in (-0.075, 0.075):
        box(
            "Impact hazard marker",
            (sign * 0.33 + x, -0.843, 0.58),
            (0.031, 0.011, 0.125),
            ochre,
            bevel=0.004,
        )
    cylinder(
        "Front status lamp housing",
        (sign * 0.57, -0.685, 0.74),
        0.041,
        0.035,
        graphite,
        axis="Y",
    )
    cylinder(
        "Front status lamp", (sign * 0.57, -0.707, 0.74), 0.025, 0.009, amber, axis="Y"
    )

box("Central impact beam", (0, -0.735, 0.49), (0.61, 0.11, 0.21), graphite, bevel=0.025)
for x in (-0.20, -0.10, 0, 0.10, 0.20):
    box(
        "Impact grille bar", (x, -0.802, 0.51), (0.035, 0.031, 0.15), steel, bevel=0.008
    )
box(
    "Reactor breastplate recess",
    (0, -0.523, 0.86),
    (0.64, 0.05, 0.22),
    graphite,
    bevel=0.027,
)
box("Charge status glass", (0, -0.555, 0.87), (0.45, 0.015, 0.12), glass, bevel=0.018)
for x in (-0.17, -0.085, 0, 0.085, 0.17):
    box(
        "Charge indicator", (x, -0.565, 0.87), (0.034, 0.012, 0.075), amber, bevel=0.008
    )
label("STAND CLEAR", (0, -0.56, 0.77), 0.035, (math.pi / 2, 0, 0))

cylinder("Sensor turntable", (0, -0.12, 1.05), 0.24, 0.085, graphite)
cylinder("Turntable bearing", (0, -0.12, 1.096), 0.20, 0.032, steel)
box("Sensor helmet", (0, -0.17, 1.19), (0.64, 0.49, 0.25), armour, "Sensor", 0.035)
box(
    "Optical brow gasket",
    (0, -0.399, 1.20),
    (0.52, 0.055, 0.17),
    graphite,
    "Sensor",
    0.033,
)
box(
    "Smoked optical band", (0, -0.43, 1.19), (0.47, 0.023, 0.12), glass, "Sensor", 0.028
)
for x in (-0.115, 0.115):
    box(
        "Optic housing",
        (x, -0.449, 1.19),
        (0.134, 0.025, 0.073),
        steel,
        "Sensor",
        0.024,
    )
    box(
        "Amber range optic",
        (x, -0.465, 1.19),
        (0.10, 0.013, 0.025),
        optic,
        "Sensor",
        0.017,
    )
for sign in (-1, 1):
    brow = box(
        "Angled armoured brow",
        (sign * 0.145, -0.467, 1.235),
        (0.29, 0.093, 0.055),
        armour,
        "Sensor",
        0.009,
    )
    brow.rotation_euler.y = -sign * 0.14
    box(
        "Brow worn rim",
        (sign * 0.145, -0.515, 1.243),
        (0.27, 0.008, 0.013),
        edge,
        "Sensor",
        0.002,
    ).rotation_euler.y = -sign * 0.14
    box(
        "Impact plate worn edge",
        (sign * 0.33, -0.848, 0.691),
        (0.32, 0.01, 0.014),
        edge,
        bevel=0.003,
    )
    box(
        "Track guard rubbed edge",
        (sign * 0.787, 0.02, 0.833),
        (0.009, 1.10, 0.012),
        edge,
        bevel=0.003,
    )
    for y in (-0.49, 0.48):
        cylinder("Bolt seat", (sign * 0.63, y, 0.854), 0.037, 0.008, armour)
box(
    "Asymmetric service tab",
    (0.267, -0.19, 1.297),
    (0.07, 0.21, 0.015),
    ochre,
    "Sensor",
    0.005,
)
for sign in (-1, 1):
    cylinder(
        "Sensor trunnion",
        (sign * 0.327, -0.17, 1.19),
        0.065,
        0.033,
        graphite,
        "Sensor",
        "X",
    )
    cylinder(
        "Trunnion retainer",
        (sign * 0.348, -0.17, 1.19),
        0.035,
        0.014,
        steel,
        "Sensor",
        "X",
        12,
    )

for sign in (-1, 1):
    box(
        "Rear battery housing",
        (sign * 0.25, 0.41, 1.025),
        (0.23, 0.27, 0.14),
        graphite,
        bevel=0.027,
    )
    for y in (0.33, 0.38, 0.43, 0.48):
        box(
            "Battery cooling fin",
            (sign * 0.25, y, 1.102),
            (0.20, 0.018, 0.018),
            steel,
            bevel=0.004,
        )
    box(
        "Tail guard",
        (sign * 0.40, 0.68, 0.56),
        (0.20, 0.11, 0.24),
        graphite,
        bevel=0.025,
    )
    box(
        "Rear warning lamp",
        (sign * 0.43, 0.744, 0.61),
        (0.105, 0.014, 0.038),
        amber,
        bevel=0.012,
    )
label("08", (0, 0.471, 0.91), 0.115, (math.pi / 2, 0, math.pi))

bpy.context.view_layer.update()
clearance_parts = (
    "Armoured belly",
    "Structural shoulder frame",
    "Side armour underlay",
    "Steel track guard",
    "Ram shoulder",
    "Steel impact plate",
    "Replaceable bumper pad",
    "Tail guard",
    "Suspension sleeve",
)
for part in parts:
    if not part.name.startswith(clearance_parts):
        continue
    points = [part.matrix_world @ vertex.co for vertex in part.data.vertices]
    for sign in (-1, 1):
        for y in (-0.56, 0, 0.56):
            centre = (sign * 0.635, y, WHEEL_RADIUS)
            half = (0.1075, 0.277, 0.277)
            overlap = [
                min(max(p[i] for p in points), centre[i] + half[i])
                - max(min(p[i] for p in points), centre[i] - half[i])
                for i in range(3)
            ]
            assert min(overlap) <= 0, (
                f"{part.name} intersects a tyre envelope: {overlap}"
            )

bpy.ops.object.select_all(action="DESELECT")
for obj in parts:
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
bpy.ops.object.join()
mesh = bpy.context.object
mesh.name = "S-08 / armoured interceptor"
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
low = Vector(tuple(min(v.co[i] for v in mesh.data.vertices) for i in range(3)))
# Tyre blocks extend slightly beyond the circular tyre surface.
for v in mesh.data.vertices:
    v.co.z -= low.z
for index, (name, pos, parent) in enumerate(bones):
    if name != "Root":
        bones[index] = (name, (pos[0], pos[1], pos[2] - low.z), parent)
armature = bpy.data.armatures.new("Sentry mechanism")
rig = bpy.data.objects.new("SentryRig", armature)
bpy.context.collection.objects.link(rig)
bpy.context.view_layer.objects.active = rig
rig.select_set(True)
bpy.ops.object.mode_set(mode="EDIT")
for name, pos, parent in bones:
    bone = armature.edit_bones.new(name)
    bone.head = pos
    bone.tail = Vector(pos) + Vector((0, 0, 0.05))
    if parent:
        bone.parent = armature.edit_bones[parent]
bpy.ops.object.mode_set(mode="OBJECT")
mesh.parent = rig
mesh.modifiers.new("Rigid mechanisms", "ARMATURE").object = rig
rest = {b.name: b.matrix_local.to_quaternion() for b in armature.bones}
rig.animation_data_create()
scene = bpy.context.scene
scene.render.fps = FPS
for clip in ("Idle", "Drive"):
    action = bpy.data.actions.new(clip)
    action.use_fake_user = True
    rig.animation_data.action = action
    frames = FPS * (4 if clip == "Idle" else 1)
    for frame in range(frames + 1):
        t = frame / frames
        for bone in rig.pose.bones:
            bone.location = (0, 0, 0)
            rotation = Euler((0, 0, 0)).to_quaternion()
            if bone.name.startswith("Wheel") and clip == "Drive":
                rotation = Euler((math.tau * t, 0, 0)).to_quaternion()
            if bone.name == "Sensor":
                rotation = Euler(
                    (
                        0.025 * math.sin(math.tau * t),
                        0,
                        0.16 * math.sin(math.tau * t) if clip == "Idle" else 0,
                    )
                ).to_quaternion()
            if bone.name == "Hull" and clip == "Drive":
                bone.location = rest[bone.name].inverted() @ Vector(
                    (0, 0, 0.003 * (1 - math.cos(2 * math.tau * t)))
                )
            bone.rotation_quaternion = (
                rest[bone.name].inverted() @ rotation @ rest[bone.name]
            )
            for channel in ("location", "rotation_quaternion"):
                bone.keyframe_insert(data_path=channel, frame=frame)
rig.animation_data.action = bpy.data.actions["Idle"]
scene.frame_set(0)
bpy.ops.object.select_all(action="DESELECT")
mesh.select_set(True)
rig.select_set(True)
bpy.context.view_layer.objects.active = rig
bpy.ops.export_scene.gltf(
    filepath=str(MODEL),
    export_format="GLB",
    use_selection=True,
    export_animations=True,
    export_animation_mode="ACTIONS",
    export_anim_single_armature=True,
    export_force_sampling=True,
    export_frame_range=False,
    export_cameras=False,
    export_lights=False,
)
raw = MODEL.read_bytes()
length = struct.unpack_from("<I", raw, 12)[0]
document = json.loads(raw[20 : 20 + length])
document["animations"].sort(key=lambda a: ("Idle", "Drive").index(a["name"]))
for animation in document["animations"]:
    animation["channels"] = [
        channel
        for channel in animation["channels"]
        if (
            document["nodes"][channel["target"]["node"]]["name"] == "Sensor"
            if animation["name"] == "Idle"
            else document["nodes"][channel["target"]["node"]]["name"].startswith(
                ("Wheel", "Hull")
            )
        )
    ]
encoded = json.dumps(document, separators=(",", ":")).encode()
encoded += b" " * (-len(encoded) % 4)
binary = raw[20 + length :]
MODEL.write_bytes(
    struct.pack(
        "<4sIIII", b"glTF", 2, 20 + len(encoded) + len(binary), len(encoded), 0x4E4F534A
    )
    + encoded
    + binary
)
print("Exported sentry:", MODEL.stat().st_size, "bytes")
print(
    "Bounds:",
    tuple(
        round(
            max(v.co[i] for v in mesh.data.vertices)
            - min(v.co[i] for v in mesh.data.vertices),
            3,
        )
        for i in range(3)
    ),
)

if "--preview" in sys.argv:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    group = bpy.data.node_groups.get("glTF Material Output")
    if group is not None:
        bpy.data.node_groups.remove(group, do_unlink=True)
    bpy.ops.import_scene.gltf(filepath=str(MODEL))
    rig = next(o for o in scene.objects if o.type == "ARMATURE")
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = track.name != "Idle"
    scene.frame_set(0)
    bpy.ops.mesh.primitive_plane_add(size=200)
    floor = bpy.context.object
    mat = bpy.data.materials.new("Studio floor")
    mat.diffuse_color = (0.075, 0.09, 0.11, 1)
    floor.data.materials.append(mat)
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.use_denoising = True
    scene.world.color = (0.18, 0.18, 0.18)
    for pos, power, size in (
        ((-3, -4, 5), 550, 4),
        ((3, -1, 3), 350, 3),
        ((1, 3, 4), 700, 3),
    ):
        bpy.ops.object.light_add(type="AREA", location=pos)
        light = bpy.context.object
        light.data.energy, light.data.size = power, size
        light.rotation_euler = (
            (Vector((0, 0, 0.4)) - light.location).to_track_quat("-Z", "Y").to_euler()
        )
    bpy.ops.object.camera_add(location=(2.6, -3.4, 2.15))
    scene.camera = bpy.context.object
    scene.camera.rotation_euler = (
        (Vector((0, -0.02, 0.66)) - scene.camera.location)
        .to_track_quat("-Z", "Y")
        .to_euler()
    )
    scene.camera.data.type = "ORTHO"
    scene.camera.data.ortho_scale = 2.45
    scene.render.resolution_x = scene.render.resolution_y = 1100
    scene.render.resolution_percentage = 100
    scene.render.filepath = "/tmp/sentry-preview.png"
    bpy.ops.render.render(write_still=True)
    camera_pose = scene.camera.matrix_world.copy()
    scene.camera.location = (2.6, 3.4, 2.15)
    scene.camera.rotation_euler = (
        (Vector((0, 0, 0.66)) - scene.camera.location)
        .to_track_quat("-Z", "Y")
        .to_euler()
    )
    scene.render.filepath = "/tmp/sentry-rear.png"
    bpy.ops.render.render(write_still=True)
    scene.camera.matrix_world = camera_pose
    if "--motion" in sys.argv:
        output = Path("/tmp/sentry-motion")
        output.mkdir(exist_ok=True)
        scene.render.engine = "BLENDER_WORKBENCH"
        scene.display.shading.light = "STUDIO"
        scene.display.shading.color_type = "MATERIAL"
        scene.display.shading.show_cavity = True
        for mat in bpy.data.materials:
            if not mat.use_nodes:
                continue
            shader = mat.node_tree.nodes.get("Principled BSDF")
            if shader and shader.inputs["Base Color"].is_linked:
                image = getattr(
                    shader.inputs["Base Color"].links[0].from_node, "image", None
                )
                if image:
                    mat.diffuse_color = tuple(image.pixels[:4])
        scene.render.resolution_x = scene.render.resolution_y = 720
        for frame in range(96):
            for track in rig.animation_data.nla_tracks:
                track.mute = False
                for strip in track.strips:
                    strip.repeat = 10
                    if track.name == "Drive":
                        strip.use_animated_time = True
                        strip.strip_time = min(max(frame - 32, 0), 40) % FPS
            scene.frame_set(frame)
            scene.render.filepath = str(output / f"frame-{frame:04d}.png")
            bpy.ops.render.render(write_still=True)
