import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


# The defaults every map inherits; a map file carries its content and overrides.
def gameplay():
    return {
        "default_map": "hotel",
        "maps": ["hotel", "obby"],
        "actors": {name: {"immovable": name == "turret"} for name in ("scuttler", "bruiser", "zapper", "turret")},
        "player": {"movement_collider": {"diameter": 0.6, "height": 1.8}},
        "power_ups": {
            kind: {"mode": "pickup", "duration_secs": None}
            for kind in ("single_shot", "multi_shot", "portal_gun", "speed", "low_gravity")
        },
        "geometry": {"grid_cell_size": 3.4, "level_height": 4.4, "floor_thickness": 0.4, "wall_thickness": 0.3},
        "movement": {
            "gravity": 25,
            "low_gravity": 5,
            "player": {"walk_speed": 6, "run_speed": 9, "speed_power_up": 1.5, "jump_speed": 12},
        },
        "player_fall": {"safe_distance": 8, "lethal_distance": 15},
        "combat": {"health": {"player": {"max": 500}}},
        "portals": "both",
    }


def map_settings(name="hotel"):
    defaults = gameplay()
    return {
        "grounds": None,
        "random_items": None,
        "placed_items": None,
        "power_ups": defaults["power_ups"],
        "geometry": defaults["geometry"],
        "movement": defaults["movement"],
        "player_fall": defaults["player_fall"],
        "combat": defaults["combat"],
        "textures": {
            alias: {"material": "test", "portalable": alias != "portal-resistant"}
            for alias in (
                ["basement-floor", "floor-a", "floor-b", "slab", "upper-floors", "wall"]
                if name == "hotel"
                else ["basement-floor", "portal-resistant", "slab"]
            )
        },
    }


# The kind catalog a test layout carries for the named map.
def map_kinds(name="hotel"):
    if name == "hotel":
        kinds = [("treasure", "#ff3333"), ("basement", "#f0c020"), ("gravity", "#5090ff"), ("lobby", "#22cc33")]
    else:
        kinds = [("barrier_1", "#f0c020"), ("bridge_1", "#30d8ff")]
    return {"field_kinds": [{"id": kind, "color": color} for kind, color in kinds]}


def install_catalogs(test, root):
    test.root = root
    test.global_path = root / "gameplay.json"
    test.assets_path = root / "assets.json"
    test.global_path.write_text(json.dumps(gameplay()))
    test.assets_path.write_text(
        json.dumps(
            {
                "wall_lights": {"decorative": {}, "utility": {}},
                "pressure_plate": {"default_color": "#2c99bc"},
            }
        )
    )
    for name in ("hotel", "obby"):
        directory = root / name
        directory.mkdir(exist_ok=True)
        (directory / "settings.json").write_text(json.dumps(map_settings(name)))
    for module, names in [
        ("map_editor.constants", ("GAMEPLAY_PATH", "ASSETS_PATH", "MAPS_DIR")),
        ("map_editor.catalogs", ("GAMEPLAY_PATH", "ASSETS_PATH", "MAPS_DIR")),
        ("map_editor.dependencies", ("GAMEPLAY_PATH", "ASSETS_PATH")),
    ]:
        for name in names:
            value = {"GAMEPLAY_PATH": test.global_path, "ASSETS_PATH": test.assets_path, "MAPS_DIR": root}[name]
            override = patch(f"{module}.{name}", value)
            override.start()
            test.addCleanup(override.stop)


class ConfigTestCase(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        install_catalogs(self, Path(self.temp.name))
