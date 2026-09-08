"""Build the turret GLB with Blender: blender --background --python client/assets/models/turret.py."""

import math
from pathlib import Path

import bpy
from mathutils import Vector

MODEL = Path(__file__).resolve().with_suffix(".glb")
ASSETS = MODEL.parent.parent
STEEL = ASSETS / "textures/used-stainless-steel-ue"

bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)


def material(name, color, metallic, roughness, textured=False, emission=None):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*color, 1)
    mat.use_backface_culling = True
    mat.use_nodes = True
    shader = mat.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = (*color, 1)
    shader.inputs["Metallic"].default_value = metallic
    shader.inputs["Roughness"].default_value = roughness
    if textured:
        texture = mat.node_tree.nodes.new("ShaderNodeTexImage")
        texture.image = bpy.data.images.load(str(STEEL / "used-stainless-steel_albedo.png"), check_existing=True)
        tint = mat.node_tree.nodes.new("ShaderNodeMix")
        tint.data_type = "RGBA"
        tint.blend_type = "MULTIPLY"
        tint.inputs[0].default_value = 1
        tint.inputs[7].default_value = (*color, 1)
        mat.node_tree.links.new(texture.outputs["Color"], tint.inputs[6])
        mat.node_tree.links.new(tint.outputs[2], shader.inputs["Base Color"])
        wear = mat.node_tree.nodes.new("ShaderNodeTexImage")
        wear.image = bpy.data.images.load(str(STEEL / "used-stainless-steel_roughness.png"), check_existing=True)
        wear.image.colorspace_settings.name = "Non-Color"
        factor = mat.node_tree.nodes.new("ShaderNodeMath")
        factor.operation = "MULTIPLY"
        factor.inputs[1].default_value = roughness
        mat.node_tree.links.new(wear.outputs["Color"], factor.inputs[0])
        mat.node_tree.links.new(factor.outputs[0], shader.inputs["Roughness"])
    if emission:
        shader.inputs["Emission Color"].default_value = (*emission, 1)
        shader.inputs["Emission Strength"].default_value = 4
    return mat


armor = material("Ceramic silver / brushed wear", (0.74, 0.81, 0.85), 0.65, 0.48, True)
dark = material("Graphite / machined steel", (0.085, 0.115, 0.15), 0.8, 0.4, True)
steel = material("Exposed steel", (0.43, 0.51, 0.57), 0.9, 0.35, True)
rubber = material("Recesses", (0.008, 0.013, 0.02), 0.1, 0.8)
warning = material("Safety amber", (0.95, 0.38, 0.045), 0.35, 0.5)
red = material("Laser emitter", (0.55, 0.015, 0.008), 0.2, 0.2, emission=(1, 0.025, 0.008))
cyan = material("Status cyan", (0.015, 0.32, 0.4), 0.25, 0.25, emission=(0.01, 0.6, 0.8))


def finish(obj, name, mat, parent, bevel=0):
    obj.name = name
    obj.data.materials.append(mat)
    if bevel:
        modifier = obj.modifiers.new("Machined edges", "BEVEL")
        modifier.width = bevel
        modifier.segments = 2
        obj.modifiers.new("Face normals", "WEIGHTED_NORMAL")
    if parent:
        obj.parent = parent
    return obj


def box(name, pos, size, mat, parent=None, bevel=0.012):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    obj = bpy.context.object
    obj.dimensions = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, name, mat, parent, bevel)


def cylinder(name, pos, radius, depth, mat, parent=None, axis="Z", vertices=16):
    bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=depth, location=pos)
    obj = bpy.context.object
    if axis == "X":
        obj.rotation_euler.y = math.pi / 2
    elif axis == "Y":
        obj.rotation_euler.x = math.pi / 2
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, name, mat, parent, 0.006)


def empty(name, pos=(0, 0, 0), parent=None):
    obj = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(obj)
    obj.location = pos
    obj.parent = parent
    return obj


def beam(name, start, end, width, mat, parent=None):
    midpoint = (Vector(start) + Vector(end)) / 2
    direction = Vector(end) - Vector(start)
    obj = box(name, midpoint, (width, width, direction.length), mat, parent, 0.008)
    obj.rotation_euler = direction.to_track_quat("Z", "Y").to_euler()
    return obj


