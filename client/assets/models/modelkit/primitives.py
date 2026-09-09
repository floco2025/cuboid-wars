"""Mesh primitives shared by the generators.

Every primitive is named, given one material, bevelled with weighted normals when
`bevel` is set, UV-projected when its material is a catalog texture, and handed to
`attach(obj)` to join its rig or parent. `smooth` is None, "quads" (cylinder sides),
or "all"; "all" is reapplied after the bevel so its new faces stay smooth.
"""

import math

import bpy
from mathutils import Vector

from .materials import project_uv


def child_of(parent):
    """`attach` callback parenting the primitive to `parent`."""

    def attach(obj):
        obj.parent = parent

    return attach


def rigged(parts, bone):
    """`attach` callback weighting the primitive fully to `bone` and listing it in
    `parts`."""

    def attach(obj):
        group = obj.vertex_groups.new(name=bone)
        group.add(list(range(len(obj.data.vertices))), 1.0, "REPLACE")
        parts.append(obj)

    return attach


def set_smooth(obj, smooth):
    if smooth == "all":
        for face in obj.data.polygons:
            face.use_smooth = True
    elif smooth == "quads":
        for face in obj.data.polygons:
            face.use_smooth = len(face.vertices) == 4


def finish(obj, name, mat, attach, bevel, segments, smooth=None, cylindrical=False):
    obj.name = name
    obj.data.name = name
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    set_smooth(obj, smooth)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        modifier = obj.modifiers.new("Edge radii", "BEVEL")
        modifier.width, modifier.segments = bevel, segments
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        modifier = obj.modifiers.new("Corner normals", "WEIGHTED_NORMAL")
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        if smooth == "all":
            set_smooth(obj, smooth)
    if "tile_size" in mat:
        # Only catalog textures read metre-scaled UVs; baked atlases are unwrapped by
        # the bake and plain materials read none.
        project_uv(obj, mat, cylindrical)
    attach(obj)
    return obj


def box(name, pos, size, mat, attach, bevel, segments, smooth=None):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    obj = bpy.context.object
    obj.dimensions = size
    return finish(obj, name, mat, attach, bevel, segments, smooth)


def orient(obj, axis):
    if axis == "X":
        obj.rotation_euler.y = math.pi / 2
    elif axis == "Y":
        obj.rotation_euler.x = math.pi / 2


def cylinder(
    name, pos, radius, depth, mat, attach, axis, vertices, bevel, segments, smooth
):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=pos
    )
    obj = bpy.context.object
    orient(obj, axis)
    return finish(obj, name, mat, attach, bevel, segments, smooth, cylindrical=True)


def rod(name, start, end, radius, mat, attach, vertices, bevel, segments, smooth):
    direction = Vector(end) - Vector(start)
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices,
        radius=radius,
        depth=direction.length,
        location=(Vector(start) + Vector(end)) / 2,
    )
    obj = bpy.context.object
    obj.rotation_euler = direction.to_track_quat("Z", "Y").to_euler()
    return finish(obj, name, mat, attach, bevel, segments, smooth, cylindrical=True)


def sphere(name, pos, size, mat, attach, segments, ring_count):
    bpy.ops.mesh.primitive_uv_sphere_add(
        segments=segments, ring_count=ring_count, radius=1, location=pos
    )
    obj = bpy.context.object
    obj.scale = size
    return finish(obj, name, mat, attach, 0, 0, smooth="all")


def empty(name, location=(0, 0, 0), parent=None):
    obj = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(obj)
    obj.location = location
    obj.parent = parent
    return obj


def label(text, pos, size, rotation, mat, attach, extrude):
    bpy.ops.object.text_add(location=pos)
    obj = bpy.context.object
    obj.data.body = text
    obj.data.size = size
    obj.data.align_x = "CENTER"
    obj.data.extrude = extrude
    obj.rotation_euler = rotation
    bpy.ops.object.convert(target="MESH")
    return finish(bpy.context.object, text, mat, attach, 0, 0)
