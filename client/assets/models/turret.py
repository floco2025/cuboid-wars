"""Build the turret GLB in Blender; add -- --preview for a still in /tmp."""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from modelkit import (
    ModelMaterials,
    bake_articulated_wear,
    child_of,
    empty,
    plain_material,
    preview,
    primitives,
)

MODEL = Path(__file__).resolve().with_suffix(".glb")

preview.clear_scene()


palette = ModelMaterials(MODEL.with_suffix(".json"))
armour = palette["armour"]
dark = palette["dark"]
steel = palette["steel"]
rubber = palette["rubber"]
warning = palette["warning"]
red = palette["red"]
cyan = palette["cyan"]


def box(name, pos, size, mat, parent=None, bevel=0.012):
    return primitives.box(name, pos, size, mat, child_of(parent), bevel, 2)


def cylinder(name, pos, radius, depth, mat, parent=None, axis="Z", vertices=16):
    return primitives.cylinder(
        name, pos, radius, depth, mat, child_of(parent), axis, vertices, 0.006, 2, None
    )


def beam(name, start, end, width, mat, parent=None):
    midpoint = (Vector(start) + Vector(end)) / 2
    direction = Vector(end) - Vector(start)
    obj = box(name, midpoint, (width, width, direction.length), mat, parent, 0.008)
    obj.rotation_euler = direction.to_track_quat("Z", "Y").to_euler()
    return obj


# Gimbals aim along local glTF -Z; the base turn makes the resting muzzle face
# gameplay +Z.
base = empty("TurretBase")
base.rotation_euler.z = math.pi
cylinder("Foundation", (0, 0, 0.07), 0.34, 0.14, dark, base, vertices=8)
cylinder("Base armour", (0, 0, 0.155), 0.29, 0.065, armour, base, vertices=8)
cylinder("Pedestal collar", (0, 0, 0.25), 0.15, 0.14, steel, base)
cylinder("Load column", (0, 0, 0.73), 0.095, 0.88, dark, base, vertices=8)
for sign in (-1, 1):
    box("Column armour", (sign * 0.088, 0, 0.73), (0.048, 0.145, 0.74), armour, base)
    beam(
        "Base brace", (sign * 0.25, 0, 0.16), (sign * 0.10, 0, 0.46), 0.075, steel, base
    )
    for y in (-0.2, 0.2):
        cylinder(
            "Anchor bolt",
            (sign * 0.20, y, 0.164),
            0.023,
            0.025,
            rubber,
            base,
            vertices=6,
        )
for z in (0.47, 0.94):
    box("Column band", (0, -0.09, z), (0.16, 0.018, 0.045), steel, base)
box("Power conduit", (0, -0.105, 0.71), (0.036, 0.035, 0.72), rubber, base)
box("Pedestal indicator", (0, 0.102, 0.90), (0.025, 0.014, 0.18), cyan, base, 0.004)
cylinder("Bearing housing", (0, 0, 1.17), 0.18, 0.13, dark, base)
cylinder("Bearing rim", (0, 0, 1.225), 0.19, 0.025, steel, base)

yaw = empty("TurretYaw", (0, 0, 1.45), base)
cylinder("Rotating turntable", (0, 0, -0.19), 0.16, 0.065, dark, yaw)
for sign in (-1, 1):
    box(
        "Gimbal fork", (sign * 0.225, 0, -0.08), (0.065, 0.19, 0.28), armour, yaw, 0.022
    )
    cylinder("Pitch bearing", (sign * 0.266, 0, 0), 0.078, 0.035, dark, yaw, "X")
    cylinder("Bearing cap", (sign * 0.288, 0, 0), 0.049, 0.018, steel, yaw, "X", 12)
    cylinder("Bearing light", (sign * 0.300, 0, 0), 0.018, 0.009, cyan, yaw, "X", 12)

