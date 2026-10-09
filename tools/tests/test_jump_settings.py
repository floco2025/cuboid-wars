import copy
import unittest

from config_fixtures import gameplay, map_settings
from map_editor.jump_settings import JumpSettings


def parse(settings):
    return JumpSettings.from_settings(settings, "maps/example/settings.json")


def at(settings, path):
    *parents, key = path.split(".")
    for parent in parents:
        settings = settings[parent]
    return settings, key


class JumpSettingsTests(unittest.TestCase):
    def test_invalid_numbers_name_the_source_and_path(self):
        settings = {**gameplay(), **map_settings()}
        for path in (
            "movement.player.move_speed",
            "movement.gravity",
            "combat.health.player.max",
            "player.movement_collider.height",
            "weapons.portals.size.width",
            "geometry.level_height",
        ):
            for value in (None, True, "1", 0, -1, float("nan"), float("inf")):
                with self.subTest(path=path, value=value):
                    invalid = copy.deepcopy(settings)
                    parent, key = at(invalid, path)
                    parent[key] = value
                    with self.assertRaisesRegex(ValueError, f"maps/example/settings.json: {path}"):
                        parse(invalid)
        for path in (
            "movement.player.air_deceleration",
            "movement.low_gravity",
            "player_fall.safe_distance",
            "weapons.portals.funnel.capture_margin",
            "weapons.portals.funnel.capture_growth",
        ):
            with self.subTest(path=path):
                zero = copy.deepcopy(settings)
                parent, key = at(zero, path)
                parent[key] = 0
                parse(zero)
                parent[key] = -1
                with self.assertRaisesRegex(ValueError, f"maps/example/settings.json: {path}"):
                    parse(zero)

    def test_missing_blocks_and_inconsistent_fall_distances_are_diagnosed(self):
        settings = {**gameplay(), **map_settings()}
        for path in ("movement.player", "player_fall", "weapons.portals.funnel", "network"):
            with self.subTest(path=path):
                missing = copy.deepcopy(settings)
                parent, key = at(missing, path)
                del parent[key]
                with self.assertRaisesRegex(ValueError, path):
                    parse(missing)
        for safe, lethal in ((8, 8), (9, 8)):
            invalid = copy.deepcopy(settings)
            invalid["player_fall"] = {"safe_distance": safe, "lethal_distance": lethal}
            with self.assertRaisesRegex(ValueError, "maps/example/settings.json: player_fall"):
                parse(invalid)
