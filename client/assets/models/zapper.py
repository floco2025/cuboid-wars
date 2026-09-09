"""Build the zapper GLB in Blender; add -- --preview [--motion] for previews in /tmp."""

import json
import math
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model_materials import ModelMaterials, plain_material, project_uv

MODEL = Path(__file__).resolve().with_suffix(".glb")
FPS = 30
DURATION = 4
PIVOT_HEIGHT = 1.35
MUZZLE_DISTANCE = 0.285
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)


palette = ModelMaterials(MODEL.with_suffix(".materials.json"))
shell = palette["shell"]
metal = palette["metal"]
graphite = palette["graphite"]
glass = palette["glass"]
accent = palette["accent"]
cyan = palette["cyan"]
red = palette["red"]


def empty(name, location=(0, 0, 0), parent=None):
    obj = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(obj)
    obj.location = location
    obj.parent = parent
    return obj


def finish(obj, name, mat, parent, bevel=0, cylindrical=False):
    obj.name = name
    obj.data.materials.append(mat)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        mod = obj.modifiers.new("Casing edge radii", "BEVEL")
        mod.width, mod.segments = bevel, 3
        bpy.ops.object.modifier_apply(modifier=mod.name)
        mod = obj.modifiers.new("Corner normals", "WEIGHTED_NORMAL")
        bpy.ops.object.modifier_apply(modifier=mod.name)
    project_uv(obj, mat, cylindrical)
    obj.parent = parent
    return obj


def box(name, pos, size, mat, parent, bevel=0.008):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    obj = bpy.context.object
    obj.dimensions = size
    return finish(obj, name, mat, parent, bevel)


def sphere(name, pos, size, mat, parent):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=32, ring_count=16, location=pos)
    obj = bpy.context.object
    obj.scale = size
    for face in obj.data.polygons:
        face.use_smooth = True
    return finish(obj, name, mat, parent)


def cylinder(name, pos, radius, depth, mat, parent, axis="Z"):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=32, radius=radius, depth=depth, location=pos
    )
    obj = bpy.context.object
    if axis == "X":
        obj.rotation_euler.y = math.pi / 2
    elif axis == "Y":
        obj.rotation_euler.x = math.pi / 2
    for face in obj.data.polygons:
        face.use_smooth = len(face.vertices) == 4
    return finish(obj, name, mat, parent, 0.003, True)


def ring(name, pos, outer, inner, height, mat, parent, axis="Z"):
    segments = 48
    vertices = []
    for radius, z in (
        (outer, -height / 2),
        (outer, height / 2),
        (inner, -height / 2),
        (inner, height / 2),
    ):
        vertices.extend(
            (
                radius * math.cos(i * math.tau / segments),
                radius * math.sin(i * math.tau / segments),
                z,
            )
            for i in range(segments)
        )
    faces = []
    for i in range(segments):
        j = (i + 1) % segments
        faces.extend(
            (
                (i, j, segments + j, segments + i),
                (
                    2 * segments + j,
                    2 * segments + i,
                    3 * segments + i,
                    3 * segments + j,
                ),
                (segments + i, segments + j, 3 * segments + j, 3 * segments + i),
                (j, i, 2 * segments + i, 2 * segments + j),
            )
        )
    data = bpy.data.meshes.new(name)
    data.from_pydata(vertices, [], faces)
    data.update()
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    obj.location = pos
    if axis == "Y":
        obj.rotation_euler.x = math.pi / 2
    for i, face in enumerate(data.polygons):
        face.use_smooth = i % 4 < 2
    return finish(obj, name, mat, parent, 0.002, True)


def label(text, pos, size, parent, rotation=(math.pi / 2, 0, math.pi)):
    bpy.ops.object.text_add(location=pos)
    obj = bpy.context.object
    obj.data.body = text
    obj.data.size = size
    obj.data.align_x = "CENTER"
    obj.data.extrude = 0.00015
    obj.rotation_euler = rotation
    bpy.ops.object.convert(target="MESH")
    return finish(bpy.context.object, text, graphite, parent)


