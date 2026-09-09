"""Studio lighting and camera shared by every preview.

Run directly to review exported GLBs without rebuilding them:
blender --background --python client/assets/models/modelkit/preview.py -- model.glb ...
"""

import sys
from pathlib import Path

import bpy
from mathutils import Vector

SAMPLES = 32
RESOLUTION = 1100
WORLD_COLOR = (0.11, 0.11, 0.11)
# Area light offsets in units of the subject size and power per square unit of it.
LIGHTS = (((-3, -4, 5), 650), ((3, -1, 3), 350), ((1, 3, 4), 650))
REVIEW_OUTPUT = Path("/tmp/model-texture-review")


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)


def visible_bounds(scene):
    """Low and high corners of every rendered mesh at the current frame."""
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
    return low, high


def studio(scene, look_at, size):
    """Light `look_at` for Cycles with three area lights scaled to `size`.

    Returns an orthographic camera framing `size`; place it with `aim`."""
    look_at = Vector(look_at)
    scene.render.engine = "CYCLES"
    scene.cycles.samples = SAMPLES
    scene.cycles.use_denoising = True
    scene.world.color = WORLD_COLOR
    for offset, power in LIGHTS:
        bpy.ops.object.light_add(type="AREA", location=look_at + Vector(offset) * size)
        light = bpy.context.object
        light.data.energy = power * size * size
        light.data.size = 3 * size
        light.rotation_euler = (
            (look_at - light.location).to_track_quat("-Z", "Y").to_euler()
        )
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    scene.camera = camera
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = size * 1.4
    scene.render.resolution_x = scene.render.resolution_y = RESOLUTION
    scene.render.resolution_percentage = 100
    return camera


def aim(camera, location, look_at):
    camera.location = location
    camera.rotation_euler = (
        (Vector(look_at) - Vector(location)).to_track_quat("-Z", "Y").to_euler()
    )


def floor(material, size=200):
    bpy.ops.mesh.primitive_plane_add(size=size)
    bpy.context.object.data.materials.append(material)


def render(scene, path):
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def review(path):
    """Front, rear, and quarter-size stills of one GLB in its Idle or Hover pose."""
    clear_scene()
    bpy.ops.import_scene.gltf(filepath=str(path))
    scene = bpy.context.scene
    for obj in scene.objects:
        if obj.animation_data:
            obj.animation_data.action = None
            for track in obj.animation_data.nla_tracks:
                track.mute = track.name not in {"Idle", "Hover"}
    scene.frame_set(14)
    low, high = visible_bounds(scene)
    center = (low + high) / 2
    size = max(high - low)
    print(path.name, "visible bounds", tuple(low), tuple(high), flush=True)
    camera = studio(scene, center, size)
    REVIEW_OUTPUT.mkdir(exist_ok=True)
    for view, offset in (("front", (2, -3.5, 1.7)), ("rear", (-2, 3.5, 1.7))):
        aim(camera, center + Vector(offset) * size, center)
        render(scene, REVIEW_OUTPUT / f"{path.stem}-{view}.png")
        scene.render.resolution_percentage = 25
        render(scene, REVIEW_OUTPUT / f"{path.stem}-{view}-small.png")
        scene.render.resolution_percentage = 100


if __name__ == "__main__":
    for argument in sys.argv[sys.argv.index("--") + 1 :]:
        review(Path(argument).resolve())
