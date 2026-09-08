"""Build the player GLB: blender --background --python client/assets/models/player.py.

Add -- --preview to render the exported model and its animation poses in /tmp.
"""

import json
import math
import struct
import sys
from pathlib import Path

import bpy
import numpy as np
from mathutils import Euler, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model_materials import catalog_material, project_uv
from player_mocap import RobotMocap

MODEL = Path(__file__).resolve().with_suffix(".glb")
MATERIAL_SETTINGS = json.loads(MODEL.with_suffix(".materials.json").read_text())
CLIPS = ("Idle", "Walk", "Run", "Climb", "Jump", "Fall", "Land", "Stunned", "StrafeLeft", "StrafeRight")
FPS = 30
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)


def material(name, color, metallic=0.0, roughness=0.4, emission=0.0):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*color, 1)
    mat.use_nodes = True
    shader = mat.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = (*color, 1)
    shader.inputs["Metallic"].default_value = metallic
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Emission Color"].default_value = (*color, 1)
    shader.inputs["Emission Strength"].default_value = emission
    return mat


ivory = catalog_material("scuffed-plastic", "Satin ceramic-white polymer", tuning=MATERIAL_SETTINGS["scuffed-plastic"])
joint = catalog_material("synth-rubber", "Fine matte elastomer", tuning=MATERIAL_SETTINGS["synth-rubber"])
steel = catalog_material("brushed-metal", "Brushed titanium mechanisms", tuning=MATERIAL_SETTINGS["brushed-metal"])
chassis = material("Graphite structural composite", (0.028, 0.036, 0.042), 0.55, 0.32)
accent = material("Muted ochre identification", (0.38, 0.19, 0.065), 0.15, 0.5)
screen = material("Smoked optical visor", (0.007, 0.013, 0.017), 0.45, 0.19)
eye = material("Ice-blue sensor", (0.10, 0.52, 0.68), 0.1, 0.22, 2.0)
amber = material("Status diode", (0.7, 0.25, 0.035), 0.15, 0.4, 0.8)
lettering = material("Graphite service stencil", (0.05, 0.07, 0.075), 0.0, 0.6)

parts = []


def finish(obj, name, mat, bone, bevel=0, cylindrical=False):
    obj.name = name
    obj.data.materials.append(mat)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        modifier = obj.modifiers.new("Rounded casing edges", "BEVEL")
        modifier.width = bevel
        modifier.segments = 3
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        modifier = obj.modifiers.new("Weighted corner normals", "WEIGHTED_NORMAL")
        bpy.ops.object.modifier_apply(modifier=modifier.name)
    project_uv(obj, mat, cylindrical)
    group = obj.vertex_groups.new(name=bone)
    group.add(list(range(len(obj.data.vertices))), 1.0, "REPLACE")
    parts.append(obj)
    return obj


def box(name, pos, size, mat, bone, bevel=0.018):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    obj = bpy.context.object
    obj.dimensions = size
    return finish(obj, name, mat, bone, bevel)


def sphere(name, pos, size, mat, bone):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=16, ring_count=8, radius=1, location=pos)
    obj = bpy.context.object
    obj.scale = size
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    return finish(obj, name, mat, bone)


def cylinder(name, pos, radius, depth, mat, bone, axis="Z", vertices=16):
    bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=depth, location=pos)
    obj = bpy.context.object
    for polygon in obj.data.polygons:
        polygon.use_smooth = len(polygon.vertices) == 4
    if axis == "X":
        obj.rotation_euler.y = math.pi / 2
    elif axis == "Y":
        obj.rotation_euler.x = math.pi / 2
    return finish(obj, name, mat, bone, 0.004, cylindrical=True)


def rod(name, start, end, radius, mat, bone):
    vector = Vector(end) - Vector(start)
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=12, radius=radius, depth=vector.length, location=(Vector(start) + Vector(end)) / 2
    )
    obj = bpy.context.object
    for polygon in obj.data.polygons:
        polygon.use_smooth = len(polygon.vertices) == 4
    obj.rotation_euler = vector.to_track_quat("Z", "Y").to_euler()
    return finish(obj, name, mat, bone, 0.003, cylindrical=True)