# Gimbals aim along local glTF -Z; the base turn makes the resting muzzle face gameplay +Z.
base = empty("TurretBase")
base.rotation_euler.z = math.pi
cylinder("Foundation", (0, 0, 0.07), 0.34, 0.14, dark, base, vertices=8)
cylinder("Base armour", (0, 0, 0.155), 0.29, 0.065, armor, base, vertices=8)
cylinder("Pedestal collar", (0, 0, 0.25), 0.15, 0.14, steel, base)
cylinder("Load column", (0, 0, 0.73), 0.095, 0.88, dark, base, vertices=8)
for sign in (-1, 1):
    box("Column armour", (sign * 0.088, 0, 0.73), (0.048, 0.145, 0.74), armor, base)
    beam("Base brace", (sign * 0.25, 0, 0.16), (sign * 0.10, 0, 0.46), 0.075, steel, base)
    for y in (-0.2, 0.2):
        cylinder("Anchor bolt", (sign * 0.20, y, 0.164), 0.023, 0.025, rubber, base, vertices=6)
for z in (0.47, 0.94):
    box("Column band", (0, -0.09, z), (0.16, 0.018, 0.045), steel, base)
box("Power conduit", (0, -0.105, 0.71), (0.036, 0.035, 0.72), rubber, base)
box("Pedestal indicator", (0, 0.102, 0.90), (0.025, 0.014, 0.18), cyan, base, 0.004)
cylinder("Bearing housing", (0, 0, 1.17), 0.18, 0.13, dark, base)
cylinder("Bearing rim", (0, 0, 1.225), 0.19, 0.025, steel, base)

yaw = empty("TurretYaw", (0, 0, 1.45), base)
cylinder("Rotating turntable", (0, 0, -0.19), 0.16, 0.065, dark, yaw)
for sign in (-1, 1):
    box("Gimbal fork", (sign * 0.225, 0, -0.08), (0.065, 0.19, 0.28), armor, yaw, 0.022)
    cylinder("Pitch bearing", (sign * 0.266, 0, 0), 0.078, 0.035, dark, yaw, "X")
    cylinder("Bearing cap", (sign * 0.288, 0, 0), 0.049, 0.018, steel, yaw, "X", 12)
    cylinder("Bearing light", (sign * 0.300, 0, 0), 0.018, 0.009, cyan, yaw, "X", 12)

pitch = empty("TurretPitch", parent=yaw)
box("Receiver", (0, -0.08, 0), (0.39, 0.45, 0.27), dark, pitch, 0.047)
box("Crown armour", (0, -0.08, 0.13), (0.42, 0.45, 0.075), armor, pitch, 0.026)
box("Underside armour", (0, -0.11, -0.13), (0.32, 0.37, 0.048), steel, pitch)
for sign in (-1, 1):
    box("Cheek armour", (sign * 0.19, -0.10, 0.008), (0.055, 0.31, 0.22), armor, pitch, 0.018)
    box("Receiver seam", (sign * 0.22, -0.10, 0.04), (0.009, 0.24, 0.025), rubber, pitch, 0.002)
    for y in (-0.22, -0.16, -0.10, -0.04):
        box("Cooling louvre", (sign * 0.222, y, -0.052), (0.009, 0.022, 0.050), dark, pitch, 0.002)
    box("Crown caution stripe", (sign * 0.10, -0.09, 0.172), (0.045, 0.28, 0.005), warning, pitch, 0.001)
    for y in (-0.23, 0.07):
        cylinder("Crown fastener", (sign * 0.165, y, 0.171), 0.011, 0.008, rubber, pitch, vertices=6)
box("Rear heat sink", (0, -0.322, 0), (0.29, 0.035, 0.18), rubber, pitch)
for x in (-0.105, -0.07, -0.035, 0, 0.035, 0.07, 0.105):
    box("Heat sink fin", (x, -0.34, 0), (0.012, 0.025, 0.14), steel, pitch, 0.003)