root = empty("ZapperRoot")
hull = empty("HoverHull", (0, 0.105, 1.54), root)
box("Central magnesium chassis", (0, 0, 0), (0.31, 0.32, 0.15), metal, hull, 0.04)
box("Upper white shell", (0, -0.008, 0.079), (0.335, 0.345, 0.095), shell, hull, 0.043)
box("Lower graphite shell", (0, 0, -0.06), (0.29, 0.30, 0.08), graphite, hull, 0.035)
box(
    "Crown identification panel",
    (0, -0.038, 0.128),
    (0.095, 0.19, 0.012),
    accent,
    hull,
    0.005,
)
label("Z-02", (0, -0.087, 0.135), 0.033, hull, (0, 0, 0))
box(
    "Rear battery cartridge",
    (0, -0.178, 0.01),
    (0.22, 0.058, 0.11),
    graphite,
    hull,
    0.018,
)
for x in (-0.065, 0, 0.065):
    box("Rear cooling fin", (x, -0.212, 0.025), (0.018, 0.02, 0.07), metal, hull, 0.003)
box("Rear flight beacon", (0, -0.224, 0.029), (0.05, 0.007, 0.018), cyan, hull, 0.004)
box("Sensor mask", (0, 0.166, 0.028), (0.235, 0.035, 0.075), glass, hull, 0.022)
for sign in (-1, 1):
    cylinder(
        "Sensor socket", (sign * 0.068, 0.189, 0.031), 0.026, 0.016, metal, hull, "Y"
    )
    cylinder("Sensor glass", (sign * 0.068, 0.20, 0.031), 0.019, 0.006, cyan, hull, "Y")
    cylinder(
        "Sensor pupil", (sign * 0.068, 0.204, 0.031), 0.012, 0.003, glass, hull, "Y"
    )
    brow = box(
        "Canted sensor brow",
        (sign * 0.071, 0.19, 0.071),
        (0.104, 0.035, 0.017),
        shell,
        hull,
        0.005,
    )
    brow.rotation_euler.y = sign * 0.12

pods, rotors, vanes = [], [], []
for sign, side in ((-1, "L"), (1, "R")):
    cylinder(
        "Lift trunnion", (sign * 0.173, -0.014, -0.015), 0.041, 0.06, metal, hull, "X"
    )
    pod = empty("LiftPod." + side, (sign * 0.322, -0.014, -0.015), hull)
    pods.append(pod)
    ring("White fan duct", (0, 0, 0), 0.137, 0.113, 0.075, shell, pod)
    ring("Metal duct lip", (0, 0, 0.04), 0.138, 0.111, 0.014, metal, pod)
    ring("Dark inner duct", (0, 0, -0.004), 0.114, 0.106, 0.051, graphite, pod)
    for angle in (0, math.pi / 2):
        brace = box(
            "Motor support", (0, 0, -0.026), (0.218, 0.015, 0.012), metal, pod, 0.003
        )
        brace.rotation_euler.z = angle
    cylinder("Lift motor", (0, 0, 0), 0.032, 0.07, graphite, pod)
    rotor = empty("Impeller." + side, (0, 0, 0.012), pod)
    rotors.append(rotor)
    cylinder("Rotor spinner", (0, 0, 0.018), 0.024, 0.025, accent, rotor)
    for blade_index in range(5):
        a = blade_index * math.tau / 5
        blade = box(
            "Swept impeller blade",
            (0.065 * math.cos(a), 0.065 * math.sin(a), 0),
            (0.078, 0.025, 0.007),
            graphite,
            rotor,
            0.003,
        )
        blade.rotation_euler = (0.16, 0, a + 0.20)
    for offset in (-0.045, 0.045):
        # Clip tops clear the duct lip instead of sharing its top plane.
        box(
            "Fan warning stripe",
            (sign * 0.126, offset, 0.028),
            (0.009, 0.026, 0.050),
            accent,
            pod,
            0.002,
        )
    vane = empty("Stabilizer." + side, (sign * 0.13, -0.175, -0.02), hull)
    vanes.append(vane)
    fin = box(
        "Tail control vane", (0, -0.03, 0.02), (0.025, 0.12, 0.092), shell, vane, 0.009
    )
    fin.rotation_euler.y = sign * 0.25
    box("Tail vane tip", (0, -0.055, 0.063), (0.034, 0.064, 0.014), accent, vane, 0.003)