def shell(name, x, rings, mat, bone, arc=(0, math.tau), exponent=1.0):
    segments = 32
    closed = arc == (0, math.tau)
    count = segments if closed else segments + 1
    vertices = []
    for z, width, depth, cy in rings:
        for j in range(count):
            angle = arc[0] + (arc[1] - arc[0]) * j / segments
            c, sn = math.cos(angle), math.sin(angle)
            vertices.append(
                (
                    x + width * math.copysign(abs(c) ** exponent, c),
                    cy + depth * math.copysign(abs(sn) ** exponent, sn),
                    z,
                )
            )
    faces = []
    for i in range(len(rings) - 1):
        for j in range(segments):
            k = (j + 1) % count
            faces.append((i * count + j, i * count + k, (i + 1) * count + k, (i + 1) * count + j))
    if closed:
        faces.extend([tuple(reversed(range(count))), tuple((len(rings) - 1) * count + j for j in range(count))])
    data = bpy.data.meshes.new(name)
    data.from_pydata(vertices, [], faces)
    data.update()
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    modifier = obj.modifiers.new("Sculpted shell curvature", "SUBSURF")
    modifier.levels = 2
    bpy.ops.object.modifier_apply(modifier=modifier.name)
    if not closed:
        modifier = obj.modifiers.new("Panel lip", "SOLIDIFY")
        modifier.thickness = 0.003
        bpy.ops.object.modifier_apply(modifier=modifier.name)
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    return finish(obj, name, mat, bone)


def cable(name, points, radius, mat, bone):
    curve = bpy.data.curves.new(name, "CURVE")
    curve.dimensions = "3D"
    curve.bevel_depth = radius
    curve.bevel_resolution = 2
    spline = curve.splines.new("BEZIER")
    spline.bezier_points.add(len(points) - 1)
    for point, coordinate in zip(spline.bezier_points, points):
        point.co = coordinate
        point.handle_left_type = point.handle_right_type = "AUTO"
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.convert(target="MESH")
    return finish(bpy.context.object, name, mat, bone)


def stencil(text, pos, size, bone):
    bpy.ops.object.text_add(location=pos, rotation=(math.pi / 2, 0, 0))
    obj = bpy.context.object
    obj.data.body = text
    obj.data.size = size
    obj.data.extrude = 0.0003
    obj.data.align_x = "CENTER"
    bpy.ops.object.convert(target="MESH")
    return finish(bpy.context.object, text, lettering, bone)


# Blender -Y exports as game +Z, matching player FaceYaw.
shell(
    "Pelvic chassis",
    0,
    [
        (0.83, 0.10, 0.065, 0),
        (0.84, 0.145, 0.085, 0),
        (0.91, 0.158, 0.10, 0),
        (0.97, 0.142, 0.085, 0),
        (0.985, 0.105, 0.064, 0),
    ],
    chassis,
    "Root",
)
shell(
    "Pelvic shell",
    0,
    [
        (0.83, 0.065, 0.084, -0.01),
        (0.85, 0.10, 0.098, -0.01),
        (0.91, 0.15, 0.109, -0.01),
        (0.965, 0.135, 0.10, -0.01),
        (0.974, 0.13, 0.093, -0.01),
    ],
    ivory,
    "Root",
    (math.pi * 1.04, math.pi * 1.96),
)
for sign in (-1, 1):
    shell(
        "Iliac shell",
        sign * 0.125,
        [(0.88, 0.019, 0.07, 0.02), (0.91, 0.035, 0.085, 0.02), (0.958, 0.035, 0.074, 0.02), (0.97, 0.024, 0.06, 0.02)],
        ivory,
        "Root",
    )
cylinder("Waist drive", (0, 0, 1.018), 0.067, 0.14, steel, "Torso")
for z, radius in ((0.982, 0.079), (1.004, 0.076), (1.026, 0.072), (1.048, 0.075), (1.07, 0.079)):
    cylinder("Abdominal bellows", (0, 0, z), radius, 0.020, joint, "Torso", vertices=32)
for sign in (-1, 1):
    rod("Abdominal stabilizer", (sign * 0.093, 0.008, 0.97), (sign * 0.127, 0.018, 1.13), 0.012, steel, "Torso")
    cable(
        "Protected waist loom",
        [(sign * 0.055, 0.066, 0.97), (sign * 0.067, 0.081, 1.025), (sign * 0.080, 0.072, 1.09)],
        0.009,
        joint,
        "Torso",
    )
