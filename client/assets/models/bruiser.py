"""Build the bruiser GLB in Blender; add -- --preview [--motion] for previews in /tmp."""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Euler, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from modelkit import ModelMaterials, wheeled_actor

MODEL = Path(__file__).resolve().with_suffix(".glb")
WHEEL_RADIUS = 0.27
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)


palette = ModelMaterials(MODEL.with_suffix(".json"))
armour = palette["armour"]
steel = palette["steel"]
rubber = palette["rubber"]
graphite = palette["graphite"]
ochre = palette["ochre"]
glass = palette["glass"]
amber = palette["amber"]
ink = palette["ink"]
optic = palette["optic"]

# Fine edges stay crisp with a single bevel segment.
parts = wheeled_actor.Parts(ink, lambda bevel: 3 if bevel >= 0.018 else 1)
bones = [
    ("Root", (0, 0, 0), None),
    ("Hull", (0, 0, 0.67), "Root"),
    ("Sensor", (0, -0.17, 1.13), "Hull"),
]
parts.box("Armoured belly", (0, 0, 0.32), (1.05, 1.14, 0.24), graphite, "Root", 0.055)
parts.box(
    "Structural shoulder frame",
    (0, 0.015, 0.65),
    (0.99, 1.11, 0.39),
    graphite,
    bevel=0.075,
)
parts.box("Gunmetal main glacis", (0, -0.03, 0.84), (0.99, 1.0, 0.38), armour, bevel=0.045)
parts.box("Upper service deck", (0, 0.13, 1.02), (0.79, 0.66, 0.09), armour, bevel=0.025)

