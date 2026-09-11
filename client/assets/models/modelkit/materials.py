"""Catalog PBR materials and metre-scaled UVs shared by generated models."""

import json
import math
from pathlib import Path

import bpy
import numpy as np
from io_scene_gltf2.blender.imp.pbrMetallicRoughness import get_settings_group

ROOT = Path(__file__).resolve().parents[4]
ASSETS = ROOT / "client/assets"
CATALOG = json.loads((ROOT / "config/client/assets.json").read_text())["materials"]
# Catalog textures are downscaled to this edge length before embedding.
EMBEDDED_TEXTURE_SIZE = 1024
TUNING_FIELDS = {
    "tile_size",
    "tint",
    "color_contrast",
    "normal_strength",
    "roughness_factor",
}


def plain_material(name, color, metallic=0.0, roughness=0.4):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*color, 1)
    mat.use_nodes = True
    shader = mat.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = (*color, 1)
    shader.inputs["Metallic"].default_value = metallic
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Emission Color"].default_value = (*color, 1)
    shader.inputs["Emission Strength"].default_value = 0.0
    return mat


class ModelMaterials:
    def __init__(self, path):
        self.path = Path(path)
        settings = json.loads(self.path.read_text())
        unknown = settings.keys() - {"materials", "wear"}
        if unknown:
            raise ValueError(f"{self.path.name}: unknown sections {sorted(unknown)}")
        self.wear = settings.get("wear", {})
        self.materials = {}
        for key, definition in settings["materials"].items():
            try:
                self.materials[key] = self.create(key, definition)
            except (ValueError, KeyError, TypeError) as error:
                raise ValueError(f"{self.path.name}: materials.{key}: {error}") from error

    def __getitem__(self, key):
        return self.materials[key]

    @staticmethod
    def create(key, definition):
        allowed = {
            "name",
            "source",
            "metallic",
            "emission",
            "emission_color",
            "backface_culling",
        }
        allowed |= TUNING_FIELDS if "source" in definition else {"color", "roughness"}
        unknown = definition.keys() - allowed
        if unknown:
            raise ValueError(f"unknown fields {sorted(unknown)}")
        for field in ("metallic", "roughness", "emission"):
            if field in definition:
                value = definition[field]
                limit = math.inf if field == "emission" else 1
                if (
                    isinstance(value, bool)
                    or not isinstance(value, (int, float))
                    or not math.isfinite(value)
                    or not 0 <= value <= limit
                ):
                    raise ValueError(f"{field} is outside its valid range")
        for field in ("color", "emission_color"):
            if field in definition:
                value = np.asarray(definition[field], dtype=float)
                if value.shape != (3,) or not np.all(np.isfinite(value) & (value >= 0) & (value <= 1)):
                    raise ValueError(f"{field} must contain three values from 0 to 1")
        if "backface_culling" in definition and not isinstance(definition["backface_culling"], bool):
            raise ValueError("backface_culling must be a boolean")
        name = definition.get("name", key)
        if "source" in definition:
            mat = catalog_material(
                definition["source"],
                name,
                tuning={field: definition[field] for field in TUNING_FIELDS if field in definition},
            )
        else:
            mat = plain_material(name, definition["color"], roughness=definition.get("roughness", 0.4))
        shader = mat.node_tree.nodes.get("Principled BSDF")
        if "metallic" in definition:
            socket = shader.inputs["Metallic"]
            for link in list(socket.links):
                mat.node_tree.links.remove(link)
            socket.default_value = definition["metallic"]
        if "emission_color" in definition:
            shader.inputs["Emission Color"].default_value = (
                *definition["emission_color"],
                1,
            )
        shader.inputs["Emission Strength"].default_value = definition.get("emission", 0)
        mat.use_backface_culling = definition.get("backface_culling", False)
        return mat


def catalog_material(key, name=None, *, tuning=None):
    definition = CATALOG[key]
    tuning = tuning or {}
    unknown = tuning.keys() - TUNING_FIELDS
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
        if max(image.size) > EMBEDDED_TEXTURE_SIZE:
            image.scale(EMBEDDED_TEXTURE_SIZE, EMBEDDED_TEXTURE_SIZE)
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
                if "normal-dx" in path.lower():
                    # glTF requires OpenGL Y.
                    rgba[:, 1] = 1 - rgba[:, 1]
                elif "normal-gl" not in path.lower():
                    raise ValueError(f"{key}: normal map must be named -dx or -gl")
                if normal_strength != 1.0:
                    # Bevy's glTF loader ignores normalTexture.scale, so bake strength.
                    vectors = rgba[:, :3] * 2 - 1
                    vectors[:, :2] *= normal_strength
                    vectors /= np.maximum(np.linalg.norm(vectors, axis=1, keepdims=True), 1e-8)
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
    if definition["textures"]["occlusion"] == definition["textures"]["metallic_roughness"]:
        links.new(separate.outputs["Red"], output.inputs["Occlusion"])
    else:
        links.new(texture("occlusion"), output.inputs["Occlusion"])
    if definition["metallic"] == 0.0:
        links.remove(shader.inputs["Metallic"].links[0])
        shader.inputs["Metallic"].default_value = 0.0
    return mat


def project_uv(obj, mat, cylindrical=False):
    tile = mat.get("tile_size", 1.0)
    uv = obj.data.uv_layers.active or obj.data.uv_layers.new()
    for face in obj.data.polygons:
        points = [obj.data.vertices[obj.data.loops[i].vertex_index].co for i in face.loop_indices]
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
