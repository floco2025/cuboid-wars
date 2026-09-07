import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from editor_fixtures import WindowTestCase
from map_editor.io import empty_map
from map_editor.normalization import normalize_map
from map_editor.textures import load_texture_catalog, texture_hosts
from map_editor.validation import validate_map


class TextureCatalogTests(unittest.TestCase):
    def test_catalogs_are_per_host_and_permissions_are_explicit(self):
        self.assertIn("upper-floors", load_texture_catalog("hotel"))
        self.assertNotIn("upper-floors", load_texture_catalog("obby"))
        self.assertFalse(load_texture_catalog("obby")["portal-resistant"])

    def test_nested_map_selects_a_host_that_contains_it(self):
        from map_editor.constants import GAMEPLAY_PATH, MAPS_DIR
        gameplay = json.loads(GAMEPLAY_PATH.read_text())
        for host in gameplay["maps"]:
            data = json.loads((MAPS_DIR / f"{host}.json").read_text())["map"]
            for child in data.get("nested_maps", []):
                preferred, choices = texture_hosts(child["map"])
                self.assertIn(preferred, choices)
                self.assertTrue(load_texture_catalog(preferred))

    def test_missing_or_non_boolean_permission_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "gameplay.json"
            for entry in ({}, {"portalable": 1}, {"portalable": "false"}):
                path.write_text(json.dumps({"maps": {"host": {"textures": {"stone": entry}}}}))
                with patch("map_editor.textures.GAMEPLAY_PATH", path):
                    with self.assertRaisesRegex(ValueError, "maps.host.textures.stone"):
                        load_texture_catalog("host")

    def test_empty_catalog_rejects_authored_faces(self):
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [{"col": 0, "row": 0, "all": "stone"}]
        self.assertTrue(validate_map(normalize_map(data), [], [], material_aliases=[]))

    def test_missing_face_material_stays_invalid(self):
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [{"col": 0, "row": 0}]
        data = normalize_map(data)
        self.assertEqual(data["levels"][0]["floors"][0]["top"], "")
        self.assertTrue(validate_map(data, [], [], material_aliases=["stone"]))


class TextureHostWindowTests(WindowTestCase):
    def test_host_change_updates_choices_validation_and_permission_labels(self):
        self.window.select_texture_host("hotel")
        self.window.current_material = "upper-floors"
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [{"col": 0, "row": 0, "all": "upper-floors"}]
        data = normalize_map(data)
        self.assertFalse(self.window.validate(data))
        self.window.select_texture_host("obby")
        self.assertNotIn("upper-floors", self.window.materials_catalog)
        self.assertIn(self.window.current_material, self.window.materials_catalog)
        self.assertTrue(self.window.validate(data))
        self.assertFalse(self.window.texture_catalog["portal-resistant"])
        from map_editor.constants import MODE_FLOOR
        self.window.mode_combo.setCurrentText(MODE_FLOOR)
        self.window.current_material = "portal-resistant"
        self.window.refresh_ui()
        self.assertEqual(self.window.tool_settings.material_permission.text(), "Portals incompatible")

    def test_editor_watches_no_client_asset_file_or_directory(self):
        watched = self.window.dependencies.watcher.files() + self.window.dependencies.watcher.directories()
        self.assertFalse(any("config/client" in path for path in watched))