shell(
    "Thoracic frame",
    0,
    [
        (1.09, 0.091, 0.084, -0.014),
        (1.12, 0.107, 0.100, -0.008),
        (1.26, 0.157, 0.113, 0),
        (1.37, 0.189, 0.110, 0),
        (1.414, 0.173, 0.091, 0),
        (1.444, 0.101, 0.059, 0),
        (1.45, 0.080, 0.057, 0),
    ],
    chassis,
    "Torso",
    exponent=0.86,
)
shell(
    "Pectoral shell",
    0,
    [
        (1.10, 0.101, 0.097, -0.014),
        (1.12, 0.117, 0.112, -0.008),
        (1.26, 0.17, 0.125, 0),
        (1.37, 0.203, 0.122, 0),
        (1.414, 0.187, 0.102, 0),
        (1.444, 0.113, 0.069, 0),
        (1.45, 0.091, 0.067, 0),
    ],
    ivory,
    "Torso",
    (math.pi * 1.035, math.pi * 1.965),
    exponent=0.86,
)
cable(
    "Sternum panel seam",
    [(0, -0.088, 1.439), (0, -0.124, 1.37), (0, -0.128, 1.27), (0, -0.121, 1.20), (0, -0.110, 1.13)],
    0.0025,
    chassis,
    "Torso",
)
shell(
    "Dorsal shell",
    0,
    [
        (1.09, 0.10, 0.084, 0.019),
        (1.15, 0.13, 0.097, 0.019),
        (1.33, 0.18, 0.116, 0.017),
        (1.414, 0.162, 0.102, 0.014),
        (1.44, 0.086, 0.058, 0.014),
    ],
    ivory,
    "Torso",
    (0.06, math.pi - 0.06),
    exponent=0.84,
)
for sign in (-1, 1):
    for z in (1.16, 1.19, 1.22):
        cable(
            "Flank ventilation",
            [(sign * 0.144, -0.058, z), (sign * 0.161, 0, z + 0.014), (sign * 0.153, 0.065, z + 0.016)],
            0.008,
            joint,
            "Torso",
        )
    rod("Clavicle linkage", (sign * 0.155, 0, 1.413), (sign * 0.25, 0, 1.414), 0.025, steel, "Torso")
    cable(
        "Shoulder service conduit",
        [(sign * 0.145, 0.06, 1.427), (sign * 0.218, 0.073, 1.43), (sign * 0.255, 0.053, 1.38)],
        0.009,
        joint,
        "Torso",
    )
for x in (-0.073, -0.036, 0, 0.036, 0.073):
    box("Back cooling slot", (x, 0.132, 1.29), (0.010, 0.013, 0.095), chassis, "Torso", 0.004)
box("Service access panel", (0, 0.124, 1.17), (0.11, 0.022, 0.069), chassis, "Torso", 0.012)
for x in (-0.04, 0.04):
    cylinder("Access captive fastener", (x, 0.137, 1.17), 0.005, 0.012, steel, "Torso", "Y", 6)
stencil("07", (-0.088, -0.121, 1.323), 0.041, "Torso")
stencil("AUTONOMOUS", (0.080, -0.122, 1.343), 0.010, "Torso")
box("Identification stripe", (-0.10, -0.122, 1.39), (0.060, 0.007, 0.013), accent, "Torso", 0.003)
for x, mat in ((0.074, eye), (0.091, amber)):
    cylinder("Chest diagnostic diode", (x, -0.130, 1.312), 0.004, 0.007, mat, "Torso", "Y", 12)

sphere("Cervical socket", (0, 0, 1.454), (0.053, 0.052, 0.046), joint, "Torso")
cylinder("Cervical bearing", (0, 0, 1.49), 0.042, 0.105, steel, "Head", vertices=32)
cylinder("Neck flexible sleeve", (0, 0, 1.479), 0.047, 0.081, joint, "Head", vertices=32)
sphere("Occipital ball joint", (0, 0, 1.539), (0.047, 0.048, 0.044), chassis, "Head")
for z in (1.458, 1.482, 1.506):
    cylinder("Neck seal", (0, 0, z), 0.052, 0.013, joint, "Head", vertices=32)
shell(
    "Cranial shell",
    0,
    [
        (1.532, 0.046, 0.058, 0.008),
        (1.55, 0.077, 0.078, 0.006),
        (1.61, 0.098, 0.095, 0.011),
        (1.70, 0.108, 0.103, 0.016),
        (1.76, 0.096, 0.095, 0.024),
        (1.80, 0.061, 0.065, 0.027),
        (1.808, 0.019, 0.023, 0.027),
    ],
    ivory,
    "Head",
    exponent=0.88,
)
shell(
    "Continuous smoked visor",
    0,
    [
        (1.565, 0.046, 0.079, -0.001),
        (1.59, 0.079, 0.096, 0),
        (1.65, 0.097, 0.108, 0.008),
        (1.716, 0.103, 0.110, 0.012),
        (1.742, 0.089, 0.105, 0.016),
    ],
    screen,
    "Head",
    (math.pi * 1.055, math.pi * 1.945),
    exponent=0.85,
)
cable(
    "Visor upper seal",
    [
        (-0.086, -0.04, 1.738),
        (-0.067, -0.086, 1.741),
        (0, -0.092, 1.744),
        (0.067, -0.086, 1.741),
        (0.086, -0.04, 1.738),
    ],
    0.0025,
    joint,
    "Head",
)
for side, x in (("L", -0.034), ("R", 0.034)):
    bone = "Eye." + side
    cylinder("Recessed optical assembly", (x, -0.103, 1.681), 0.016, 0.005, chassis, bone, "Y", 32)
    cylinder("Optical aperture", (x, -0.107, 1.681), 0.009, 0.003, eye, bone, "Y", 32)
    cylinder("Optical pupil", (x, -0.109, 1.681), 0.0045, 0.002, screen, bone, "Y", 24)
