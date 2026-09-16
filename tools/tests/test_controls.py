import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtWidgets import QDialog

from editor_fixtures import WindowTestCase, qt_app
from map_editor.control_catalogs import edit_catalog, validate_catalog
from map_editor.dialogs.control_catalogs import ControlCatalogDialog, FireworksDialog
from map_editor.dialogs.controls import FieldPropertiesDialog
from map_editor.document import MapDocument
from map_editor.editing import update_records
from map_editor.io import read_map, write_map
from map_editor.nesting import NestedMotion
from map_editor.normalization import empty_map


class ControlTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = qt_app()

    def test_switch_kind_policies_are_validated(self):
        good = [
            {"id": "lobby", "activation": "auto", "reset_on_player_death": "never"},
            {"id": "finale", "activation": "momentary", "reset_on_player_death": "all", "held": "everyone"},
        ]
        validate_catalog("switch_kinds", good)
        for bad, message in [
            ([{"id": " lobby", "activation": "auto", "reset_on_player_death": "never"}], "no surrounding spaces"),
            (good + [good[0]], "unique"),
            ([{"id": "a", "activation": "hold", "reset_on_player_death": "never"}], "activation must be one of"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "always"}], "reset_on_player_death must be"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "never", "held": "all"}], "held must be"),
            (
                [{"id": "a", "activation": "auto", "reset_on_player_death": "never", "plate_color": "red"}],
                "color must look like",
            ),
            ([{"activation": "auto", "reset_on_player_death": "never"}], "nonempty"),
            ({}, "expected a list"),
        ]:
            with self.assertRaisesRegex(ValueError, message):
                validate_catalog("switch_kinds", bad)

    def root(self):
        root = empty_map(3, 3)
        root["switch_kinds"] = [{"id": "lobby", "activation": "auto", "reset_on_player_death": "never"}]
        root["barrier_kinds"] = [{"id": "green", "color": "#22cc33"}]
        root["bridge_kinds"] = []
        root["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "kind": "green", "switch": "lobby"}]
        root["items"] = [{"col": 1, "row": 1, "level": 0, "type": "key", "kind": "green"}]
        root["pressure_plates"] = [{"col": col, "row": 2, "level": 0, "switch": "lobby"} for col in (0, 2)]
        nested = copy.deepcopy(root)
        for catalog in ("switch_kinds", "barrier_kinds", "bridge_kinds"):
            nested.pop(catalog)
        root["nested_geometry"] = {"room": nested}
        root["fireworks"] = {"switch": "lobby", "cooldown_secs": 2}
        return root

    def test_renaming_kinds_updates_placed_and_unplaced_geometry_and_keys(self):
        root = self.root()
        after = edit_catalog(root, "barrier_kinds", [{"id": "blue", "color": "#0000ff"}], {"green": "blue"})
        for geometry in (after, after["nested_geometry"]["room"]):
            self.assertEqual(geometry["items"][0]["kind"], "blue")
            self.assertEqual(geometry["levels"][0]["barriers"][0]["kind"], "blue")
            self.assertNotIn("key_kind", geometry["levels"][0]["barriers"][0])
        after = edit_catalog(
            after,
            "switch_kinds",
            [{"id": "entrance", "activation": "auto", "reset_on_player_death": "never"}],
            {"lobby": "entrance"},
        )
        self.assertEqual(after["fireworks"]["switch"], "entrance")
        self.assertTrue(all(p["switch"] == "entrance" for p in after["pressure_plates"]))
        self.assertEqual(after["nested_geometry"]["room"]["levels"][0]["barriers"][0]["switch"], "entrance")
        self.assertEqual(root["items"][0]["kind"], "green")

    def test_used_kinds_cannot_be_deleted(self):
        for catalog in ("switch_kinds", "barrier_kinds"):
            with self.assertRaisesRegex(ValueError, "still assigned"):
                edit_catalog(self.root(), catalog, [], {})

    def test_deleting_a_kind_another_is_renamed_to_is_refused_while_a_swap_passes(self):
        root = self.root()
        root["switch_kinds"].append({"id": "b", "activation": "toggle", "reset_on_player_death": "never"})
        root["pressure_plates"][1]["switch"] = "b"
        with self.assertRaisesRegex(ValueError, "'b' is still assigned"):
            edit_catalog(
                root,
                "switch_kinds",
                [{"id": "b", "activation": "auto", "reset_on_player_death": "never"}],
                {"lobby": "b"},
            )
        swapped = edit_catalog(
            root,
            "switch_kinds",
            [
                {"id": "b", "activation": "auto", "reset_on_player_death": "never"},
                {"id": "lobby", "activation": "toggle", "reset_on_player_death": "never"},
            ],
            {"lobby": "b", "b": "lobby"},
        )
        self.assertEqual([plate["switch"] for plate in swapped["pressure_plates"]], ["b", "lobby"])
        self.assertEqual(swapped["fireworks"]["switch"], "b")
        self.assertEqual(swapped["nested_geometry"]["room"]["pressure_plates"][1]["switch"], "b")

    def test_control_dialogs_report_only_changes_and_clear_both_keys_for_none(self):
        entry = {"c0": 0, "r0": 0, "c1": 1, "r1": 0, "kind": "green", "switch": "lobby", "switch_inverted": True}
        dialog = FieldPropertiesDialog(None, "Edit Barrier", ["green"], ["lobby", "door"], [entry])
        self.assertEqual(dialog.values(), {"kind": "green"})
        dialog.control.kind.setCurrentIndex(dialog.control.kind.findData("door"))
        self.assertEqual(dialog.values(), {"kind": "green", "switch": "door"})
        dialog.control.kind.setCurrentIndex(dialog.control.kind.findData(""))
        self.assertEqual(dialog.values(), {"kind": "green", "switch": None})
        dialog.deleteLater()
        data = empty_map(2, 2)
        data["levels"][0]["barriers"] = [entry]
        cleared = update_records(data, "barriers", lambda b: True, {"switch": None}, 0)
        self.assertEqual(cleared["levels"][0]["barriers"], [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "kind": "green"}])
        bare = FieldPropertiesDialog(None, "Edit Barrier", ["green"], ["lobby"], [cleared["levels"][0]["barriers"][0]])
        self.assertEqual(bare.values(), {"kind": "green"})
        bare.control.kind.setCurrentIndex(bare.control.kind.findData("lobby"))
        bare.control.response.setCurrentIndex(bare.control.response.findData("Off"))
        self.assertEqual(bare.values(), {"kind": "green", "switch": "lobby", "switch_inverted": True})
        bare.deleteLater()
        fireworks = FireworksDialog(None, ["lobby"], {"switch": "lobby", "cooldown_secs": 3})
        self.assertEqual(fireworks.value(), {"switch": "lobby", "cooldown_secs": 3.0})
        fireworks.deleteLater()

    def test_catalog_dialogs_edit_colours_through_swatches(self):
        dialog = ControlCatalogDialog(None, "Barrier Kinds", "barrier_kinds", [{"id": "green", "color": "#22cc33"}])
        swatch = dialog.table.cellWidget(0, 1)
        self.assertEqual((swatch.color, swatch.button.text()), ("#22cc33", "#22cc33"))
        self.assertFalse(swatch.clear_button.isVisibleTo(dialog))
        swatch.set_color("#123456")
        dialog.add_entry({"id": "fresh", "color": dialog.fresh_color()})
        entries, renames = dialog.values()
        self.assertEqual(entries[0], {"id": "green", "color": "#123456"})
        self.assertRegex(entries[1]["color"], r"#[0-9a-f]{6}")
        self.assertNotEqual(entries[1]["color"], entries[0]["color"])
        self.assertEqual(renames, {})
        dialog.deleteLater()
        plates = ControlCatalogDialog(
            None,
            "Pressure Plate Kinds",
            "switch_kinds",
            [{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}],
        )
        override = plates.table.cellWidget(0, 4)
        self.assertEqual((override.color, override.button.text()), (None, "Inherit"))
        override.set_color("#9b5de5")
        self.assertEqual(plates.values()[0][0]["plate_color"], "#9b5de5")
        self.assertTrue(override.clear_button.isVisibleTo(plates))
        override.clear_button.click()
        self.assertNotIn("plate_color", plates.values()[0][0])
        plates.deleteLater()

    def test_catalog_dialog_accepts_a_name_still_being_edited(self):
        dialog = ControlCatalogDialog(
            None,
            "Pressure Plate Kinds",
            "switch_kinds",
            [{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}],
        )
        item = dialog.table.item(0, 0)
        dialog.table.setCurrentItem(item)
        dialog.table.editItem(item)
        dialog.table.cellWidget(0, 0).setText("gate")
        dialog.accept()
        self.assertEqual(dialog.result(), QDialog.DialogCode.Accepted)
        entries, renames = dialog.values()
        self.assertEqual(entries[0]["id"], "gate")
        self.assertEqual(renames, {"door": "gate"})
        dialog.deleteLater()

    def map_folder(self, directory, name, settings, layout):
        path = Path(directory) / name / "layout.json"
        path.parent.mkdir()
        path.with_name("settings.json").write_text(json.dumps(settings, indent=2) + "\n")
        write_map(path, layout)
        return path

    def test_save_as_carries_the_catalogs_in_the_layout_and_leaves_settings_alone(self):
        with tempfile.TemporaryDirectory() as directory:
            layout = empty_map(3, 3)
            layout["barrier_kinds"] = [{"id": "barrier_1", "color": "#f0c020"}]
            layout["bridge_kinds"] = [{"id": "bridge_1", "color": "#30d8ff"}]
            layout["levels"][0]["light_bridges"] = [{"col": 0, "row": 0, "kind": "bridge_1"}]
            obby = self.map_folder(directory, "obby", {"portals": "single"}, layout)
            hotel = self.map_folder(directory, "hotel", {"portals": "both"}, empty_map(3, 3))
            settings = {path: path.with_name("settings.json").read_text() for path in (obby, hotel)}
            doc = MapDocument(obby)
            recolored = [{"id": "barrier_1", "color": "#123456"}]
            doc.apply_root_change("Recolor", edit_catalog(doc.root_data, "barrier_kinds", recolored, {}), None)
            doc.write(hotel)
            written = read_map(hotel)
            self.assertEqual(written["barrier_kinds"], recolored)
            self.assertEqual(written["bridge_kinds"], layout["bridge_kinds"])
            self.assertEqual([bridge["kind"] for bridge in written["levels"][0]["light_bridges"]], ["bridge_1"])
            self.assertEqual(doc.path, hotel)
            self.assertFalse(doc.dirty)
            for path, text in settings.items():
                self.assertEqual(path.with_name("settings.json").read_text(), text)

    def test_bulk_controls_preserve_mixed_appearance_and_choose_one_plate_kind(self):
        dialog = FieldPropertiesDialog(
            None,
            "Fields",
            ["red", "blue"],
            ["a", "b"],
            [{"kind": "red", "switch": "a"}, {"kind": "blue", "switch": "b"}],
        )
        self.assertNotIn("kind", dialog.values())
        self.assertNotIn("switch", dialog.values())
        dialog.control.kind.setCurrentIndex(dialog.control.kind.findData("b"))
        dialog.control.response.setCurrentIndex(dialog.control.response.findData("Off"))
        self.assertEqual(dialog.values(), {"switch": "b", "switch_inverted": True})
        dialog.deleteLater()

    def test_catalog_changes_share_undo_recovery_and_save_without_touching_settings(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            settings_path = path.with_name("settings.json")
            settings_path.write_text('{"movement": {"gravity": 12}}')
            write_map(path, self.root())
            doc = MapDocument(path)
            before = copy.deepcopy(doc.root_data)
            after = edit_catalog(before, "barrier_kinds", [{"id": "blue", "color": "#0000ff"}], {"green": "blue"})
            doc.apply_root_change("Rename barrier", after, None)
            doc.undo_stack.undo()
            self.assertEqual(doc.root_data, before)
            doc.undo_stack.redo()
            doc.write_autosave()
            self.assertEqual(read_map(doc.autosave_path()), after)
            doc.write()
            self.assertEqual(read_map(path)["barrier_kinds"], [{"id": "blue", "color": "#0000ff"}])
            self.assertEqual(settings_path.read_text(), '{"movement": {"gravity": 12}}')
            self.assertFalse(doc.dirty)


class ControlWindowTests(WindowTestCase):
    def test_catalog_renames_and_deletions_follow_into_the_toolbar_defaults(self):
        window = self.window
        window.doc.root_data["switch_kinds"] = [
            {"id": name, "activation": "toggle", "reset_on_player_death": "never"} for name in ("lobby", "door")
        ]
        window.switch_ids = ["lobby", "door"]
        window.recent_barrier_controls = {"switch": "lobby", "switch_inverted": True}
        window.recent_bridge_controls = {"switch": "door", "switch_inverted": False}
        window.recent_actor_spawn_switch = "lobby"
        window.recent_actor_spawn_inverted = True
        window.recent_pressure_plate_switch = "door"
        window.recent_nested_map = NestedMotion(
            "tile", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), "lobby", True
        )
        renamed = [
            {"id": "entrance", "activation": "toggle", "reset_on_player_death": "never"},
            {"id": "door", "activation": "toggle", "reset_on_player_death": "never"},
        ]
        with patch(
            "map_editor.control_actions.ControlCatalogDialog.prompt", return_value=(renamed, {"lobby": "entrance"})
        ):
            window.edit_control_catalog("switch_kinds", "Pressure Plate Kinds")
        self.assertEqual(window.recent_barrier_controls, {"switch": "entrance", "switch_inverted": True})
        self.assertEqual(window.recent_actor_spawn_switch, "entrance")
        self.assertEqual(window.recent_nested_map.switch, "entrance")
        self.assertEqual(window.recent_pressure_plate_switch, "door")
        deleted = [{"id": "entrance", "activation": "toggle", "reset_on_player_death": "never"}]
        with patch("map_editor.control_actions.ControlCatalogDialog.prompt", return_value=(deleted, {})):
            window.edit_control_catalog("switch_kinds", "Pressure Plate Kinds")
        self.assertEqual(window.recent_bridge_controls, {})
        self.assertEqual(window.recent_pressure_plate_switch, "entrance")
        self.assertEqual(window.switches, ["entrance"])
        with patch("map_editor.control_actions.ControlCatalogDialog.prompt", return_value=([], {})):
            window.edit_control_catalog("switch_kinds", "Pressure Plate Kinds")
        self.assertEqual(window.recent_barrier_controls, {})
        self.assertEqual(window.recent_actor_spawn_switch, "")
        self.assertFalse(window.recent_actor_spawn_inverted)
        self.assertIsNone(window.recent_pressure_plate_switch)
        self.assertIsNone(window.recent_nested_map.switch)
        self.assertFalse(window.recent_nested_map.switch_inverted)
