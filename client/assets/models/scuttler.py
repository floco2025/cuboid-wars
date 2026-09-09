"""Build the scuttler GLB in Blender; add -- --preview [--motion] for previews in /tmp."""

import json
import math
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Euler, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model_materials import ModelMaterials, project_uv
from model_wear import bake_armour, remember_panel_coordinates

MODEL = Path(__file__).resolve().with_suffix(".glb")
FPS = 30
WHEEL_RADIUS = 0.21
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
palette = ModelMaterials(MODEL.with_suffix(".json"))
ivory = palette["ivory"]
rubber = palette["rubber"]
steel = palette["steel"]
graphite = palette["graphite"]
ochre = palette["ochre"]
glass = palette["glass"]
blue = palette["blue"]
amber = palette["amber"]
ink = palette["ink"]
parts = []


def finish(obj, name, mat, bone, bevel=0, cylindrical=False):
    obj.name = name
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        mod = obj.modifiers.new("Rounded armour", "BEVEL")
        mod.width, mod.segments = bevel, 3
        bpy.ops.object.modifier_apply(modifier=mod.name)
        mod = obj.modifiers.new("Corner normals", "WEIGHTED_NORMAL")
        bpy.ops.object.modifier_apply(modifier=mod.name)
    project_uv(obj, mat, cylindrical)
    if mat == ivory:
        remember_panel_coordinates(obj)
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
    ("Hull", (0, 0, 0.42), "Root"),
    ("Sensor", (0, -0.24, 0.73), "Hull"),
]
box("Underbody skid", (0, 0, 0.26), (0.64, 0.76, 0.16), graphite, "Root", 0.05)
box("Floating armour belt", (0, 0, 0.40), (0.72, 0.78, 0.20), graphite, bevel=0.07)
box("Rounded charge housing", (0, 0.01, 0.52), (0.70, 0.72, 0.29), ivory, bevel=0.10)
for sign in (-1, 1):
    box(
        "Shoulder armour",
        (sign * 0.265, 0.025, 0.65),
        (0.18, 0.59, 0.15),
        ivory,
        bevel=0.065,
    )
    box(
        "Armour seam",
        (sign * 0.21, 0.03, 0.687),
        (0.012, 0.42, 0.006),
        graphite,
        bevel=0.002,
    )
    for y in (-0.29, 0.29):
        bone = ("WheelL" if sign < 0 else "WheelR") + ("Front" if y < 0 else "Rear")
        bones.append((bone, (sign * 0.435, y, WHEEL_RADIUS), "Root"))
        cylinder("Axle", (sign * 0.33, y, 0.21), 0.056, 0.18, steel, "Root", "X")
        cylinder(
            "All-terrain tyre",
            (sign * 0.435, y, 0.21),
            WHEEL_RADIUS,
            0.15,
            rubber,
            bone,
            "X",
            48,
        )
        cylinder(
            "Recessed wheel rim",
            (sign * 0.514, y, 0.21),
            0.137,
            0.015,
            graphite,
            bone,
            "X",
        )
        cylinder("Ivory hub", (sign * 0.525, y, 0.21), 0.10, 0.025, ivory, bone, "X")
        cylinder(
            "Captive axle bolt",
            (sign * 0.542, y, 0.21),
            0.035,
            0.012,
            steel,
            bone,
            "X",
            12,
        )
        for spoke in range(5):
            angle = math.tau * spoke / 5
            obj = box(
                "Recessed hub spoke",
                (
                    sign * 0.54,
                    y + math.sin(angle) * 0.075,
                    0.21 + math.cos(angle) * 0.075,
                ),
                (0.004, 0.013, 0.06),
                graphite,
                bone,
                0.003,
            )
            obj.rotation_euler.x = -angle
        for tread in range(28):
            angle = math.tau * tread / 28
            obj = box(
                "Traction block",
                (
                    sign * 0.435,
                    y + math.sin(angle) * 0.209,
                    0.21 + math.cos(angle) * 0.209,
                ),
                (0.142, 0.023, 0.012),
                rubber,
                bone,
                0.004,
            )
            obj.rotation_euler.x = -angle
    label(
        "M-03", (sign * 0.359, 0.04, 0.54), 0.067, (math.pi / 2, 0, sign * math.pi / 2)
    )
    for y in (0.12, 0.18, 0.24):
        box(
            "Cooling gill",
            (sign * 0.353, y, 0.47),
            (0.012, 0.035, 0.10),
            graphite,
            bevel=0.006,
        )

cylinder("Charge collar", (0, 0.12, 0.66), 0.182, 0.11, steel)
cylinder("Smoked charge chamber", (0, 0.12, 0.73), 0.155, 0.15, glass)
for height in (0.707, 0.773):
    cylinder("Charge warning band", (0, 0.12, height), 0.158, 0.014, amber)