for sign in (-1, 1):
    cylinder("Temporal actuator", (sign * 0.106, 0.024, 1.647), 0.033, 0.013, chassis, "Head", "X", 32)
    cylinder("Temporal hub", (sign * 0.114, 0.024, 1.647), 0.020, 0.009, steel, "Head", "X", 24)
    for z in (1.666, 1.684, 1.702):
        box("Auditory intake", (sign * 0.099, 0.065, z), (0.008, 0.029, 0.005), chassis, "Head", 0.002)
shell(
    "Cervical cover",
    0,
    [(1.531, 0.046, 0.040, 0.055), (1.558, 0.059, 0.05, 0.053), (1.61, 0.067, 0.055, 0.04)],
    chassis,
    "Head",
    (0.10, math.pi - 0.10),
)

for side, sign in (("L", -1), ("R", 1)):
    hip, knee, ankle = "Thigh." + side, "Shin." + side, "Foot." + side
    x = sign * 0.112
    sphere("Hip ball", (x, 0, 0.90), (0.061, 0.065, 0.064), joint, hip)
    cylinder("Hip hub", (x + sign * 0.04, 0, 0.90), 0.036, 0.07, steel, hip, "X", 24)
    rod("Femoral axle", (x, 0, 0.90), (x, 0, 0.52), 0.028, chassis, hip)
    shell(
        "Femoral mechanism",
        x,
        [(0.55, 0.033, 0.039, 0), (0.62, 0.049, 0.055, 0), (0.78, 0.057, 0.062, 0), (0.855, 0.042, 0.045, 0)],
        chassis,
        hip,
    )
    shell(
        "Thigh shell",
        x,
        [
            (0.574, 0.036, 0.054, -0.006),
            (0.61, 0.055, 0.071, -0.006),
            (0.75, 0.066, 0.076, -0.004),
            (0.84, 0.062, 0.070, -0.004),
            (0.854, 0.044, 0.047, -0.003),
        ],
        ivory,
        hip,
        exponent=0.88,
    )
    cable(
        "Thigh panel seam",
        [(x + sign * 0.057, 0.025, 0.64), (x + sign * 0.064, 0.03, 0.74), (x + sign * 0.055, 0.03, 0.82)],
        0.0025,
        chassis,
        hip,
    )
    rod("Posterior thigh tendon", (x, 0.062, 0.59), (x, 0.071, 0.82), 0.011, steel, hip)
    cylinder("Knee hinge", (x, 0, 0.52), 0.048, 0.131, joint, knee, "X", 32)
    cylinder("Knee bearing cap", (x + sign * 0.067, 0, 0.52), 0.034, 0.012, steel, knee, "X", 24)
    cylinder("Knee fastener", (x + sign * 0.075, 0, 0.52), 0.013, 0.006, chassis, knee, "X", 12)
    shell(
        "Patellar shield",
        x,
        [
            (0.478, 0.027, 0.032, -0.026),
            (0.50, 0.046, 0.045, -0.021),
            (0.538, 0.049, 0.045, -0.019),
            (0.562, 0.032, 0.035, -0.009),
        ],
        ivory,
        knee,
    )
    rod("Tibial axle", (x, 0, 0.52), (x, 0, 0.115), 0.020, chassis, knee)
    shell(
        "Tibial frame",
        x,
        [
            (0.15, 0.021, 0.024, 0.014),
            (0.24, 0.029, 0.032, 0.014),
            (0.41, 0.041, 0.038, 0.014),
            (0.474, 0.037, 0.029, 0.007),
        ],
        chassis,
        knee,
    )
    shell(
        "Calf shell",
        x,
        [
            (0.163, 0.028, 0.030, 0.005),
            (0.20, 0.035, 0.042, 0.008),
            (0.33, 0.052, 0.068, 0.022),
            (0.411, 0.057, 0.069, 0.020),
            (0.46, 0.044, 0.046, 0.012),
        ],
        ivory,
        knee,
        exponent=0.9,
    )
    cable(
        "Shin longitudinal seam",
        [(x - sign * 0.021, -0.028, 0.195), (x - sign * 0.031, -0.039, 0.31), (x - sign * 0.03, -0.045, 0.41)],
        0.002,
        chassis,
        knee,
    )
    rod("Ankle linear actuator", (x + sign * 0.033, 0.017, 0.172), (x + sign * 0.048, 0.038, 0.378), 0.008, steel, knee)
    cylinder("Ankle gimbal", (x, 0, 0.115), 0.031, 0.105, steel, ankle, "X", 24)
    for z in (0.127, 0.146):
        cylinder("Ankle flex seal", (x, 0.008, z), 0.032, 0.012, joint, ankle, vertices=24)
    shell(
        "Articulated foot sole",
        x,
        [
            (0.006, 0.048, 0.10, -0.048),
            (0.013, 0.070, 0.14, -0.055),
            (0.037, 0.074, 0.141, -0.055),
            (0.046, 0.066, 0.128, -0.054),
        ],
        joint,
        ankle,
        exponent=0.60,
    )
    shell(
        "Metatarsal shell",
        x,
        [
            (0.042, 0.064, 0.125, -0.051),
            (0.055, 0.069, 0.129, -0.05),
            (0.087, 0.061, 0.11, -0.044),
            (0.12, 0.047, 0.072, -0.016),
            (0.14, 0.027, 0.042, 0.002),
        ],
        ivory,
        ankle,
        exponent=0.64,
    )
    for y, z in ((-0.131, 0.078), (-0.104, 0.092)):
        cable(
            "Flexible toe seam",
            [(x - 0.052, y, z - 0.01), (x, y - 0.005, z), (x + 0.052, y, z - 0.01)],
            0.0035,
            joint,
            ankle,
        )
    box("Heel bumper", (x, 0.084, 0.05), (0.085, 0.023, 0.037), chassis, ankle, 0.009)
    for y in (-0.15, -0.09, -0.03, 0.03):
        box("Outsole tread", (x, y, 0.012), (0.13, 0.015, 0.020), joint, ankle, 0.003)

    upper, forearm, hand = "UpperArm." + side, "Forearm." + side, "Hand." + side
    sx, ex, wx = sign * 0.255, sign * 0.291, sign * 0.303
    sphere("Shoulder articulation", (sx, 0, 1.409), (0.056, 0.060, 0.059), joint, upper)
    cylinder("Shoulder drive ring", (sx + sign * 0.032, 0, 1.409), 0.046, 0.032, steel, upper, "X", 32)
    shell(
        "Deltoid shell",
        sx,
        [
            (1.334, 0.039, 0.041, 0),
            (1.364, 0.068, 0.071, 0),
            (1.43, 0.074, 0.076, 0.004),
            (1.466, 0.051, 0.053, 0.006),
            (1.472, 0.027, 0.030, 0.006),
        ],
        ivory,
        upper,
    )
    rod("Humeral drive", (sx, 0, 1.409), (ex, 0, 1.10), 0.019, chassis, upper)
    shell(
        "Upper arm shell",
        ex - sign * 0.007,
        [(1.153, 0.028, 0.034, 0), (1.185, 0.040, 0.046, 0), (1.285, 0.047, 0.052, 0), (1.32, 0.036, 0.040, 0)],
        ivory,
        upper,
    )
    cable(
        "Triceps cable",
        [(sx + sign * 0.025, 0.053, 1.354), (ex + sign * 0.021, 0.05, 1.27), (ex + sign * 0.023, 0.034, 1.14)],
        0.006,
        joint,
        upper,
    )
    cylinder("Elbow hinge", (ex, 0, 1.10), 0.039, 0.111, joint, forearm, "X", 24)
    cylinder("Elbow cap", (ex + sign * 0.058, 0, 1.10), 0.027, 0.013, steel, forearm, "X", 24)
    rod("Forearm drive", (ex, 0, 1.10), (wx, 0, 0.823), 0.021, chassis, forearm)
    rod("Radial strut", (ex + sign * 0.021, 0, 1.061), (wx + sign * 0.021, 0, 0.858), 0.012, steel, forearm)
    shell(
        "Forearm shell",
        wx,
        [
            (0.855, 0.026, 0.032, -0.004),
            (0.88, 0.034, 0.04, -0.004),
            (0.97, 0.047, 0.05, 0),
            (1.041, 0.049, 0.049, 0),
            (1.06, 0.037, 0.035, 0),
        ],
        ivory,
        forearm,
        exponent=0.82,
    )
    shell(
        "Forearm rear service inset",
        wx,
        [(0.898, 0.018, 0.040, 0), (0.926, 0.025, 0.051, 0), (1.018, 0.031, 0.053, 0), (1.034, 0.022, 0.049, 0)],
        chassis,
        forearm,
        (0.4, math.pi - 0.4),
    )
    box(
        "Cuff service mark",
        (wx, -0.047, 1.017),
        (0.038, 0.005, 0.009),
        accent if side == "L" else chassis,
        forearm,
        0.002,
    )
    cylinder("Wrist bearing", (wx, 0, 0.82), 0.022, 0.067, steel, hand, vertices=24)
    shell(
        "Palm chassis",
        wx,
        [
            (0.729, 0.035, 0.020, -0.003),
            (0.743, 0.042, 0.025, -0.003),
            (0.793, 0.041, 0.026, 0),
            (0.804, 0.025, 0.023, 0),
        ],
        chassis,
        hand,
        exponent=0.60,
    )
    shell(
        "Dorsal hand shell",
        wx,
        [(0.738, 0.031, 0.024, 0), (0.747, 0.040, 0.028, 0), (0.783, 0.036, 0.029, 0), (0.796, 0.023, 0.026, 0)],
        ivory,
        hand,
        (0.08, math.pi - 0.08),
        exponent=0.66,
    )
    fingers = "Fingers." + side
    for index, offset in enumerate((-0.029, -0.010, 0.010, 0.029)):
        length = (0.053, 0.069, 0.066, 0.051)[index]
        fx = wx + offset
        tip = f"FingerTip.{index}." + side
        cylinder("Metacarpal bearing", (fx, -0.002, 0.735), 0.011, 0.016, steel, fingers, "X", 12)
        rod("Proximal phalanx", (fx, -0.004, 0.730), (fx, -0.006, 0.730 - length * 0.53), 0.0085, ivory, fingers)
        cylinder("Finger hinge", (fx, -0.006, 0.730 - length * 0.53), 0.0085, 0.017, chassis, tip, "X", 12)
        rod("Distal phalanx", (fx, -0.007, 0.727 - length * 0.53), (fx, -0.010, 0.73 - length), 0.0075, ivory, tip)
        sphere("Tactile fingertip", (fx, -0.010, 0.73 - length), (0.008, 0.009, 0.009), joint, tip)
    rod("Thumb metacarpal", (wx + sign * 0.032, -0.003, 0.785), (wx + sign * 0.056, -0.01, 0.758), 0.011, ivory, hand)
    cylinder("Thumb hinge", (wx + sign * 0.056, -0.01, 0.758), 0.011, 0.020, chassis, hand, "X", 12)
    rod("Thumb phalanx", (wx + sign * 0.056, -0.011, 0.758), (wx + sign * 0.056, -0.028, 0.732), 0.009, ivory, hand)
    sphere("Tactile thumb tip", (wx + sign * 0.056, -0.030, 0.730), (0.0095, 0.010, 0.010), joint, hand)