cylinder("Barrel root", (0, 0.18, 0), 0.105, 0.13, steel, pitch, "Y")
cylinder("Emitter barrel", (0, 0.35, 0), 0.066, 0.29, dark, pitch, "Y", 12)
for y in (0.245, 0.31, 0.375, 0.44):
    cylinder("Cooling ring", (0, y, 0), 0.084, 0.024, steel, pitch, "Y", 12)
for sign in (-1, 1):
    box("Emitter rail", (sign * 0.083, 0.34, 0), (0.026, 0.30, 0.068), armor, pitch)
    box("Rail charge strip", (sign * 0.098, 0.35, 0), (0.007, 0.18, 0.013), red, pitch, 0.002)
cylinder("Muzzle shroud", (0, 0.50, 0), 0.094, 0.095, dark, pitch, "Y", 12)
cylinder("Muzzle rim", (0, 0.55, 0), 0.093, 0.015, steel, pitch, "Y", 12)
cylinder("Muzzle recess", (0, 0.56, 0), 0.071, 0.008, rubber, pitch, "Y", 24)
cylinder("Emitter lens", (0, 0.566, 0), 0.047, 0.009, red, pitch, "Y", 24)
empty("TurretMuzzle", (0, 0.572, 0), pitch)
box("Sensor visor", (0.135, 0.165, 0.08), (0.075, 0.05, 0.048), rubber, pitch)
box("Sensor slit", (0.135, 0.193, 0.08), (0.045, 0.008, 0.014), red, pitch, 0.003)

bpy.ops.object.select_all(action="DESELECT")
groups = {}
for obj in list(bpy.context.scene.objects):
    if obj.type == "MESH":
        bpy.context.view_layer.objects.active = obj
        obj.select_set(True)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.cube_project(cube_size=0.7)
        bpy.ops.object.mode_set(mode="OBJECT")
        for modifier in list(obj.modifiers):
            bpy.ops.object.modifier_apply(modifier=modifier.name)
        obj.select_set(False)
        groups.setdefault((obj.parent, obj.active_material), []).append(obj)

for (parent, mat), objects in groups.items():
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    objects[0].name = f"{parent.name} / {mat.name}"
    objects[0].select_set(False)

bpy.ops.object.select_all(action="SELECT")
bpy.ops.export_scene.gltf(
    filepath=str(MODEL),
    export_format="GLB",
    use_selection=True,
    export_apply=True,
    export_animations=False,
    export_cameras=False,
    export_lights=False,
)

bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
bpy.ops.import_scene.gltf(filepath=str(MODEL))

floor = material("Studio floor", (0.028, 0.038, 0.058), 0.2, 0.6)
box("Studio", (0, 0, -0.07), (200, 200, 0.1), floor)
scene = bpy.context.scene
scene.render.engine = "CYCLES"
scene.cycles.samples = 48
scene.cycles.use_denoising = True
scene.world.color = (0.18, 0.18, 0.18)
for name, location, power, color, size in [
    ("Key", (2, 3, 4), 450, (0.78, 0.88, 1), 3),
    ("Rim", (-2, -2, 3), 550, (0.35, 0.65, 1), 2),
    ("Fill", (-1, 2, 1.4), 140, (1, 0.57, 0.35), 2),
]:
    data = bpy.data.lights.new(name, "AREA")
    data.energy, data.color, data.shape, data.size = power, color, "DISK", size
    obj = bpy.data.objects.new(name, data)
    scene.collection.objects.link(obj)
    obj.location = location
    obj.rotation_euler = (Vector((0, 0, 0.9)) - obj.location).to_track_quat("-Z", "Y").to_euler()
bpy.ops.object.camera_add(location=(2.3, -3.4, 2.35))
scene.camera = bpy.context.object
scene.camera.rotation_euler = (Vector((0, -0.05, 0.86)) - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
scene.camera.data.type = "ORTHO"
scene.camera.data.ortho_scale = 2.2
scene.render.resolution_x = 1000
scene.render.resolution_y = 1000
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = "PNG"
scene.render.filepath = "/tmp/cuboid-turret-preview.png"
bpy.ops.render.render(write_still=True)
