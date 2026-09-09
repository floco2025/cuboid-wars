"""Render GLBs with consistent studio lighting: Blender --python this.py -- model.glb ..."""

import sys
from pathlib import Path

import bpy
from mathutils import Vector


def render(path):
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    bpy.ops.import_scene.gltf(filepath=str(path))
    scene = bpy.context.scene
    for obj in scene.objects:
        if obj.animation_data:
            obj.animation_data.action = None
            for track in obj.animation_data.nla_tracks:
                track.mute = track.name not in {"Idle", "Hover"}
    scene.frame_set(14)
    graph = bpy.context.evaluated_depsgraph_get()
    points = [
        obj.matrix_world @ obj.data.vertices[index].co
        for source in scene.objects
        if source.type == "MESH" and source.visible_get() and not source.hide_render
        for obj in [source.evaluated_get(graph)]
        for index in {vertex for face in obj.data.polygons for vertex in face.vertices}
    ]
    low = Vector(tuple(min(p[i] for p in points) for i in range(3)))
    high = Vector(tuple(max(p[i] for p in points) for i in range(3)))
    center = (low + high) / 2
    size = max(high - low)
    print(path.name, "visible bounds", tuple(low), tuple(high), flush=True)
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 32
    scene.cycles.use_denoising = True
    scene.world.color = (0.11, 0.11, 0.11)
    for offset, power in (((-3, -4, 5), 650), ((3, -1, 3), 350), ((1, 3, 4), 650)):
        bpy.ops.object.light_add(type="AREA", location=center + Vector(offset) * size)
        light = bpy.context.object
        light.data.energy = power * size * size
        light.data.size = 3 * size
        light.rotation_euler = (
            (center - light.location).to_track_quat("-Z", "Y").to_euler()
        )
    bpy.ops.object.camera_add()
    scene.camera = bpy.context.object
    scene.camera.data.type = "ORTHO"
    scene.camera.data.ortho_scale = size * 1.4
    scene.render.resolution_x = scene.render.resolution_y = 1100
    scene.render.resolution_percentage = 100
    output = Path("/tmp/model-texture-review")
    output.mkdir(exist_ok=True)
    for view, offset in (("front", (2, -3.5, 1.7)), ("rear", (-2, 3.5, 1.7))):
        scene.camera.location = center + Vector(offset) * size
        scene.camera.rotation_euler = (
            (center - scene.camera.location).to_track_quat("-Z", "Y").to_euler()
        )
        scene.render.filepath = str(output / f"{path.stem}-{view}.png")
        bpy.ops.render.render(write_still=True)
        scene.render.resolution_percentage = 25
        scene.render.filepath = str(output / f"{path.stem}-{view}-small.png")
        bpy.ops.render.render(write_still=True)
        scene.render.resolution_percentage = 100


for argument in sys.argv[sys.argv.index("--") + 1 :]:
    render(Path(argument).resolve())
