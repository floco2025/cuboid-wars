"""What the wheeled actors share: parts bound to rig bones, the Idle/Drive rig and
clips, the export with its clip contract, and the preview."""

import math
from pathlib import Path

import bpy
from mathutils import Euler, Vector

from . import preview, primitives
from .glb import channel_target, rewrite_glb_json
from .materials import plain_material
from .wear import bake_wear

FPS = 30
CLIPS = ("Idle", "Drive")
IDLE_SECS = 4
DRIVE_SECS = 1
FLOOR_COLOR = (0.075, 0.09, 0.11)


class Parts:
    """Mesh parts of one actor; every primitive joins the vertex group of its bone.

    `bevel_segments(width)` is the actor's rule for how finely an edge of that width
    is rounded."""

    def __init__(self, ink, bevel_segments):
        self.objects = []
        self.ink = ink
        self.bevel_segments = bevel_segments

    def attach(self, bone):
        return primitives.rigged(self.objects, bone)

    def box(self, name, pos, size, mat, bone="Hull", bevel=0.018):
        return primitives.box(name, pos, size, mat, self.attach(bone), bevel, self.bevel_segments(bevel))

    def cylinder(self, name, pos, radius, depth, mat, bone="Hull", axis="Z", vertices=32):
        return primitives.cylinder(
            name,
            pos,
            radius,
            depth,
            mat,
            self.attach(bone),
            axis,
            vertices,
            0.006,
            self.bevel_segments(0.006),
            "quads",
        )

    def label(self, text, pos, size, rotation, bone="Hull"):
        return primitives.label(text, pos, size, rotation, self.ink, self.attach(bone), 0.0005)


def assemble(parts, armour, wear, model, name):
    """Bake the armour parts and join every part into one triangulated mesh."""
    armour_parts = [obj for obj in parts if obj.active_material == armour]
    objects = [obj for obj in parts if obj.active_material != armour]
    objects.append(bake_wear(armour_parts, armour, wear, model))
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    mesh = bpy.context.object
    mesh.name = name
    triangulate = mesh.modifiers.new("Export triangles", "TRIANGULATE")
    triangulate.keep_custom_normals = True
    bpy.ops.object.modifier_apply(modifier=triangulate.name)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return mesh


def build_rig(bones, mesh, name):
    """Rest the mesh on the floor, lower the bones with it, and bind it to an
    armature."""
    # Tyre blocks extend slightly beyond the circular tyre surface.
    low = min(vertex.co.z for vertex in mesh.data.vertices)
    for vertex in mesh.data.vertices:
        vertex.co.z -= low
    armature = bpy.data.armatures.new(f"{name} mechanism")
    rig = bpy.data.objects.new(f"{name}Rig", armature)
    bpy.context.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    rig.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    for bone_name, pos, parent in bones:
        head = Vector(pos)
        if bone_name != "Root":
            head.z -= low
        bone = armature.edit_bones.new(bone_name)
        bone.head = head
        bone.tail = head + Vector((0, 0, 0.05))
        if parent:
            bone.parent = armature.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    mesh.parent = rig
    mesh.modifiers.new("Rigid mechanisms", "ARMATURE").object = rig
    return rig


def animate(rig, sensor_sway, hull_bob):
    """Key Idle (the sensor sweeps by the `sensor_sway` pitch and yaw amplitudes) and
    Drive (the wheels turn once while the hull bobs `hull_bob` metres)."""
    rest = {bone.name: bone.matrix_local.to_quaternion() for bone in rig.data.bones}
    pitch, yaw = sensor_sway
    rig.animation_data_create()
    scene = bpy.context.scene
    scene.render.fps = FPS
    for clip in CLIPS:
        action = bpy.data.actions.new(clip)
        action.use_fake_user = True
        rig.animation_data.action = action
        frames = FPS * (IDLE_SECS if clip == "Idle" else DRIVE_SECS)
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
                            pitch * math.sin(math.tau * t),
                            0,
                            yaw * math.sin(math.tau * t) if clip == "Idle" else 0,
                        )
                    ).to_quaternion()
                if bone.name == "Hull" and clip == "Drive":
                    bone.location = rest[bone.name].inverted() @ Vector(
                        (0, 0, hull_bob * (1 - math.cos(2 * math.tau * t)))
                    )
                bone.rotation_quaternion = rest[bone.name].inverted() @ rotation @ rest[bone.name]
                for channel in ("location", "rotation_quaternion"):
                    bone.keyframe_insert(data_path=channel, frame=frame)
    rig.animation_data.action = bpy.data.actions["Idle"]
    scene.frame_set(0)


def export(model, rig, mesh):
    bpy.ops.object.select_all(action="DESELECT")
    mesh.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.export_scene.gltf(
        filepath=str(model),
        export_format="GLB",
        use_selection=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_anim_single_armature=True,
        export_force_sampling=True,
        export_frame_range=False,
        export_tangents=True,
        export_cameras=False,
        export_lights=False,
    )

    def order_clips(document):
        document["animations"].sort(key=lambda clip: CLIPS.index(clip["name"]))
        for animation in document["animations"]:
            animation["channels"] = [
                channel
                for channel in animation["channels"]
                if (
                    channel_target(document, channel) == "Sensor"
                    if animation["name"] == "Idle"
                    else channel_target(document, channel).startswith(("Wheel", "Hull"))
                )
            ]

    rewrite_glb_json(model, order_clips)
    print(f"Exported {model.stem}:", model.stat().st_size, "bytes")
    print(
        "Bounds:",
        tuple(
            round(
                max(v.co[i] for v in mesh.data.vertices) - min(v.co[i] for v in mesh.data.vertices),
                3,
            )
            for i in range(3)
        ),
    )


def preview_actor(model, camera, look_at, ortho_scale, extra_views=(), motion=False):
    """Re-import the GLB and render its Idle pose from `camera`; each extra view is
    (suffix, camera, look_at, ortho_scale, resolution). `motion` adds a workbench
    frame sequence of the Drive clip starting and stopping."""
    scene = bpy.context.scene
    preview.clear_scene()
    bpy.ops.import_scene.gltf(filepath=str(model))
    rig = next(obj for obj in scene.objects if obj.type == "ARMATURE")
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = track.name != "Idle"
    scene.frame_set(0)
    low, high = preview.visible_bounds(scene)
    preview.floor(plain_material("Studio floor", FLOOR_COLOR))
    studio_camera = preview.studio(scene, look_at, max(high - low))
    for suffix, view_camera, view_look_at, view_scale, resolution in (
        ("preview", camera, look_at, ortho_scale, preview.RESOLUTION),
        *extra_views,
    ):
        preview.aim(studio_camera, view_camera, view_look_at)
        studio_camera.data.ortho_scale = view_scale
        scene.render.resolution_x = scene.render.resolution_y = resolution
        preview.render(scene, f"/tmp/{model.stem}-{suffix}.png")
    if not motion:
        return
    output = Path(f"/tmp/{model.stem}-motion")
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
            image = getattr(shader.inputs["Base Color"].links[0].from_node, "image", None)
            if image:
                mat.diffuse_color = tuple(image.pixels[:4])
    preview.aim(studio_camera, camera, look_at)
    studio_camera.data.ortho_scale = ortho_scale
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
        preview.render(scene, output / f"frame-{frame:04d}.png")
