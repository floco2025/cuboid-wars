"""Build the scuttler GLB in Blender; add -- --preview [--motion] for previews in /tmp."""

import math
import sys
from pathlib import Path

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from modelkit import ModelMaterials, wheeled_actor

MODEL = Path(__file__).resolve().with_suffix(".glb")
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
parts = wheeled_actor.Parts(ink, lambda bevel: 3)
bones = [
    ("Root", (0, 0, 0), None),
    ("Hull", (0, 0, 0.42), "Root"),
    ("Sensor", (0, -0.24, 0.73), "Hull"),
]
parts.box("Underbody skid", (0, 0, 0.26), (0.64, 0.76, 0.16), graphite, "Root", 0.05)
parts.box("Floating armour belt", (0, 0, 0.40), (0.72, 0.78, 0.20), graphite, bevel=0.07)
parts.box("Rounded charge housing", (0, 0.01, 0.52), (0.70, 0.72, 0.29), ivory, bevel=0.10)
for sign in (-1, 1):
    parts.box(
        "Shoulder armour",
        (sign * 0.265, 0.025, 0.65),
        (0.18, 0.59, 0.15),
        ivory,
        bevel=0.065,
    )
    parts.box(
        "Armour seam",
        (sign * 0.21, 0.03, 0.687),
        (0.012, 0.42, 0.006),
        graphite,
        bevel=0.002,
    )
    for y in (-0.29, 0.29):
        bone = ("WheelL" if sign < 0 else "WheelR") + ("Front" if y < 0 else "Rear")
        bones.append((bone, (sign * 0.435, y, WHEEL_RADIUS), "Root"))
        parts.cylinder("Axle", (sign * 0.33, y, 0.21), 0.056, 0.18, steel, "Root", "X")
        parts.cylinder(
            "All-terrain tyre",
            (sign * 0.435, y, 0.21),
            WHEEL_RADIUS,
            0.15,
            rubber,
            bone,
            "X",
            48,
        )
        parts.cylinder(
            "Recessed wheel rim",
            (sign * 0.514, y, 0.21),
            0.137,
            0.015,
            graphite,
            bone,
            "X",
        )
        parts.cylinder("Ivory hub", (sign * 0.525, y, 0.21), 0.10, 0.025, ivory, bone, "X")
        parts.cylinder(
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
            obj = parts.box(
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
            obj = parts.box(
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
    parts.label("M-03", (sign * 0.359, 0.04, 0.54), 0.067, (math.pi / 2, 0, sign * math.pi / 2))
    for y in (0.12, 0.18, 0.24):
        parts.box(
            "Cooling gill",
            (sign * 0.353, y, 0.47),
            (0.012, 0.035, 0.10),
            graphite,
            bevel=0.006,
        )

parts.cylinder("Charge collar", (0, 0.12, 0.66), 0.182, 0.11, steel)
parts.cylinder("Smoked charge chamber", (0, 0.12, 0.73), 0.155, 0.15, glass)
for height in (0.707, 0.773):
    parts.cylinder("Charge warning band", (0, 0.12, height), 0.158, 0.014, amber)
parts.cylinder("Sealed charge cap", (0, 0.12, 0.825), 0.17, 0.045, ivory)
parts.cylinder("Recessed arming dial", (0, 0.12, 0.851), 0.082, 0.014, ochre)
parts.label("!", (0, 0.086, 0.86), 0.095, (0, 0, 0))
for angle in range(0, 360, 60):
    a = math.radians(angle)
    parts.box(
        "Charge cage rib",
        (0.152 * math.sin(a), 0.12 + 0.152 * math.cos(a), 0.744),
        (0.024, 0.024, 0.145),
        graphite,
        bevel=0.008,
    )
    parts.cylinder(
        "Charge cap screw",
        (0.139 * math.sin(a), 0.12 + 0.139 * math.cos(a), 0.85),
        0.014,
        0.008,
        steel,
        vertices=12,
    )
parts.box("Front shock bumper", (0, -0.421, 0.315), (0.65, 0.09, 0.12), rubber, "Root", 0.035)
parts.box(
    "Bumper identification plate",
    (0, -0.469, 0.326),
    (0.34, 0.01, 0.066),
    ochre,
    "Root",
    0.008,
)
for x in (-0.105, 0, 0.105):
    obj = parts.box("Caution slash", (x, -0.476, 0.326), (0.025, 0.005, 0.057), ink, "Root", 0.001)
    obj.rotation_euler.y = -0.45
parts.cylinder("Sensor swivel", (0, -0.24, 0.70), 0.105, 0.07, graphite)
parts.box("Sensor pod", (0, -0.27, 0.785), (0.34, 0.25, 0.17), ivory, "Sensor", 0.055)
parts.box(
    "Smoked sensor fascia",
    (0, -0.396, 0.79),
    (0.277, 0.025, 0.108),
    glass,
    "Sensor",
    0.03,
)
parts.cylinder("Optic bezel", (-0.039, -0.421, 0.79), 0.053, 0.016, steel, "Sensor", "Y")
parts.cylinder("Amber tracking optic", (-0.039, -0.432, 0.79), 0.039, 0.012, amber, "Sensor", "Y")
parts.cylinder("Optic pupil", (-0.039, -0.441, 0.79), 0.024, 0.006, glass, "Sensor", "Y")
parts.cylinder("Rangefinder", (0.085, -0.414, 0.79), 0.018, 0.008, blue, "Sensor", "Y")
parts.box(
    "Offset brow",
    (-0.025, -0.392, 0.859),
    (0.23, 0.047, 0.014),
    graphite,
    "Sensor",
    0.006,
)
parts.box("Rear battery hatch", (0, 0.381, 0.52), (0.40, 0.027, 0.18), graphite, bevel=0.024)
for x in (-0.13, 0.13):
    parts.cylinder("Battery latch", (x, 0.402, 0.52), 0.022, 0.018, steel, axis="Y", vertices=12)
parts.label("CAUTION", (0, 0.406, 0.55), 0.033, (math.pi / 2, 0, math.pi))

mesh = wheeled_actor.assemble(parts.objects, ivory, palette.wear, MODEL, "M-03 / demolition rover")
rig = wheeled_actor.build_rig(bones, mesh, "Scuttler")
wheeled_actor.animate(rig, sensor_sway=(0.045, 0.24), hull_bob=0.005)
wheeled_actor.export(MODEL, rig, mesh)

if "--preview" in sys.argv:
    wheeled_actor.preview_actor(
        MODEL,
        camera=(2, -3, 1.8),
        look_at=(0, 0, 0.43),
        ortho_scale=1.65,
        motion="--motion" in sys.argv,
    )
