"""Catalog PBR materials and metre-scaled UVs shared by generated models."""

import json
import math
from pathlib import Path

import bpy
import numpy as np
from io_scene_gltf2.blender.imp.pbrMetallicRoughness import get_settings_group

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / "client/assets"
CATALOG = json.loads((ROOT / "config/client/assets.json").read_text())["materials"]


def catalog_material(key, name=None, *, tuning=None):
    definition = CATALOG[key]
    tuning = tuning or {}
    unknown = tuning.keys() - {
        "tile_size",
        "tint",
        "color_contrast",
        "normal_strength",
        "roughness_factor",
    }
    if unknown:
        raise ValueError(f"Unknown material settings for {key}: {sorted(unknown)}")
    tile_size = tuning.get("tile_size", definition["tile_size"])
    tint = np.asarray(tuning.get("tint", [1, 1, 1]), dtype=np.float32)
    contrast = tuning.get("color_contrast", 1.0)
    normal_strength = tuning.get("normal_strength", 1.0)
    roughness_factor = tuning.get("roughness_factor", 1.0) * definition["roughness"]
    if not math.isfinite(tile_size) or tile_size <= 0:
        raise ValueError(f"{key}: tile_size must be positive")
    if tint.shape != (3,) or not np.all(np.isfinite(tint) & (tint >= 0) & (tint <= 1)):
        raise ValueError(f"{key}: tint must contain three values from 0 to 1")
    for field, value in (
        ("color_contrast", contrast),
        ("normal_strength", normal_strength),
        ("roughness_factor", roughness_factor),
    ):
        if not math.isfinite(value) or value < 0:
            raise ValueError(f"{key}: {field} must be nonnegative")
    mat = bpy.data.materials.new(name or key)
    mat.use_nodes = True
    mat["tile_size"] = tile_size
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    shader = nodes.get("Principled BSDF")

    def texture(channel):
        path = definition["textures"][channel]
        image = bpy.data.images.load(str(ASSETS / path), check_existing=False)
        if channel != "base_color":
            image.colorspace_settings.name = "Non-Color"
        if image.size[0] > 1024 or image.size[1] > 1024:
            image.scale(1024, 1024)
        if channel in ("base_color", "metallic_roughness", "normal"):
            pixels = np.empty(image.size[0] * image.size[1] * 4, dtype=np.float32)
            image.pixels.foreach_get(pixels)
            rgba = pixels.reshape(-1, 4)
            if channel == "base_color":
                rgb = rgba[:, :3]
                mean = rgb.mean(axis=0)
                rgb[:] = np.clip((mean + (rgb - mean) * contrast) * tint, 0, 1)
            elif channel == "metallic_roughness":
                rgba[:, 1] = np.clip(rgba[:, 1] * roughness_factor, 0, 1)
                rgba[:, 2] *= definition["metallic"]
            else:
                if "normal-gl" not in path:
                    # The UE source maps use DirectX Y; glTF requires OpenGL Y.
                    rgba[:, 1] = 1 - rgba[:, 1]
                if normal_strength != 1.0:
                    # Bevy's glTF loader ignores normalTexture.scale, so bake strength.
                    vectors = rgba[:, :3] * 2 - 1
                    vectors[:, :2] *= normal_strength
                    vectors /= np.maximum(
                        np.linalg.norm(vectors, axis=1, keepdims=True), 1e-8
                    )
                    rgba[:, :3] = vectors * 0.5 + 0.5
            image.pixels.foreach_set(pixels)
        image.pack()
        node = nodes.new("ShaderNodeTexImage")
        node.image = image
        return node.outputs["Color"]

    links.new(texture("base_color"), shader.inputs["Base Color"])
    separate = nodes.new("ShaderNodeSeparateColor")
    links.new(texture("metallic_roughness"), separate.inputs["Color"])
    links.new(separate.outputs["Green"], shader.inputs["Roughness"])
    links.new(separate.outputs["Blue"], shader.inputs["Metallic"])
    normal = nodes.new("ShaderNodeNormalMap")
    links.new(texture("normal"), normal.inputs["Color"])
    links.new(normal.outputs["Normal"], shader.inputs["Normal"])
    group = get_settings_group()
    output = nodes.new("ShaderNodeGroup")
    output.node_tree = group
    links.new(texture("occlusion"), output.inputs["Occlusion"])
    if definition["metallic"] == 0.0:
        links.remove(shader.inputs["Metallic"].links[0])
        shader.inputs["Metallic"].default_value = 0.0
    return mat


def project_uv(obj, mat, cylindrical=False):
    tile = mat.get("tile_size", 1.0)
    uv = obj.data.uv_layers.active or obj.data.uv_layers.new()
    for face in obj.data.polygons:
        points = [
            obj.data.vertices[obj.data.loops[i].vertex_index].co
            for i in face.loop_indices
        ]
        if cylindrical and abs(face.normal.z) < 0.5:
            angles = [math.atan2(point.y, point.x) for point in points]
            if max(angles) - min(angles) > math.pi:
                angles = [angle + math.tau if angle < 0 else angle for angle in angles]
            for loop, point, angle in zip(face.loop_indices, points, angles):
                uv.data[loop].uv = (
                    angle * math.hypot(point.x, point.y) / tile,
                    point.z / tile,
                )
        else:
            axis = max(range(3), key=lambda i: abs(face.normal[i]))
            axes = ((1, 2), (0, 2), (0, 1))[axis]
            for loop, point in zip(face.loop_indices, points):
                uv.data[loop].uv = (point[axes[0]] / tile, point[axes[1]] / tile)
