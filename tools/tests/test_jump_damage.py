import copy
from dataclasses import replace
import unittest
from unittest.mock import patch

from editor_fixtures import WindowTestCase
from map_editor.catalogs import load_map_settings
from map_editor.jump_reach import ANTI_GRAVITY, BOTH, NORMAL, SPEED, FallSettings, JumpSettings, calculate_reach
from map_editor.normalization import empty_level, empty_map


class JumpDamageTests(unittest.TestCase):
    settings = JumpSettings(1, 1, 2, 1, 1, 2, 2, 1, 0, FallSettings(4, 12, 100))

    def reach(self, settings=None, **kwargs):
        data = empty_map(24, 24)
        data["levels"] = [empty_level(i) for i in range(18)]
        return calculate_reach(settings or self.settings, (15, 5, 5), data, running=True, margin=0, **kwargs)

    def test_safe_partial_and_fatal_drops_include_the_jump_apex(self):
        reach = self.reach()
        for level, expected in ((15, 0), (12, 0), (11, 0.125), (8, 0.5), (5, 0.875), (4, 1), (0, 1)):
            with self.subTest(level=level):
                self.assertEqual(reach[level, 6, 5][NORMAL], expected)
                self.assertEqual(reach[level, 6, 5][SPEED], expected)
        self.assertEqual(reach[16, 6, 5][NORMAL], 0)
        self.assertNotIn(NORMAL, reach[17, 6, 5])

    def test_anti_gravity_can_turn_fatal_into_damage_and_damage_into_safe(self):
        reach = self.reach()
        self.assertEqual(reach[8, 6, 5][NORMAL], 0.5)
        self.assertEqual(reach[8, 6, 5][ANTI_GRAVITY], 0.0625)
        self.assertEqual(reach[4, 6, 5][NORMAL], 1)
        self.assertEqual(reach[4, 6, 5][ANTI_GRAVITY], 0.3125)
        self.assertEqual(reach[4, 6, 5][BOTH], 0.3125)
        self.assertEqual(reach[11, 6, 5][NORMAL], 0.125)
        self.assertEqual(reach[11, 6, 5][ANTI_GRAVITY], 0)

    def test_negligible_damage_uses_maximum_health_and_includes_the_cutoff(self):
        fall = FallSettings(4, 12, 100)
        self.assertEqual(fall.damage_fraction(4.04, 2, 2), 0)
        self.assertAlmostEqual(replace(fall, max_health=1000).damage_fraction(4.04, 2, 2), 0.005)
        self.assertEqual(FallSettings(0, 8, 8).damage_fraction(1, 2, 2), 0.125)
        self.assertEqual(FallSettings(0, 8, 8).damage_fraction(0.999, 2, 2), 0)
        self.assertEqual(FallSettings(0, 8, 0.5).damage_fraction(100, 2, 2), 0)

    def test_damage_is_only_reported_for_reachable_combinations(self):
        reach = self.reach()
        self.assertNotIn(NORMAL, reach[4, 14, 5])
        self.assertEqual(reach[4, 14, 5][SPEED], 1)
        zero_gravity = self.reach(replace(self.settings, low_gravity=0))
        self.assertTrue(all(set(landings) <= {NORMAL, SPEED} for landings in zero_gravity.values()))

    def test_invalid_fall_settings_and_health_name_their_source(self):
        settings = load_map_settings("hotel")
        gameplay = {"combat": {"health": {"player": {"max": 100}}}}

        def parse(value, global_value=gameplay):
            return JumpSettings.from_settings(value, "maps/example/settings.json", gameplay=global_value, gameplay_source="gameplay.json")

        for field in ("safe_distance", "lethal_distance"):
            for value in (None, True, "8", -1, float("nan"), float("inf")):
                with self.subTest(field=field, value=value):
                    invalid = copy.deepcopy(settings)
                    invalid["player_fall"][field] = value
                    with self.assertRaisesRegex(ValueError, f"maps/example/settings.json: player_fall.{field}"):
                        parse(invalid)
            invalid = copy.deepcopy(settings)
            del invalid["player_fall"][field]
            with self.assertRaisesRegex(ValueError, f"player_fall.{field}"):
                parse(invalid)
        for safe, lethal in ((8, 8), (9, 8), (0, 0)):
            invalid = copy.deepcopy(settings)
            invalid["player_fall"] = {"safe_distance": safe, "lethal_distance": lethal}
            with self.assertRaisesRegex(ValueError, "maps/example/settings.json: player_fall"):
                parse(invalid)
        invalid = copy.deepcopy(settings)
        del invalid["player_fall"]
        with self.assertRaisesRegex(ValueError, "player_fall"):
            parse(invalid)
        settings["player_fall"]["safe_distance"] = 0
        self.assertEqual(parse(settings).fall.safe_distance, 0)
        for value in (None, True, "100", 0, -1, float("nan"), float("inf")):
            gameplay["combat"]["health"]["player"]["max"] = value
            with self.assertRaisesRegex(ValueError, "gameplay.json: combat.health.player.max"):
                parse(settings)


class JumpDamageWindowTests(WindowTestCase):
    def test_settings_reload_changes_damage_and_hover_without_losing_origin(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"] = [empty_level(i) for i in range(16)]
        window.doc.replace_with_new(data)
        overlay = window.jump_reach
        settings = load_map_settings("hotel")
        settings["geometry"]["level_height"] = 1
        settings["movement"].update(gravity=2, low_gravity=1)
        settings["movement"]["player"]["jump_speed"] = 2
        settings["player_fall"] = {"safe_distance": 4, "lethal_distance": 12}
        with patch("map_editor.jump_reach_overlay.load_map_settings", return_value=settings):
            window.reload_dependencies()
            window.select_level(15)
            overlay.select(2, 2)
            window.select_level(4)
            self.assertIn("× Normal (damage: 100%)", overlay.hover_text(3, 2))
            self.assertIn("△ Anti-gravity (damage: 31.2%)", overlay.hover_text(3, 2))
            settings["player_fall"] = {"safe_distance": 13, "lethal_distance": 20}
            window.reload_dependencies()
            self.assertEqual(overlay.origin, (15, 2, 2))
            self.assertEqual(overlay.hover_text(3, 2), "Jump Reach:\n● Normal\n● Speed\n● Anti-gravity\n● Both")
            settings["player_fall"]["lethal_distance"] = 12
            window.reload_dependencies()
            self.assertFalse(overlay.results)
            self.assertIn("player_fall", overlay.legend.text())
            self.assertEqual(overlay.origin, (15, 2, 2))

    def test_global_health_reload_applies_the_negligible_damage_cutoff(self):
        window = self.window
        overlay = window.jump_reach
        settings = load_map_settings("hotel")
        settings["movement"]["gravity"] = 2
        settings["movement"]["player"]["jump_speed"] = 2
        settings["player_fall"] = {"safe_distance": 0, "lethal_distance": 200}
        gameplay = {"combat": {"health": {"player": {"max": 100}}}}
        with (
            patch("map_editor.jump_reach_overlay.load_map_settings", return_value=settings),
            patch("map_editor.jump_reach_overlay.read_settings_json", return_value=gameplay),
        ):
            overlay.reload_settings()
            overlay.select(2, 2)
            self.assertEqual(overlay.results[0, 3, 2][NORMAL], 0)
            gameplay["combat"]["health"]["player"]["max"] = 1000
            window.reload_dependencies()
            self.assertEqual(overlay.results[0, 3, 2][NORMAL], 0.005)
            self.assertEqual(overlay.origin, (0, 2, 2))
