#!/usr/bin/env python3
"""Convert 2:1 panoramas to the game's RGBA PNG cube cross (NumPy + Pillow)."""

import argparse
import math
from pathlib import Path
import shutil
import subprocess
import sys

import numpy as np
from PIL import Image


FACES = (
    ("+X", 2, 1),
    ("-X", 0, 1),
    ("+Y", 1, 0),
    ("-Y", 1, 2),
    ("+Z", 1, 1),
    ("-Z", 3, 1),
)
SOURCE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".tif", ".tiff", ".hdr"}


def load_panorama(path):
    if path.suffix.lower() == ".hdr":
        magick = shutil.which("magick")
        if magick is None:
            raise ValueError("Radiance HDR input requires ImageMagick (magick)")
        dimensions = subprocess.check_output([magick, "identify", "-format", "%w %h", str(path)], text=True)
        width, height = map(int, dimensions.split())
        validate_dimensions(width, height)
        raw = subprocess.check_output(
            [
                magick,
                str(path),
                "-alpha",
                "off",
                "-colorspace",
                "RGB",
                "-define",
                "quantum:format=floating-point",
                "-endian",
                "LSB",
                "-depth",
                "32",
                "rgb:-",
            ]
        )
        pixels = np.frombuffer(raw, dtype="<f4").reshape(height, width, 3)
        if not np.isfinite(pixels).all() or np.any(pixels < 0):
            raise ValueError("HDR pixels must be finite and nonnegative")
        return pixels, True
    with Image.open(path) as source:
        validate_dimensions(*source.size)
        return np.asarray(source.convert("RGB")), False


def validate_dimensions(width, height):
    if height < 2 or width != height * 2:
        raise ValueError(f"expected a 2:1 equirectangular panorama, got {width}x{height}")


def face_direction(face, s, t):
    one = np.ones_like(s)
    if face == "+X":
        return one, -t, -s
    if face == "-X":
        return -one, -t, s
    if face == "+Y":
        return s, one, t
    if face == "-Y":
        return s, -one, -t
    if face == "+Z":
        return s, -t, one
    if face == "-Z":
        return -s, -t, -one
    raise ValueError(f"unknown cube face: {face}")


def sample_panorama(pixels, x, y, z, yaw_degrees):
    height, width = pixels.shape[:2]
    u = (np.arctan2(x, z) + math.radians(yaw_degrees)) / math.tau + 0.5
    v = np.arctan2(np.hypot(x, z), y) / math.pi
    px = u * width - 0.5
    py = np.clip(v * height - 0.5, 0, height - 1)
    ix, iy = np.floor(px).astype(int), np.floor(py).astype(int)
    fx, fy = (px - ix)[..., None], (py - iy)[..., None]
    top = pixels[iy, ix % width] * (1 - fx) + pixels[iy, (ix + 1) % width] * fx
    bottom = pixels[np.minimum(iy + 1, height - 1), ix % width] * (1 - fx)
    bottom += pixels[np.minimum(iy + 1, height - 1), (ix + 1) % width] * fx
    return top * (1 - fy) + bottom * fy


def to_display_rgb(pixels, hdr, exposure, tone_map):
    values = pixels if hdr else pixels / 255.0
    values = values * exposure
    if tone_map == "reinhard":
        values = values / (1.0 + values)
    return np.clip(values, 0, 1)


def make_cross(pixels, face_size=1024, yaw_degrees=0.0, samples=2, hdr=False, exposure=1.0, tone_map="clip"):
    validate_dimensions(pixels.shape[1], pixels.shape[0])
    cross = np.zeros((face_size * 3, face_size * 4, 4), dtype=np.uint8)
    cross[:, :, 3] = 255
    columns = np.arange(face_size, dtype=np.float64)[None, :]
    for face, column, row in FACES:
        for start in range(0, face_size, 32):
            end = min(start + 32, face_size)
            rows = np.arange(start, end, dtype=np.float64)[:, None]
            color = np.zeros((end - start, face_size, 3), dtype=np.float64)
            for sy in range(samples):
                for sx in range(samples):
                    s, t = np.broadcast_arrays(
                        (columns + (sx + 0.5) / samples) * 2 / face_size - 1,
                        (rows + (sy + 0.5) / samples) * 2 / face_size - 1,
                    )
                    sampled = sample_panorama(pixels, *face_direction(face, s, t), yaw_degrees)
                    color += to_display_rgb(sampled, hdr, exposure, tone_map)
            target = cross[
                row * face_size + start : row * face_size + end, column * face_size : (column + 1) * face_size, :3
            ]
            target[:] = np.rint(color * (255 / samples**2)).astype(np.uint8)
    return Image.fromarray(cross)