for sign in (-1, 1):
    for index, y in enumerate((-0.56, 0, 0.56)):
        bone = ("WheelL" if sign < 0 else "WheelR") + str(index)
        bones.append((bone, (sign * 0.635, y, WHEEL_RADIUS), "Root"))
        parts.cylinder("Drive axle", (sign * 0.50, y, 0.27), 0.085, 0.30, steel, "Root", "X")
        parts.cylinder(
            "Heavy elastomer tyre",
            (sign * 0.635, y, 0.27),
            WHEEL_RADIUS,
            0.215,
            rubber,
            bone,
            "X",
            48,
        )
        parts.cylinder("Hub recess", (sign * 0.749, y, 0.27), 0.18, 0.017, graphite, bone, "X")
        parts.cylinder("Armoured hub", (sign * 0.761, y, 0.27), 0.135, 0.022, armour, bone, "X")
        parts.cylinder("Axle cap", (sign * 0.78, y, 0.27), 0.064, 0.024, steel, bone, "X", 12)
        for angle_index in range(8):
            a = math.tau * angle_index / 8
            parts.cylinder(
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
                block = parts.box(
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
                    Euler((-a, 0, 0)).to_quaternion() @ Euler((0, 0, lane * 0.20)).to_quaternion()
                )
                outward = block.rotation_quaternion @ Vector((0, 0, 1))
                assert outward.dot(Vector((0, math.sin(a), math.cos(a)))) > 0.9999
        for z in (0.43, 0.49):
            parts.box(
                "Suspension sleeve",
                (sign * 0.48, y, z),
                (0.05, 0.14, 0.028),
                steel,
                "Root",
                0.006,
            )
    parts.box(
        "Side armour underlay",
        (sign * 0.61, 0.015, 0.65),
        (0.32, 1.36, 0.16),
        armour,
        bevel=0.045,
    )
    for panel_y in (-0.455, 0.0, 0.455):
        parts.box(
            "Steel track guard",
            (sign * 0.61, panel_y, 0.76),
            (0.35, 0.439, 0.19),
            armour,
            bevel=0.026,
        )
    parts.box(
        "Shoulder seam",
        (sign * 0.445, 0.02, 0.794),
        (0.012, 0.96, 0.008),
        graphite,
        bevel=0.002,
    )
    parts.box(
        "Ochre flanking plate",
        (sign * 0.793, -0.40, 0.74),
        (0.012, 0.26, 0.10),
        ochre,
        bevel=0.015,
    )
    for y in (0.13, 0.20, 0.27, 0.34):
        parts.box(
            "Cooling outlet",
            (sign * 0.793, y, 0.73),
            (0.013, 0.034, 0.065),
            graphite,
            bevel=0.006,
        )
    parts.label("S-08", (sign * 0.786, -0.05, 0.73), 0.066, (math.pi / 2, 0, sign * math.pi / 2))
    parts.cylinder("Impact piston", (sign * 0.40, -0.58, 0.49), 0.096, 0.25, steel, axis="Y")
    parts.cylinder("Piston dust boot", (sign * 0.40, -0.60, 0.49), 0.12, 0.12, rubber, axis="Y")
    parts.box(
        "Ram shoulder",
        (sign * 0.33, -0.71, 0.51),
        (0.32, 0.18, 0.34),
        graphite,
        bevel=0.04,
    )
    parts.box(
        "Steel impact plate",
        (sign * 0.33, -0.815, 0.54),
        (0.32, 0.045, 0.30),
        armour,
        bevel=0.024,
    )
    parts.box(
        "Replaceable bumper pad",
        (sign * 0.33, -0.846, 0.42),
        (0.31, 0.034, 0.073),
        rubber,
        bevel=0.012,
    )
    for x in (-0.075, 0.075):
        parts.box(
            "Impact hazard marker",
            (sign * 0.33 + x, -0.843, 0.58),
            (0.031, 0.011, 0.125),
            ochre,
            bevel=0.004,
        )
    parts.cylinder(
        "Front status lamp housing",
        (sign * 0.57, -0.685, 0.74),
        0.041,
        0.035,
        graphite,
        axis="Y",
    )
    parts.cylinder("Front status lamp", (sign * 0.57, -0.707, 0.74), 0.025, 0.009, amber, axis="Y")

parts.box("Central impact beam", (0, -0.735, 0.49), (0.61, 0.11, 0.21), graphite, bevel=0.025)
for x in (-0.20, -0.10, 0, 0.10, 0.20):
    parts.box("Impact grille bar", (x, -0.802, 0.51), (0.035, 0.031, 0.15), steel, bevel=0.008)
parts.box(
    "Reactor breastplate recess",
    (0, -0.523, 0.86),
    (0.64, 0.05, 0.22),
    graphite,
    bevel=0.027,
)
parts.box("Charge status glass", (0, -0.555, 0.87), (0.45, 0.015, 0.12), glass, bevel=0.018)
for x in (-0.17, -0.085, 0, 0.085, 0.17):
    parts.box("Charge indicator", (x, -0.565, 0.87), (0.034, 0.012, 0.075), amber, bevel=0.008)
parts.label("STAND CLEAR", (0, -0.56, 0.77), 0.035, (math.pi / 2, 0, 0))

parts.cylinder("Sensor turntable", (0, -0.12, 1.05), 0.24, 0.085, graphite)
parts.cylinder("Turntable bearing", (0, -0.12, 1.096), 0.20, 0.032, steel)
parts.box("Sensor helmet", (0, -0.17, 1.19), (0.64, 0.49, 0.25), armour, "Sensor", 0.035)
parts.box(
    "Optical brow gasket",
    (0, -0.399, 1.20),
    (0.52, 0.055, 0.17),
    graphite,
    "Sensor",
    0.033,
)
parts.box("Smoked optical band", (0, -0.43, 1.19), (0.47, 0.023, 0.12), glass, "Sensor", 0.028)
for x in (-0.115, 0.115):
    parts.box(
        "Optic housing",
        (x, -0.449, 1.19),
        (0.134, 0.025, 0.073),
        steel,
        "Sensor",
        0.024,
    )
    parts.box(
        "Amber range optic",
        (x, -0.465, 1.19),
        (0.10, 0.013, 0.025),
        optic,
        "Sensor",
        0.017,
    )
for sign in (-1, 1):
    brow = parts.box(
        "Angled armoured brow",
        (sign * 0.145, -0.467, 1.235),
        (0.29, 0.093, 0.055),
        armour,
        "Sensor",
        0.009,
    )
    brow.rotation_euler.y = -sign * 0.14
parts.box(
    "Asymmetric service tab",
    (0.267, -0.19, 1.297),
    (0.07, 0.21, 0.015),
    ochre,
    "Sensor",
    0.005,
)
for sign in (-1, 1):
    parts.cylinder(
        "Sensor trunnion",
        (sign * 0.327, -0.17, 1.19),
        0.065,
        0.033,
        graphite,
        "Sensor",
        "X",
    )
    parts.cylinder(
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
    parts.box(
        "Rear battery housing",
        (sign * 0.25, 0.41, 1.025),
        (0.23, 0.27, 0.14),
        graphite,
        bevel=0.027,
    )
    for y in (0.33, 0.38, 0.43, 0.48):
        parts.box(
            "Battery cooling fin",
            (sign * 0.25, y, 1.102),
            (0.20, 0.018, 0.018),
            steel,
            bevel=0.004,
        )
    parts.box(
        "Tail guard",
        (sign * 0.40, 0.68, 0.56),
        (0.20, 0.11, 0.24),
        graphite,
        bevel=0.025,
    )
    parts.box(
        "Rear warning lamp",
        (sign * 0.43, 0.744, 0.61),
        (0.105, 0.014, 0.038),
        amber,
        bevel=0.012,
    )
parts.label("08", (0, 0.471, 0.91), 0.115, (math.pi / 2, 0, math.pi))

for sign in (-1, 1):
    for panel_y in (-0.455, 0.0, 0.455):
        parts.cylinder(
            "Recessed guard socket",
            (sign * 0.65, panel_y, 0.856),
            0.025,
            0.003,
            graphite,
        )
        parts.cylinder(
            "Socket hex head",
            (sign * 0.65, panel_y, 0.857),
            0.013,
            0.004,
            steel,
            vertices=6,
        )
parts.box("Service hatch gasket", (0, 0.17, 1.068), (0.29, 0.35, 0.009), graphite, bevel=0.018)
parts.box("Service hatch", (0, 0.17, 1.075), (0.264, 0.324, 0.012), armour, bevel=0.012)
for x in (-0.10, 0.10):
    for y in (0.045, 0.295):
        parts.cylinder("Hatch socket", (x, y, 1.082), 0.016, 0.002, graphite)
        parts.cylinder("Hatch bolt", (x, y, 1.083), 0.009, 0.003, steel, vertices=6)

for name, centre, size in (
    ("Gunmetal main glacis", (0.29, -0.542, 0.965), (0.043, 0.024, 0.025)),
    ("Sensor helmet", (-0.285, -0.405, 1.295), (0.028, 0.025, 0.018)),
):
    panel = next(obj for obj in parts.objects if obj.name == name)
    bpy.ops.mesh.primitive_uv_sphere_add(segments=16, ring_count=8, location=centre)
    cutter = bpy.context.object
    cutter.scale = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    bpy.context.view_layer.objects.active = panel
    modifier = panel.modifiers.new("Shallow impact dent", "BOOLEAN")
    modifier.operation = "DIFFERENCE"
    modifier.object = cutter
    bpy.ops.object.modifier_apply(modifier=modifier.name)
    bpy.data.objects.remove(cutter, do_unlink=True)
    group = panel.vertex_groups.get("Sensor" if name == "Sensor helmet" else "Hull")
    group.add(list(range(len(panel.data.vertices))), 1, "REPLACE")

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
for part in parts.objects:
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
            assert min(overlap) <= 0, f"{part.name} intersects a tyre envelope: {overlap}"

mesh = wheeled_actor.assemble(parts.objects, armour, palette.wear, MODEL, "S-08 / armoured interceptor")
rig = wheeled_actor.build_rig(bones, mesh, "Bruiser")
wheeled_actor.animate(rig, sensor_sway=(0.025, 0.16), hull_bob=0.003)
wheeled_actor.export(MODEL, rig, mesh)

if "--preview" in sys.argv:
    wheeled_actor.preview_actor(
        MODEL,
        camera=(2.6, -3.4, 2.15),
        look_at=(0, -0.02, 0.66),
        ortho_scale=2.45,
        extra_views=(
            ("rear", (2.6, 3.4, 2.15), (0, 0, 0.66), 2.45, 1100),
            ("gameplay", (2.6, -3.4, 2.15), (0, -0.02, 0.66), 5.0, 512),
        ),
        motion="--motion" in sys.argv,
    )
