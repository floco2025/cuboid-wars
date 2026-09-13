import math
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from convert_skybox import light_direction, load_panorama, main, make_cross, sample_panorama, to_display_rgb


class SkyboxConversionTests(unittest.TestCase):
    def test_face_centers_and_edges_preserve_panorama_orientation(self):
        width, height = 1024, 512
        longitude = ((np.arange(width) + 0.5) / width - 0.5) * math.tau
        latitude = (0.5 - (np.arange(height) + 0.5) / height) * math.pi
        x = np.cos(latitude[:, None]) * np.sin(longitude)
        y = np.broadcast_to(np.sin(latitude[:, None]), (height, width))
        z = np.cos(latitude[:, None]) * np.cos(longitude)
        panorama = np.rint((np.stack((x, y, z), axis=-1) + 1) * 127.5).astype(np.uint8)
        cross = np.asarray(make_cross(panorama, face_size=17, samples=1))
        for column, row, expected in [
            (2, 1, [1, 0, 0]),
            (0, 1, [-1, 0, 0]),
            (1, 0, [0, 1, 0]),
            (1, 2, [0, -1, 0]),
            (1, 1, [0, 0, 1]),
            (3, 1, [0, 0, -1]),
        ]:
            actual = cross[row * 17 + 8, column * 17 + 8, :3] / 127.5 - 1
            np.testing.assert_allclose(actual, expected, atol=0.01)
        for a, b in [
            (cross[16, 17:34], cross[17, 17:34]),
            (cross[33, 17:34], cross[34, 17:34]),
            (cross[17:34, 16], cross[17:34, 17]),
            (cross[17:34, 33], cross[17:34, 34]),
            (cross[17:34, 50], cross[17:34, 51]),
            (cross[17:34, 67], cross[17:34, 0]),
        ]:
            np.testing.assert_allclose(a.astype(float), b.astype(float), atol=6)

    def test_sampling_wraps_longitude_and_clamps_poles(self):
        pixels = np.zeros((2, 4, 3), dtype=np.uint8)
        pixels[:, 0] = 100
        pixels[:, -1] = 200
        seam = sample_panorama(pixels, np.array([0.0]), np.array([0.0]), np.array([-1.0]), 0)
        np.testing.assert_allclose(seam, [[150, 150, 150]])
        pixels[0] = 20
        pixels[1] = 230
        for y, expected in [(1.0, 20), (-1.0, 230)]:
            pole = sample_panorama(pixels, np.array([0.0]), np.array([y]), np.array([0.0]), 0)
            np.testing.assert_allclose(pole, [[expected] * 3])

    def test_light_pixel_tracks_bevy_axes_and_yaw(self):
        np.testing.assert_allclose(light_direction((1.5, 0.5), 4, 2, 0), [0, 0, -1], atol=1e-8)
        np.testing.assert_allclose(light_direction((1.5, 0.5), 4, 2, 90), [-1, 0, 0], atol=1e-8)
        np.testing.assert_allclose(light_direction((2.5, 0.5), 4, 2, 0), [1, 0, 0], atol=1e-8)

    def test_cli_writes_rgba_cross_and_rejects_source_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "panorama.png"
            output = Path(directory) / "cross.png"
            Image.new("RGB", (16, 8), (70, 120, 200)).save(source)
            original = source.read_bytes()
            self.assertEqual(main([str(source), str(output), "--face-size", "8"]), 0)
            with Image.open(output) as image:
                self.assertEqual(image.size, (32, 24))
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.getpixel((12, 12)), (70, 120, 200, 255))
            self.assertEqual(main([str(source), str(source), "--face-size", "8"]), 1)
            self.assertEqual(source.read_bytes(), original)

    @unittest.skipUnless(shutil.which("magick"), "HDR loading requires ImageMagick")
    def test_hdr_loading_preserves_values_above_one_before_tone_mapping(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample.hdr"
            path.write_bytes(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 2 +X 4\n" + bytes([128, 64, 32, 130]) * 8)
            pixels, hdr = load_panorama(path)
            self.assertTrue(hdr)
            np.testing.assert_allclose(pixels[0, 0], [2, 1, 0.5], atol=0.01)
            mapped = to_display_rgb(pixels[0, 0], hdr, 4, "reinhard")
            np.testing.assert_allclose(mapped, [8 / 9, 4 / 5, 2 / 3], atol=0.01)


if __name__ == "__main__":
    unittest.main()