# Aim nodes stay unanimated so the shared beam system owns their complete rotation.
cylinder("Gimbal suspension", (0, 0.105, -0.095), 0.044, 0.13, metal, hull)
yaw = empty("ZapperYaw", (0, 0.105, PIVOT_HEIGHT - 1.54), hull)
pitch = empty("ZapperPitch", parent=yaw)
cylinder("Azimuth bearing", (0, 0, 0.05), 0.071, 0.05, metal, yaw)
sphere("Gun cradle", (0, 0, 0), (0.098, 0.09, 0.082), graphite, yaw)
for sign in (-1, 1):
    cylinder("Elevation bearing", (sign * 0.102, 0, 0), 0.04, 0.018, metal, yaw, "X")
    cylinder("Elevation cap", (sign * 0.113, 0, 0), 0.026, 0.007, accent, yaw, "X")
box("Beam power block", (0, 0.03, 0), (0.147, 0.22, 0.12), metal, pitch, 0.025)
for sign in (-1, 1):
    box(
        "Emitter white cheek",
        (sign * 0.079, 0.04, 0),
        (0.018, 0.15, 0.086),
        shell,
        pitch,
        0.008,
    )
cylinder("Emitter core", (0, 0.148, 0), 0.049, 0.145, graphite, pitch, "Y")
for y in (0.11, 0.15, 0.19):
    ring("Emitter cooling ring", (0, y, 0), 0.064, 0.048, 0.013, metal, pitch, "Y")
for sign in (-1, 1):
    box(
        "Capacitor rail",
        (sign * 0.058, 0.15, 0),
        (0.018, 0.15, 0.027),
        metal,
        pitch,
        0.005,
    )
    box(
        "Charge indicator",
        (sign * 0.069, 0.15, 0),
        (0.004, 0.105, 0.009),
        red,
        pitch,
        0.002,
    )
cylinder("Muzzle housing", (0, 0.242, 0), 0.073, 0.06, shell, pitch, "Y")
ring("Muzzle lip", (0, 0.273, 0), 0.065, 0.048, 0.016, metal, pitch, "Y")
cylinder("Muzzle recess", (0, 0.275, 0), 0.047, 0.005, glass, pitch, "Y")
cylinder("Beam lens", (0, 0.28, 0), 0.035, 0.007, red, pitch, "Y")
empty("ZapperMuzzle", (0, MUZZLE_DISTANCE, 0), pitch)

bpy.context.view_layer.update()
groups = {}
for obj in list(bpy.context.scene.objects):
    if obj.type == "MESH":
        groups.setdefault(obj.parent, []).append(obj)
for parent, objects in groups.items():
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    objects[0].name = parent.name + " geometry"

scene = bpy.context.scene
scene.render.fps = FPS
scene.frame_start, scene.frame_end = 0, FPS * DURATION
animated = [hull, *pods, *rotors, *vanes]
for frame in range(scene.frame_end + 1):
    phase = math.tau * frame / scene.frame_end
    hull.location.z = 1.54 + 0.012 * math.sin(phase)
    hull.rotation_euler = (
        0.018 * math.sin(phase),
        0.025 * math.sin(phase * 2),
        math.pi,
    )
    hull.keyframe_insert(data_path="location", frame=frame)
    hull.keyframe_insert(data_path="rotation_euler", frame=frame)
    for index, (pod, rotor, vane) in enumerate(zip(pods, rotors, vanes)):
        sign = -1 if index == 0 else 1
        pod.rotation_euler = (
            0.07 * math.sin(phase + sign * 0.4),
            sign * 0.035 * math.cos(phase),
            0,
        )
        rotor.rotation_euler.z = sign * phase * 8
        vane.rotation_euler.z = sign * 0.09 * math.sin(phase * 2)
        for obj in (pod, rotor, vane):
            obj.keyframe_insert(data_path="rotation_euler", frame=frame)