pitch = empty("TurretPitch", parent=yaw)
box("Receiver", (0, -0.08, 0), (0.39, 0.45, 0.27), dark, pitch, 0.047)
box("Crown armour", (0, -0.08, 0.13), (0.42, 0.45, 0.075), armour, pitch, 0.026)
box("Underside armour", (0, -0.11, -0.13), (0.32, 0.37, 0.048), steel, pitch)
for sign in (-1, 1):
    box(
        "Cheek armour",
        (sign * 0.19, -0.10, 0.008),
        (0.055, 0.31, 0.22),
        armour,
        pitch,
        0.018,
    )
    box(
        "Receiver seam",
        (sign * 0.22, -0.10, 0.04),
        (0.009, 0.24, 0.025),
        rubber,
        pitch,
        0.002,
    )
    for y in (-0.22, -0.16, -0.10, -0.04):
        box(
            "Cooling louvre",
            (sign * 0.222, y, -0.052),
            (0.009, 0.022, 0.050),
            dark,
            pitch,
            0.002,
        )
    box(
        "Crown caution stripe",
        (sign * 0.10, -0.09, 0.172),
        (0.045, 0.28, 0.005),
        warning,
        pitch,
        0.001,
    )
    for y in (-0.23, 0.07):
        cylinder(
            "Crown fastener",
            (sign * 0.165, y, 0.171),
            0.011,
            0.008,
            rubber,
            pitch,
            vertices=6,
        )
box("Rear heat sink", (0, -0.322, 0), (0.29, 0.035, 0.18), rubber, pitch)
for x in (-0.105, -0.07, -0.035, 0, 0.035, 0.07, 0.105):
    box("Heat sink fin", (x, -0.34, 0), (0.012, 0.025, 0.14), steel, pitch, 0.003)
cylinder("Barrel root", (0, 0.18, 0), 0.105, 0.13, steel, pitch, "Y")
cylinder("Emitter barrel", (0, 0.35, 0), 0.066, 0.29, dark, pitch, "Y", 12)
for y in (0.245, 0.31, 0.375, 0.44):
    cylinder("Cooling ring", (0, y, 0), 0.084, 0.024, steel, pitch, "Y", 12)
for sign in (-1, 1):
    box("Emitter rail", (sign * 0.083, 0.34, 0), (0.026, 0.30, 0.068), armour, pitch)
    box(
        "Rail charge strip",
        (sign * 0.098, 0.35, 0),
        (0.007, 0.18, 0.013),
        red,
        pitch,
        0.002,
    )
cylinder("Muzzle shroud", (0, 0.50, 0), 0.094, 0.095, dark, pitch, "Y", 12)
cylinder("Muzzle rim", (0, 0.55, 0), 0.093, 0.015, steel, pitch, "Y", 12)
cylinder("Muzzle recess", (0, 0.56, 0), 0.071, 0.008, rubber, pitch, "Y", 24)
cylinder("Emitter lens", (0, 0.566, 0), 0.047, 0.009, red, pitch, "Y", 24)
empty("TurretMuzzle", (0, 0.572, 0), pitch)
box("Sensor visor", (0.135, 0.165, 0.08), (0.075, 0.05, 0.048), rubber, pitch)
box("Sensor slit", (0.135, 0.193, 0.08), (0.045, 0.008, 0.014), red, pitch, 0.003)

groups = {}
for obj in bpy.context.scene.objects:
    if obj.type == "MESH":
        groups.setdefault((obj.parent, obj.active_material), []).append(obj)

bake_articulated_wear(
    [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH" and obj.active_material == armour
    ],
    armour,
    palette.wear,
    MODEL,
)
bpy.ops.object.select_all(action="DESELECT")
for (parent, mat), objects in groups.items():
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    objects[0].name = f"{parent.name} / {mat.name}"
    mod = objects[0].modifiers.new("Export triangles", "TRIANGULATE")
    mod.keep_custom_normals = True
    bpy.ops.object.modifier_apply(modifier=mod.name)
    objects[0].select_set(False)

bpy.ops.object.select_all(action="SELECT")
bpy.ops.export_scene.gltf(
    filepath=str(MODEL),
    export_format="GLB",
    export_tangents=True,
    use_selection=True,
    export_apply=True,
    export_animations=False,
    export_cameras=False,
    export_lights=False,
)

print("Exported turret:", MODEL.stat().st_size, "bytes")

if "--preview" in sys.argv:
    preview.clear_scene()
    bpy.ops.import_scene.gltf(filepath=str(MODEL))
    scene = bpy.context.scene
    look_at = (0, -0.05, 0.86)
    low, high = preview.visible_bounds(scene)
    preview.floor(plain_material("Studio floor", (0.028, 0.038, 0.058), 0.2, 0.6))
    camera = preview.studio(scene, look_at, max(high - low))
    preview.aim(camera, (2.3, -3.4, 2.35), look_at)
    camera.data.ortho_scale = 2.2
    preview.render(scene, "/tmp/turret-preview.png")