bpy.ops.object.select_all(action="DESELECT")
for obj in parts:
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
bpy.ops.object.join()
mesh = bpy.context.object
mesh.name = "Field unit / rigid mechanical skin"
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

armature = bpy.data.armatures.new("Field unit skeleton")
rig = bpy.data.objects.new("FieldUnit", armature)
bpy.context.collection.objects.link(rig)
bpy.context.view_layer.objects.active = rig
mesh.select_set(False)
rig.select_set(True)
bpy.ops.object.mode_set(mode="EDIT")
bone_specs = [
    ("Root", (0, 0, 0.91), None),
    ("Torso", (0, 0, 1.00), "Root"),
    ("Head", (0, 0, 1.48), "Torso"),
    ("Antenna", (0.05, 0.045, 1.789), "Head"),
    ("Eye.L", (-0.034, -0.103, 1.681), "Head"),
    ("Eye.R", (0.034, -0.103, 1.681), "Head"),
]
for side, sign in (("L", -1), ("R", 1)):
    bone_specs.extend(
        [
            ("Thigh." + side, (sign * 0.112, 0, 0.90), "Root"),
            ("Shin." + side, (sign * 0.112, 0, 0.52), "Thigh." + side),
            ("Foot." + side, (sign * 0.112, 0, 0.115), "Shin." + side),
            ("UpperArm." + side, (sign * 0.255, 0, 1.409), "Torso"),
            ("Forearm." + side, (sign * 0.291, 0, 1.10), "UpperArm." + side),
            ("Hand." + side, (sign * 0.303, 0, 0.823), "Forearm." + side),
            ("Fingers." + side, (sign * 0.303, -0.002, 0.735), "Hand." + side),
        ]
    )
    for index, offset in enumerate((-0.029, -0.010, 0.010, 0.029)):
        length = (0.053, 0.069, 0.066, 0.051)[index]
        bone_specs.append(
            (f"FingerTip.{index}." + side, (sign * 0.303 + offset, -0.006, 0.730 - length * 0.53), "Fingers." + side)
        )
