"""Generate the seamless rock texture set beside this script.

Writes rock_albedo.png, rock_normal-gl.png, rock_ao.png, and
rock_metallic-roughness.png (1024 px, one square metre): a weathered
grey-brown stone surface built from periodic noise, so every map tiles: mottled grain
with sparse weathering pits and faint bedding bands, and no crack network,
which would read as scales on a rounded body.
Run from anywhere: python3 client/assets/textures/rocks/rock.py
"""

from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 1024
SEED = 7


def periodic_value_noise(rng, size, period):
    """Smooth noise whose lattice wraps at `size`, so the result tiles."""
    lattice = rng.random((period, period))
    y, x = np.mgrid[0:size, 0:size] * (period / size)
    x0, y0 = np.floor(x).astype(int), np.floor(y).astype(int)
    tx, ty = x - x0, y - y0
    tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
    x1, y1 = (x0 + 1) % period, (y0 + 1) % period
    top = lattice[y0 % period, x0 % period] * (1 - tx) + lattice[y0 % period, x1] * tx
    bottom = lattice[y1, x0 % period] * (1 - tx) + lattice[y1, x1] * tx
    return top * (1 - ty) + bottom * ty


def fbm(rng, size, period, octaves, gain=0.5):
    total, amplitude, weight = np.zeros((size, size)), 1.0, 0.0
    for _ in range(octaves):
        total += amplitude * periodic_value_noise(rng, size, period)
        weight += amplitude
        period *= 2
        amplitude *= gain
    return total / weight


def normal_map(height, strength):
    dx = (np.roll(height, -1, axis=1) - np.roll(height, 1, axis=1)) * strength
    dy = (np.roll(height, -1, axis=0) - np.roll(height, 1, axis=0)) * strength
    normal = np.dstack([-dx, dy, np.ones_like(height)])
    normal /= np.linalg.norm(normal, axis=2, keepdims=True)
    return normal


def to_image(array):
    return Image.fromarray(np.clip(array * 255 + 0.5, 0, 255).astype(np.uint8))


def main():
    rng = np.random.default_rng(SEED)
    out = Path(__file__).resolve().parent

    grain = fbm(rng, SIZE, 8, 7)
    mottle = fbm(rng, SIZE, 3, 4)
    drift = fbm(rng, SIZE, 2, 3)
    # Sparse weathering pits, and faint bands like the bedding of sandstone.
    pits = np.clip((fbm(rng, SIZE, 24, 3, 0.6) - 0.6) / 0.22, 0, 1) ** 1.5
    bands = 1 - np.abs(2 * fbm(rng, SIZE, 2, 3) - 1)
    speckle = periodic_value_noise(rng, SIZE, 256)

    height = 0.5 * grain + 0.35 * mottle + 0.12 * bands - 0.45 * pits + 0.05 * speckle
    height = (height - height.min()) / (height.max() - height.min())

    base = np.array([0.40, 0.38, 0.35])
    warm_tint = np.array([0.47, 0.40, 0.31])
    cool_tint = np.array([0.34, 0.36, 0.39])
    lightness = 0.7 + 0.55 * (0.55 * mottle + 0.3 * grain + 0.15 * bands)
    colour = base[None, None, :] * lightness[..., None]
    colour = colour * (1 - 0.35 * drift[..., None]) + warm_tint[None, None, :] * 0.35 * (drift * lightness)[..., None]
    colour = (
        colour * (1 - 0.25 * (1 - drift)[..., None])
        + cool_tint[None, None, :] * 0.25 * ((1 - drift) * lightness)[..., None]
    )
    colour *= (1 - 0.3 * pits)[..., None]
    colour *= (0.86 + 0.28 * speckle)[..., None]

    ao = 1 - 0.5 * pits - 0.12 * (1 - grain) - 0.08 * (1 - mottle)
    roughness = 0.8 + 0.15 * (1 - height) + 0.05 * pits
    metallic_roughness = np.dstack([np.zeros_like(height), np.clip(roughness, 0, 1), np.zeros_like(height)])

    to_image(colour).save(out / "rock_albedo.png")
    to_image(normal_map(height, 5.0) * 0.5 + 0.5).save(out / "rock_normal-gl.png")
    to_image(np.clip(ao, 0, 1)).save(out / "rock_ao.png")
    to_image(metallic_roughness).save(out / "rock_metallic-roughness.png")


if __name__ == "__main__":
    main()