cylinder("Sealed charge cap", (0, 0.12, 0.825), 0.17, 0.045, ivory)
cylinder("Recessed arming dial", (0, 0.12, 0.851), 0.082, 0.014, ochre)
label("!", (0, 0.086, 0.86), 0.095, (0, 0, 0))
for angle in range(0, 360, 60):
    a = math.radians(angle)
    box(
        "Charge cage rib",
        (0.152 * math.sin(a), 0.12 + 0.152 * math.cos(a), 0.744),
        (0.024, 0.024, 0.145),
        graphite,
        bevel=0.008,
    )
    cylinder(
        "Charge cap screw",
        (0.139 * math.sin(a), 0.12 + 0.139 * math.cos(a), 0.85),
        0.014,
        0.008,
        steel,
        vertices=12,
    )
box("Front shock bumper", (0, -0.421, 0.315), (0.65, 0.09, 0.12), rubber, "Root", 0.035)
box(
    "Bumper identification plate",
    (0, -0.469, 0.326),
    (0.34, 0.01, 0.066),
    ochre,
    "Root",
    0.008,
)
for x in (-0.105, 0, 0.105):
    obj = box(
        "Caution slash", (x, -0.476, 0.326), (0.025, 0.005, 0.057), ink, "Root", 0.001
    )
    obj.rotation_euler.y = -0.45
cylinder("Sensor swivel", (0, -0.24, 0.70), 0.105, 0.07, graphite)
box("Sensor pod", (0, -0.27, 0.785), (0.34, 0.25, 0.17), ivory, "Sensor", 0.055)
box(
    "Smoked sensor fascia",
    (0, -0.396, 0.79),
    (0.277, 0.025, 0.108),
    glass,
    "Sensor",
    0.03,
)
cylinder("Optic bezel", (-0.039, -0.421, 0.79), 0.053, 0.016, steel, "Sensor", "Y")
cylinder(
    "Amber tracking optic", (-0.039, -0.432, 0.79), 0.039, 0.012, amber, "Sensor", "Y"
)
cylinder("Optic pupil", (-0.039, -0.441, 0.79), 0.024, 0.006, glass, "Sensor", "Y")
cylinder("Rangefinder", (0.085, -0.414, 0.79), 0.018, 0.008, blue, "Sensor", "Y")
box(
    "Offset brow",
    (-0.025, -0.392, 0.859),
    (0.23, 0.047, 0.014),
    graphite,
    "Sensor",
    0.006,
)
box("Rear battery hatch", (0, 0.381, 0.52), (0.40, 0.027, 0.18), graphite, bevel=0.024)
for x in (-0.13, 0.13):
    cylinder(
        "Battery latch", (x, 0.402, 0.52), 0.022, 0.018, steel, axis="Y", vertices=12
    )
label("CAUTION", (0, 0.406, 0.55), 0.033, (math.pi / 2, 0, math.pi))

armour_parts = [obj for obj in parts if obj.active_material == ivory]
parts = [obj for obj in parts if obj.active_material != ivory]
parts.append(bake_armour(armour_parts, ivory, palette.wear, MODEL))

bpy.ops.object.select_all(action="DESELECT")
for obj in parts:
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
bpy.ops.object.join()
mesh = bpy.context.object
mesh.name = "M-03 / demolition rover"
triangulate = mesh.modifiers.new("Export triangles", "TRIANGULATE")
triangulate.keep_custom_normals = True
bpy.ops.object.modifier_apply(modifier=triangulate.name)
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
low = Vector(tuple(min(v.co[i] for v in mesh.data.vertices) for i in range(3)))
# Tyre blocks extend slightly beyond the circular tyre surface.
for v in mesh.data.vertices:
    v.co.z -= low.z
for index, (name, pos, parent) in enumerate(bones):
    if name != "Root":
        bones[index] = (name, (pos[0], pos[1], pos[2] - low.z), parent)
armature = bpy.data.armatures.new("Scuttler mechanism")
rig = bpy.data.objects.new("ScuttlerRig", armature)
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
                        0.045 * math.sin(math.tau * t),
                        0,
                        0.24 * math.sin(math.tau * t) if clip == "Idle" else 0,
                    )
                ).to_quaternion()
            if bone.name == "Hull" and clip == "Drive":
                bone.location = rest[bone.name].inverted() @ Vector(
                    (0, 0, 0.005 * (1 - math.cos(2 * math.tau * t)))
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
    export_tangents=True,
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
print("Exported scuttler:", MODEL.stat().st_size, "bytes")
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
    bpy.ops.object.camera_add(location=(2, -3, 1.8))
    scene.camera = bpy.context.object
    scene.camera.rotation_euler = (
        (Vector((0, 0, 0.43)) - scene.camera.location)
        .to_track_quat("-Z", "Y")
        .to_euler()
    )
    scene.camera.data.type = "ORTHO"
    scene.camera.data.ortho_scale = 1.65
    scene.render.resolution_x = scene.render.resolution_y = 1100
    scene.render.resolution_percentage = 100
    scene.render.filepath = "/tmp/scuttler-preview.png"
    bpy.ops.render.render(write_still=True)
    if "--motion" in sys.argv:
        output = Path("/tmp/scuttler-motion")
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
