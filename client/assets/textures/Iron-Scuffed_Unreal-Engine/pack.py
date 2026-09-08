"""Run with Blender --background --python to pack Bevy/glTF data maps."""

from pathlib import Path

import bpy
import numpy as np

HERE = Path(__file__).resolve().parent
PREFIX = "Iron-Scuffed"


def read(name):
    image = bpy.data.images.load(str(HERE / name), check_existing=False)
    image.colorspace_settings.name = "Non-Color"
    pixels = np.empty(image.size[0] * image.size[1] * 4, dtype=np.float32)
    image.pixels.foreach_get(pixels)
    return tuple(image.size), pixels.reshape(-1, 4)


def write(name, size, pixels):
    image = bpy.data.images.new(name, width=size[0], height=size[1], alpha=False)
    image.colorspace_settings.name = "Non-Color"
    image.pixels.foreach_set(pixels.ravel())
    image.filepath_raw = str(HERE / name)
    image.file_format = "PNG"
    image.save()


size, roughness = read(PREFIX + "_roughness.png")
metal_size, metallic = read(PREFIX + "_metallic.png")
assert size == metal_size
packed = np.ones_like(roughness)
packed[:, 1] = roughness[:, 0]
packed[:, 2] = metallic[:, 0]
write(PREFIX + "_metallic-roughness.png", size, packed)
size, normal = read("Iron-Scuffed_normal.png")
normal[:, 1] = 1 - normal[:, 1]
write(PREFIX + "_normal-gl.png", size, normal)
if not (HERE / (PREFIX + "_ao.png")).exists():
    write(PREFIX + "_ao.png", (1, 1), np.ones((1, 4), dtype=np.float32))