for name, position, parent in bone_specs:
    bone = armature.edit_bones.new(name)
    bone.head = position
    bone.tail = Vector(position) + Vector((0, 0, 0.06))
    if parent:
        bone.parent = armature.edit_bones[parent]
bpy.ops.object.mode_set(mode="OBJECT")
mesh.parent = rig
modifier = mesh.modifiers.new("Rigid joint deformation", "ARMATURE")
modifier.object = rig
rest_rotation = {bone.name: bone.matrix_local.to_quaternion() for bone in armature.bones}


def rotate(name, x=0, y=0, z=0):
    rotation = rest_rotation[name]
    rig.pose.bones[name].rotation_quaternion = rotation.inverted() @ Euler((x, y, z)).to_quaternion() @ rotation


def translate(name, x=0, y=0, z=0):
    rig.pose.bones[name].location = rest_rotation[name].inverted() @ Vector((x, y, z))


def pose(clip, t):
    for bone in rig.pose.bones:
        bone.location = (0, 0, 0)
        bone.rotation_quaternion = (1, 0, 0, 0)
        bone.scale = (1, 1, 1)
    phase = math.tau * t
    sine = math.sin(phase)
    clearance = 0.0
    if clip == "Stunned":
        translate("Root", z=-0.045)
        rotate("Torso", x=0.12, y=0.055 * sine)
        rotate("Head", x=0.14, y=0.18 * sine, z=0.22 * sine)
        for side, sign in (("L", -1), ("R", 1)):
            rotate("Thigh." + side, x=-0.18)
            rotate("Shin." + side, x=0.36)
            rotate("Foot." + side, x=-0.18)
            rotate("UpperArm." + side, x=0.06 * sine, y=-sign * 0.12)
            rotate("Forearm." + side, x=-0.10)
    else:
        clearance = mocap.apply(clip, t)
    for side, sign in (("L", -1), ("R", 1)):
        gripping = clip == "Climb"
        rotate("Hand." + side, x=0.15 if gripping else 0, z=0 if gripping else -sign * math.pi / 2)
        rotate("Fingers." + side, x=-0.60 if gripping else -0.02)
        for index in range(4):
            rotate(f"FingerTip.{index}." + side, x=-0.85 if gripping else -0.10)
    rotate("Antenna", x=0.01 * sine)
    return clearance