def light_direction(pixel, width, height, yaw_degrees):
    x, y = pixel
    if not (0 <= x < width and 0 <= y < height):
        raise ValueError("--light-pixel must lie inside the source panorama")
    longitude = ((x + 0.5) / width - 0.5) * math.tau - math.radians(yaw_degrees)
    latitude = (0.5 - (y + 0.5) / height) * math.pi
    # Bevy negates world Z when sampling its left-handed cubemap.
    return [math.cos(latitude) * math.sin(longitude), math.sin(latitude), -math.cos(latitude) * math.cos(longitude)]


def convert_file(source, output, args):
    if source.resolve() == output.resolve():
        raise ValueError("source and output must be different files")
    if output.suffix.lower() != ".png":
        raise ValueError("output must have a .png extension")
    pixels, hdr = load_panorama(source)
    direction = None
    if args.light_pixel:
        direction = light_direction(args.light_pixel, pixels.shape[1], pixels.shape[0], args.yaw_degrees)
    exposure = args.exposure if args.exposure is not None else (4.0 if hdr else 1.0)
    tone_map = args.tone_map or ("reinhard" if hdr else "clip")
    cross = make_cross(pixels, args.face_size, args.yaw_degrees, args.samples, hdr, exposure, tone_map)
    output.parent.mkdir(parents=True, exist_ok=True)
    cross.save(output)
    print(f"{source.name} -> {output} ({cross.width}x{cross.height}, RGBA)")
    if direction:
        print("celestial_disc.direction: [" + ", ".join(f"{v:.6f}" for v in direction) + "]")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="panorama file, or directory to convert recursively")
    parser.add_argument("output", type=Path, help="PNG file, or output directory for a source directory")
    parser.add_argument("--face-size", type=int, default=1024, help="pixels per cube face (default: 1024)")
    parser.add_argument("--yaw-degrees", type=float, default=0, help="rotate the sky about Bevy's +Y axis")
    parser.add_argument("--samples", type=int, choices=(1, 2, 4), default=2, help="samples per axis (default: 2)")
    parser.add_argument("--exposure", type=float, help="brightness multiplier (PNG: 1, HDR: 4)")
    parser.add_argument("--tone-map", choices=("clip", "reinhard"), help="PNG default: clip; HDR default: reinhard")
    parser.add_argument(
        "--light-pixel",
        type=float,
        nargs=2,
        metavar=("X", "Y"),
        help="print the map light direction for a sun/moon pixel in the source (top-left origin)",
    )
    args = parser.parse_args(argv)
    if not 1 <= args.face_size <= 4096:
        parser.error("--face-size must be between 1 and 4096")
    if not math.isfinite(args.yaw_degrees):
        parser.error("--yaw-degrees must be finite")
    if args.exposure is not None and (not math.isfinite(args.exposure) or args.exposure <= 0):
        parser.error("--exposure must be finite and positive")
    try:
        if args.source.is_dir():
            if args.light_pixel:
                raise ValueError("--light-pixel requires a single source file")
            if args.output.resolve().is_relative_to(args.source.resolve()):
                raise ValueError("output directory must be outside the source directory")
            sources = sorted(p for p in args.source.rglob("*") if p.suffix.lower() in SOURCE_EXTENSIONS and p.is_file())
            if not sources:
                raise ValueError("no supported panoramas found")
            outputs = [args.output / source.relative_to(args.source).with_suffix(".png") for source in sources]
            if len(set(outputs)) != len(outputs):
                raise ValueError("source filenames would overwrite the same output PNG")
            for source, output in zip(sources, outputs):
                convert_file(source, output, args)
        else:
            convert_file(args.source, args.output, args)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
