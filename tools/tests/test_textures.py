import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from editor_fixtures import WindowTestCase
from map_editor.catalogs import load_texture_catalog
from map_editor.constants import MODE_FLOOR
from map_editor.normalization import empty_map, normalize_map
from map_editor.validation import validate_map


class TextureCatalogTests(unittest.TestCase):
    def test_missing_or_non_boolean_permission_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "gameplay.json"
            path.write_text(json.dumps({"default_map": "host", "maps": ["host"]}))
            maps = Path(directory) / "maps"
            settings = maps / "host" / "settings.json"
            settings.parent.mkdir(parents=True)
            for entry in ({}, {"portalable": 1}, {"portalable": "false"}):
                settings.write_text(json.dumps({"textures": {"stone": entry}}))
                with patch("map_editor.catalogs.GAMEPLAY_PATH", path), patch("map_editor.catalogs.MAPS_DIR", maps):
                    with self.assertRaisesRegex(ValueError, "settings.json: textures.stone"):
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
    def test_map_change_updates_choices_validation_and_permission_labels(self):
        self.window.adopt_map("hotel")
        self.window.current_material = "upper-floors"
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [{"col": 0, "row": 0, "all": "upper-floors"}]
        data = normalize_map(data)
        self.assertFalse(self.window.validate(data))
        self.window.adopt_map("obby")
        self.assertNotIn("upper-floors", self.window.materials_catalog)
        self.assertIn(self.window.current_material, self.window.materials_catalog)
        self.assertTrue(self.window.validate(data))
        self.assertFalse(self.window.texture_catalog["portal-resistant"])
        self.window.mode_combo.setCurrentText(MODE_FLOOR)
        self.window.current_material = "portal-resistant"
        self.window.refresh_ui()
        self.assertEqual(self.window.tool_settings.material_permission.text(), "Portals incompatible")

    def test_editor_watches_wall_light_catalog_but_not_texture_images(self):
        watched = self.window.dependencies.watcher.files() + self.window.dependencies.watcher.directories()
        self.assertIn(str(self.assets_path.resolve()), watched)
        self.assertFalse(any("client/assets/textures" in path for path in watched))