scene.frame_set(0)
bpy.ops.object.select_all(action="SELECT")
bpy.ops.export_scene.gltf(
    filepath=str(MODEL),
    export_format="GLB",
    use_selection=True,
    export_animations=True,
    export_animation_mode="ACTIVE_ACTIONS",
    export_nla_strips_merged_animation_name="Hover",
    export_force_sampling=True,
    export_frame_range=True,
    export_cameras=False,
    export_lights=False,
)
raw = MODEL.read_bytes()
length = struct.unpack_from("<I", raw, 12)[0]
doc = json.loads(raw[20 : 20 + length])
assert len(doc["animations"]) == 1, "Hover loop missing or split across clips"
doc["animations"][0]["name"] = "Hover"
animated_names = {obj.name for obj in animated}
doc["animations"][0]["channels"] = [
    c
    for c in doc["animations"][0]["channels"]
    if doc["nodes"][c["target"]["node"]]["name"] in animated_names
]
assert all(
    any(
        doc["nodes"][c["target"]["node"]]["name"] == name
        for c in doc["animations"][0]["channels"]
    )
    for name in animated_names
)
assert all("uri" not in image for image in doc.get("images", [])), "Textures must be embedded"
encoded = json.dumps(doc, separators=(",", ":")).encode()
encoded += b" " * (-len(encoded) % 4)
binary = raw[20 + length :]
MODEL.write_bytes(
    struct.pack(
        "<4sIIII", b"glTF", 2, 20 + len(encoded) + len(binary), len(encoded), 0x4E4F534A
    )
    + encoded
    + binary
)
print("Exported zapper:", MODEL.stat().st_size, "bytes; Hover + independent aim rig")

if "--preview" in sys.argv:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    bpy.ops.import_scene.gltf(filepath=str(MODEL))
    yaw = bpy.data.objects["ZapperYaw"]
    pitch = bpy.data.objects["ZapperPitch"]
    yaw.rotation_mode = pitch.rotation_mode = "XYZ"
    scene.frame_set(12)
    floor_mat = plain_material("Studio floor", (0.065, 0.082, 0.10), roughness=0.7)
    bpy.ops.mesh.primitive_plane_add(size=200)
    bpy.context.object.data.materials.append(floor_mat)
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.use_denoising = True
    scene.world.color = (0.15, 0.15, 0.15)
    for location, power, size in (
        ((2, 3, 4), 420, 3),
        ((-2, 1, 3), 300, 3),
        ((0, -3, 4), 650, 3),
    ):
        bpy.ops.object.light_add(type="AREA", location=location)
        light = bpy.context.object
        light.data.energy, light.data.size = power, size
        light.rotation_euler = (
            (Vector((0, 0, 1.5)) - light.location).to_track_quat("-Z", "Y").to_euler()
        )
    bpy.ops.object.camera_add(location=(1.3, -2.4, 2.5))
    scene.camera = bpy.context.object
    scene.camera.rotation_euler = (
        (Vector((0, 0.035, 1.48)) - scene.camera.location)
        .to_track_quat("-Z", "Y")
        .to_euler()
    )
    scene.camera.data.type = "ORTHO"
    scene.camera.data.ortho_scale = 1.16
    scene.render.resolution_x = scene.render.resolution_y = 1100
    scene.render.resolution_percentage = 100
    scene.render.filepath = "/tmp/zapper-preview.png"
    bpy.ops.render.render(write_still=True)
    camera_pose = scene.camera.matrix_world.copy()
    scene.camera.location = (1.3, 2.4, 2.3)
    scene.camera.rotation_euler = (
        (Vector((0, 0.035, 1.48)) - scene.camera.location)
        .to_track_quat("-Z", "Y")
        .to_euler()
    )
    scene.render.filepath = "/tmp/zapper-rear.png"
    bpy.ops.render.render(write_still=True)
    scene.camera.matrix_world = camera_pose
    yaw.rotation_euler.z = -0.5
    pitch.rotation_euler.x = 0.30
    scene.render.filepath = "/tmp/zapper-aim.png"
    bpy.ops.render.render(write_still=True)
    if "--motion" in sys.argv:
        output = Path("/tmp/zapper-motion")
        output.mkdir(exist_ok=True)
        scene.render.engine = "BLENDER_WORKBENCH"
        scene.display.shading.light = "STUDIO"
        scene.display.shading.color_type = "MATERIAL"
        scene.display.shading.show_cavity = True
        shell.diffuse_color = (0.8, 0.81, 0.8, 1)
        metal.diffuse_color = (0.4, 0.45, 0.5, 1)
        scene.render.resolution_x = scene.render.resolution_y = 720
        for frame in range(FPS * DURATION):
            scene.frame_set(frame)
            yaw.rotation_euler.z = 0.55 * math.sin(frame / FPS * 1.2)
            pitch.rotation_euler.x = 0.25 * math.sin(frame / FPS * 1.6)
            scene.render.filepath = str(output / f"frame-{frame:04d}.png")
            bpy.ops.render.render(write_still=True)
