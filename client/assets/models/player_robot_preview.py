"""Render a motion study: blender --background --python client/assets/models/player_robot_preview.py.

Frames are written to /tmp/player-robot-mocap-preview for encoding with ffmpeg.
Add -- --hands for a textured hand close-up at /tmp/player-robot-hand.png.
Add -- --jump for the takeoff, descent and landing clips.
"""

import sys
from pathlib import Path

import bpy
from mathutils import Vector

MODEL = Path(__file__).resolve().with_name("player_robot.glb")
HANDS = "--hands" in sys.argv
JUMP = "--jump" in sys.argv
OUTPUT = Path(
    "/tmp/player-robot-jump-preview" if JUMP else "/tmp/player-robot-mocap-preview"
)
OUTPUT.mkdir(exist_ok=True)
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
scene = bpy.context.scene
scene.render.fps = 24
scene.render.engine = "BLENDER_WORKBENCH"
scene.display.shading.light = "STUDIO"
scene.display.shading.studiolight_rotate_z = 0.4
scene.display.shading.color_type = "MATERIAL"
scene.display.shading.show_shadows = True
scene.display.shading.show_cavity = True
scene.display.shading.cavity_type = "BOTH"
scene.display.shading.background_type = "WORLD"
scene.world.color = (0.035, 0.05, 0.065)
scene.render.resolution_x = 1440
scene.render.resolution_y = 640
scene.render.resolution_percentage = 100

clips = (
    ("Jump", "Fall", "Land", "Idle") if JUMP else ("Walk", "Run", "Climb", "StrafeLeft")
)
for index, clip in enumerate(("Idle",) if HANDS else clips):
    previous = set(scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(MODEL))
    imported = set(scene.objects) - previous
    rig = next(obj for obj in imported if obj.type == "ARMATURE")
    rig.location.x = 0 if HANDS else (index - 1.5) * 1.2
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = track.name != clip
        if not track.mute:
            for strip in track.strips:
                if clip not in ("Jump", "Fall", "Land"):
                    strip.repeat = 10
    if HANDS:
        continue
    bpy.ops.object.text_add(location=(rig.location.x, -0.1, -0.2))
    text = bpy.context.object
    text.data.body = clip
    text.data.align_x = "CENTER"
    text.data.size = 0.16
    text.rotation_euler = (
        (Vector((0, -10, 3)) - text.location).to_track_quat("Z", "Y").to_euler()
    )

bpy.ops.object.camera_add(location=(3.1, -10, 3.3))
scene.camera = bpy.context.object
scene.camera.rotation_euler = (
    (Vector((0, 0, 0.85)) - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
)
scene.camera.data.type = "ORTHO"
scene.camera.data.ortho_scale = 5.7
scene.render.image_settings.file_format = "PNG"
scene.render.filepath = str(OUTPUT / "frame-")
scene.frame_start = 0
scene.frame_end = 71
if HANDS:
    scene.frame_set(10)
    target = rig.matrix_world @ rig.pose.bones["Hand.R"].head + Vector((0, 0, -0.075))
    scene.camera.location = target + Vector((0.1, -0.8, 0.16))
    scene.camera.rotation_euler = (
        (target - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
    )
    scene.camera.data.ortho_scale = 0.25
    scene.render.resolution_x = scene.render.resolution_y = 900
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 40
    scene.cycles.use_denoising = True
    for offset, power in (((-1, -2, 2), 180), ((1, -1, 1), 100)):
        bpy.ops.object.light_add(type="AREA", location=target + Vector(offset))
        light = bpy.context.object
        light.data.energy = power
        light.data.size = 2
        light.rotation_euler = (
            (target - light.location).to_track_quat("-Z", "Y").to_euler()
        )
    scene.render.filepath = "/tmp/player-robot-hand.png"
    bpy.ops.render.render(write_still=True)
else:
    bpy.ops.render.render(animation=True)