mocap = RobotMocap(rig)
durations = {**mocap.durations, "Stunned": 1.6}

checked_bones = ("Root", "Foot.L", "Foot.R", "Forearm.L", "Forearm.R", "Hand.L", "Hand.R", "Fingers.L", "Fingers.R")
checked_bones += tuple(f"FingerTip.{index}.{side}" for side in ("L", "R") for index in range(4))
rest_vertices = {}
for name in checked_bones:
    group = mesh.vertex_groups[name].index
    rest_vertices[name] = np.array(
        [(*vertex.co, 1) for vertex in mesh.data.vertices if any(weight.group == group for weight in vertex.groups)]
    )
inverse_bind = {name: np.array(armature.bones[name].matrix_local.inverted()) for name in checked_bones}


def posed_vertices(name):
    matrix = np.array(rig.pose.bones[name].matrix) @ inverse_bind[name]
    return (rest_vertices[name] @ matrix.T)[:, :3]


def finish_pose(clip, frame, clearance):
    bpy.context.view_layer.update()
    if clip not in ("Jump", "Fall", "Climb"):
        lowest = min(posed_vertices(name)[:, 2].min() for name in ("Foot.L", "Foot.R"))
        root = rig.pose.bones["Root"]
        root.location += rest_rotation["Root"].inverted() @ Vector((0, 0, clearance - lowest))
        bpy.context.view_layer.update()
    pelvis = posed_vertices("Root")
    for name in checked_bones[3:]:
        limb = posed_vertices(name)
        overlap = np.minimum(pelvis.max(axis=0), limb.max(axis=0)) - np.maximum(pelvis.min(axis=0), limb.min(axis=0))
        assert not np.all(overlap > 0), f"{clip} frame {frame}: {name} intersects the pelvic armour by {overlap}"


