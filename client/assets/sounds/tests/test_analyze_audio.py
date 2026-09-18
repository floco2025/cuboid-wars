import copy
import io
import json
import math
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from analyze_audio import analysis_differences, main, measure


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


class AnalysisFreshnessTests(unittest.TestCase):
    def setUp(self):
        self.current = {
            "version": 1,
            "analysis_sample_rate": 48000,
            "target_50ms_rms_dbfs": -24.0,
            "peak_ceiling_dbfs": -6.0,
            "sounds": {
                "sounds/test.wav": {
                    "sha256": "a" * 64,
                    "sample_rate": 44100,
                    "channels": 2,
                    "duration_secs": 1.25,
                    "peak_dbfs": -12.0,
                    "rms_dbfs": -32.0,
                    "strongest_50ms_rms_dbfs": -24.0,
                    "suggested_gain_db": 0.0,
                }
            },
        }

    def test_measurement_noise_passes(self):
        saved = copy.deepcopy(self.current)
        sound = saved["sounds"]["sounds/test.wav"]
        sound["duration_secs"] += 0.0000005
        for field in ("peak_dbfs", "rms_dbfs", "strongest_50ms_rms_dbfs", "suggested_gain_db"):
            sound[field] += 0.000002
        self.assertEqual(list(analysis_differences(saved, self.current)), [])

    def test_meaningful_measurement_changes_report_file_field_and_values(self):
        for field, delta in (
            ("duration_secs", 1 / 48000),
            ("peak_dbfs", 0.001),
            ("rms_dbfs", 0.001),
            ("strongest_50ms_rms_dbfs", 0.001),
            ("suggested_gain_db", 0.001),
        ):
            with self.subTest(field=field):
                saved = copy.deepcopy(self.current)
                saved_value = saved["sounds"]["sounds/test.wav"][field] + delta
                saved["sounds"]["sounds/test.wav"][field] = saved_value
                current_value = self.current["sounds"]["sounds/test.wav"][field]
                self.assertEqual(
                    list(analysis_differences(saved, self.current)),
                    [f"sounds.sounds/test.wav.{field}: saved {saved_value!r}, current {current_value!r}"],
                )

    def test_settings_hashes_and_discrete_metadata_are_exact(self):
        for path, replacement in (
            (("version",), 2),
            (("analysis_sample_rate",), 48001),
            (("target_50ms_rms_dbfs",), -24.000001),
            (("peak_ceiling_dbfs",), -6.000001),
            (("sounds", "sounds/test.wav", "sha256"), "b" * 64),
            (("sounds", "sounds/test.wav", "sample_rate"), 48000),
            (("sounds", "sounds/test.wav", "channels"), 1),
            (("sounds", "sounds/test.wav", "channels"), 2.0),
            (("version",), True),
        ):
            with self.subTest(path=path, replacement=replacement):
                saved = copy.deepcopy(self.current)
                parent = saved
                for key in path[:-1]:
                    parent = parent[key]
                parent[path[-1]] = replacement
                differences = list(analysis_differences(saved, self.current))
                self.assertEqual(len(differences), 1)
                self.assertIn(".".join(path), differences[0])

    def test_file_membership_and_schema_changes_are_reported(self):
        saved = copy.deepcopy(self.current)
        saved["sounds"]["sounds/removed.wav"] = saved["sounds"].pop("sounds/test.wav")
        self.assertEqual(
            list(analysis_differences(saved, self.current)),
            [
                "sounds.sounds/removed.wav: no longer present",
                "sounds.sounds/test.wav: missing from saved analysis",
            ],
        )
        saved = copy.deepcopy(self.current)
        del saved["version"]
        saved["extra_setting"] = None
        del saved["sounds"]["sounds/test.wav"]["peak_dbfs"]
        saved["sounds"]["sounds/test.wav"]["extra_measurement"] = 0.0
        self.assertEqual(
            list(analysis_differences(saved, self.current)),
            [
                "extra_setting: no longer present",
                "sounds.sounds/test.wav.extra_measurement: no longer present",
                "sounds.sounds/test.wav.peak_dbfs: missing from saved analysis",
                "version: missing from saved analysis",
            ],
        )

    def test_silence_nulls_remain_distinct_from_zero_and_missing_fields(self):
        current = copy.deepcopy(self.current)
        current["sounds"]["sounds/test.wav"]["peak_dbfs"] = None
        self.assertEqual(list(analysis_differences(copy.deepcopy(current), current)), [])
        for value in (0.0, -12.0):
            with self.subTest(value=value):
                saved = copy.deepcopy(current)
                saved["sounds"]["sounds/test.wav"]["peak_dbfs"] = value
                self.assertEqual(len(list(analysis_differences(saved, current))), 1)
                self.assertEqual(len(list(analysis_differences(current, saved))), 1)
        saved = copy.deepcopy(current)
        del saved["sounds"]["sounds/test.wav"]["peak_dbfs"]
        self.assertEqual(
            list(analysis_differences(saved, current)),
            ["sounds.sounds/test.wav.peak_dbfs: missing from saved analysis"],
        )

    def test_invalid_numeric_measurements_do_not_pass(self):
        for value in (True, False, "0.0", float("nan"), float("inf"), float("-inf")):
            with self.subTest(value=value):
                saved = copy.deepcopy(self.current)
                saved["sounds"]["sounds/test.wav"]["suggested_gain_db"] = value
                self.assertEqual(len(list(analysis_differences(saved, self.current))), 1)

    def test_check_accepts_noise_without_rewriting_and_reports_a_stale_gain(self):
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            destination = assets / "sounds" / "analysis.json"
            destination.parent.mkdir()
            saved = copy.deepcopy(self.current)
            saved["sounds"]["sounds/test.wav"]["suggested_gain_db"] = 0.000002
            original = json.dumps(saved)
            destination.write_text(original)
            with (
                patch("analyze_audio.ASSETS", assets),
                patch("analyze_audio.catalog", return_value=self.current),
                patch.object(sys, "argv", ["analyze_audio.py", "--check"]),
                redirect_stdout(io.StringIO()) as output,
            ):
                main()
                self.assertEqual(destination.read_text(), original)
                self.assertEqual(output.getvalue(), "Checked 1 audio files\n")
                saved["sounds"]["sounds/test.wav"]["suggested_gain_db"] = 0.5
                destination.write_text(json.dumps(saved))
                with self.assertRaises(SystemExit) as failure:
                    main()
                self.assertIn("sounds/test.wav.suggested_gain_db: saved 0.5, current 0.0", str(failure.exception))
                self.assertIn("Run client/assets/sounds/analyze_audio.py", str(failure.exception))
                destination.unlink()
                with self.assertRaisesRegex(SystemExit, "sounds/analysis.json: missing saved analysis"):
                    main()


if __name__ == "__main__":
    unittest.main()
