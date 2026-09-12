import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from analyze_audio import measure


class AudioAnalysisTests(unittest.TestCase):
    def test_short_silent_and_stereo_files(self):
        silent = measure([0.0] * 100, 2)
        self.assertIsNone(silent["peak_dbfs"])
        self.assertIsNone(silent["rms_dbfs"])
        self.assertEqual(silent["suggested_gain_db"], 0)
        mono = [0.25] * 100
        stereo = [v for sample in mono for v in (sample, sample)]
        self.assertEqual(measure(mono, 1), measure(stereo, 2))

    def test_balancing_ignores_silent_tails_and_respects_peaks(self):
        samples = [0.2 * math.sin(i * 0.1) for i in range(4800)]
        measured = measure(samples, 1)
        padded = measure(samples + [0.0] * 48000, 1)
        self.assertAlmostEqual(measured["suggested_gain_db"], padded["suggested_gain_db"])
        self.assertAlmostEqual(measured["strongest_50ms_rms_dbfs"] + measured["suggested_gain_db"], -24)
        peak = measure([1.0] + [0.0] * 4800, 1)
        self.assertLessEqual(peak["peak_dbfs"] + peak["suggested_gain_db"], -6)
        quiet = measure([0.00001] * 100, 1)
        self.assertGreater(quiet["suggested_gain_db"], 0)
        self.assertAlmostEqual(quiet["strongest_50ms_rms_dbfs"] + quiet["suggested_gain_db"], -24)

    def test_invalid_audio_is_rejected(self):
        for samples, channels in [([], 1), ([0.0], 2), ([float("nan")], 1), ([float("inf")], 1)]:
            with self.assertRaises(ValueError):
                measure(samples, channels)


if __name__ == "__main__":
    unittest.main()