bpy.context.scene.render.fps = FPS
rig.animation_data_create()
actions = {}
for clip in CLIPS:
    action = bpy.data.actions.new(clip)
    action.use_fake_user = True
    rig.animation_data.action = action
    frames = round(durations[clip] * FPS)
    for frame in range(frames + 1):
        clearance = pose(clip, frame / frames)
        finish_pose(clip, frame, clearance)
        if frame == 0:
            first_pose = {bone.name: np.array(bone.matrix) for bone in rig.pose.bones}
        elif frame == frames and clip not in ("Jump", "Fall", "Land"):
            for bone in rig.pose.bones:
                error = np.abs(np.array(bone.matrix) - first_pose[bone.name]).max()
                assert error < 0.0001, f"{clip}: {bone.name} has a discontinuous loop ({error})"
        for bone in rig.pose.bones:
            for channel in ("location", "rotation_quaternion", "scale"):
                bone.keyframe_insert(data_path=channel, frame=frame)
    actions[clip] = action

rig.animation_data.action = actions["Idle"]
bpy.context.scene.frame_set(0)
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

# Clip indices are an asset contract; Blender's action ordering is not.
raw = MODEL.read_bytes()
json_length = struct.unpack_from("<I", raw, 12)[0]
document = json.loads(raw[20 : 20 + json_length])
by_name = {animation["name"]: animation for animation in document["animations"]}
document["animations"] = [by_name[name] for name in CLIPS]
encoded = json.dumps(document, separators=(",", ":")).encode()
encoded += b" " * (-len(encoded) % 4)
binary_chunk = raw[20 + json_length :]
MODEL.write_bytes(
    struct.pack("<4sIIII", b"glTF", 2, 20 + len(encoded) + len(binary_chunk), len(encoded), 0x4E4F534A)
    + encoded
    + binary_chunk
)
print(f"Exported {MODEL.name}: {len(CLIPS)} clips, {len(armature.bones)} joints, {MODEL.stat().st_size:,} bytes")

if "--preview" in sys.argv or "--rear-preview" in sys.argv:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    bpy.ops.import_scene.gltf(filepath=str(MODEL))
    rig = next(obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE")
    rig.animation_data.action = None
    tracks = {track.name: track for track in rig.animation_data.nla_tracks}
    print("Imported preview tracks:", list(tracks))
    for track in tracks.values():
        track.mute = True
    floor = material("Studio floor", (0.075, 0.10, 0.115), 0.1, 0.65)
    box("Studio", (0, 0, -0.05), (200, 200, 0.1), floor, "Root", 0)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 40
    scene.cycles.use_denoising = True
    scene.world.color = (0.22, 0.22, 0.22)
    for name, position, power, color, size in [
        ("Key", (-3, -4, 5), 700, (1.0, 0.90, 0.78), 4),
        ("Rim", (2, 2, 3), 850, (0.55, 0.80, 1.0), 3),
        ("Fill", (3, -2, 2.5), 350, (0.65, 0.9, 1.0), 3),
    ]:
        light = bpy.data.lights.new(name, "AREA")
        light.energy, light.color, light.shape, light.size = power, color, "DISK", size
        obj = bpy.data.objects.new(name, light)
        scene.collection.objects.link(obj)
        obj.location = position
        obj.rotation_euler = (Vector((0, 0, 0.9)) - obj.location).to_track_quat("-Z", "Y").to_euler()
    bpy.ops.object.camera_add(location=(2.6, -4.4, 2.5))
    scene.camera = bpy.context.object
    scene.camera.rotation_euler = (Vector((0, 0, 0.91)) - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
    scene.camera.data.type = "ORTHO"
    scene.camera.data.ortho_scale = 2.35
    scene.render.resolution_x = scene.render.resolution_y = 800
    scene.render.resolution_percentage = 100
    for clip in (CLIPS if "--preview" in sys.argv else ()):
        track = tracks[clip]
        track.mute = False
        scene.frame_set(round(durations[clip] * FPS * (0.45 if clip == "Jump" else 0.15)))
        scene.render.filepath = f"/tmp/player-{clip.lower()}.png"
        bpy.ops.render.render(write_still=True)
        track.mute = True

    tracks["Idle"].mute = False
    scene.frame_set(14)
    scene.camera.location = (1.25, -2.6, 1.63)
    scene.camera.rotation_euler = (Vector((0, 0, 1.44)) - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
    scene.camera.data.ortho_scale = 0.95
    scene.render.resolution_x = scene.render.resolution_y = 1100
    scene.render.filepath = "/tmp/player-detail.png"
    bpy.ops.render.render(write_still=True)

    if "--rear-preview" in sys.argv:
        scene.camera.location = (0.75, 2.6, 1.85)
        scene.camera.rotation_euler = (Vector((0, 0, 1.65)) - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
        scene.camera.data.ortho_scale = 0.52
        scene.render.filepath = "/tmp/player-rear-head.png"
        bpy.ops.render.render(write_still=True)
