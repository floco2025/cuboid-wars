import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtWidgets import QDialog

from editor_fixtures import WindowTestCase, qt_app
from map_editor.constants import HIT_BARRIER, HIT_LIGHT_BRIDGE
from map_editor.control_catalogs import catalog_usage, edit_catalog, validate_catalog
from map_editor.dialogs.control_catalogs import ControlCatalogDialog, FireworksDialog
from map_editor.document import MapDocument
from map_editor.hover import element_hover_text
from map_editor.io import read_map, write_map
from map_editor.nesting import NestedMotion
from map_editor.normalization import empty_map


class ControlTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = qt_app()

    def test_switch_policies_are_validated(self):
        good = [
            {"id": "lobby", "activation": "auto", "reset_on_player_death": "never"},
            {"id": "finale", "activation": "momentary", "reset_on_player_death": "all", "held": "everyone"},
        ]
        validate_catalog("switches", good)
        for bad, message in [
            ([{"id": " lobby", "activation": "auto", "reset_on_player_death": "never"}], "no surrounding spaces"),
            (good + [good[0]], "unique"),
            ([{"id": "a", "activation": "hold", "reset_on_player_death": "never"}], "activation must be one of"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "always"}], "reset_on_player_death must be"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "never", "held": "all"}], "held must be"),
            (
                [{"id": "a", "activation": "auto", "reset_on_player_death": "never", "color": "red"}],
                "color must look like",
            ),
            ([{"activation": "auto", "reset_on_player_death": "never"}], "nonempty"),
            ({}, "expected a list"),
        ]:
            with self.assertRaisesRegex(ValueError, message):
                validate_catalog("switches", bad)

    def root(self):
        root = empty_map(3, 3)
        root["switches"] = [{"id": "lobby", "activation": "auto", "reset_on_player_death": "never"}]
        root["fields"] = [{"id": "green", "color": "#22cc33", "switch": "lobby"}]
        root["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "field": "green"}]
        root["levels"][0]["light_bridges"] = [{"col": 2, "row": 0, "field": "green"}]
        root["items"] = [{"col": 1, "row": 1, "level": 0, "type": "key", "field": "green"}]
        root["pressure_plates"] = [{"col": col, "row": 2, "level": 0, "switch": "lobby"} for col in (0, 2)]
        nested = copy.deepcopy(root)
        for catalog in ("switches", "fields"):
            nested.pop(catalog)
        root["nested_geometry"] = {"room": nested}
        root["fireworks"] = {"switch": "lobby", "cooldown_secs": 2}
        return root

    def test_renaming_a_field_updates_barriers_bridges_and_keys_in_placed_and_unplaced_geometry(self):
        root = self.root()
        blue = {"id": "blue", "color": "#0000ff", "switch": "lobby"}
        after = edit_catalog(root, "fields", [blue], {"green": "blue"})
        for geometry in (after, after["nested_geometry"]["room"]):
            self.assertEqual(geometry["items"][0]["field"], "blue")
            self.assertEqual(geometry["levels"][0]["barriers"][0]["field"], "blue")
            self.assertEqual(geometry["levels"][0]["light_bridges"][0]["field"], "blue")
        self.assertEqual(root["items"][0]["field"], "green")

    def test_renaming_a_switch_updates_the_fields_plates_and_fireworks_naming_it(self):
        root = self.root()
        after = edit_catalog(
            root,
            "switches",
            [{"id": "entrance", "activation": "auto", "reset_on_player_death": "never"}],
            {"lobby": "entrance"},
        )
        self.assertEqual(after["fireworks"]["switch"], "entrance")
        self.assertTrue(all(p["switch"] == "entrance" for p in after["pressure_plates"]))
        self.assertTrue(all(p["switch"] == "entrance" for p in after["nested_geometry"]["room"]["pressure_plates"]))
        self.assertEqual(after["fields"], [{"id": "green", "color": "#22cc33", "switch": "entrance"}])
        self.assertEqual(root["fields"][0]["switch"], "lobby")

    def test_used_catalog_entries_cannot_be_deleted(self):
        for catalog in ("switches", "fields"):
            with self.assertRaisesRegex(ValueError, "still assigned"):
                edit_catalog(self.root(), catalog, [], {})

    def test_a_switch_named_only_by_a_field_cannot_be_deleted(self):
        root = empty_map(3, 3)
        root["switches"] = [{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}]
        root["fields"] = [{"id": "green", "color": "#22cc33", "switch": "door"}]
        with self.assertRaisesRegex(ValueError, "'door' is still assigned"):
            edit_catalog(root, "switches", [], {})
        self.assertEqual(catalog_usage(root, "switches"), {"door": "1 field"})

    def test_a_field_used_by_any_barrier_bridge_or_key_cannot_be_deleted(self):
        users = [
            ("barriers", {"c0": 0, "r0": 0, "c1": 1, "r1": 0, "field": "green"}),
            ("light_bridges", {"col": 2, "row": 0, "field": "green"}),
            ("items", {"col": 1, "row": 1, "level": 0, "type": "key", "field": "green"}),
        ]
        for name, entry in users:
            root = empty_map(3, 3)
            root["fields"] = [{"id": "green", "color": "#22cc33"}]
            (root if name == "items" else root["levels"][0])[name] = [entry]
            with self.subTest(name), self.assertRaisesRegex(ValueError, "still assigned"):
                edit_catalog(root, "fields", [], {})

    def test_deleting_an_entry_another_is_renamed_to_is_refused_while_a_swap_passes(self):
        root = self.root()
        root["switches"].append({"id": "b", "activation": "toggle", "reset_on_player_death": "never"})
        root["pressure_plates"][1]["switch"] = "b"
        with self.assertRaisesRegex(ValueError, "'b' is still assigned"):
            edit_catalog(
                root,
                "switches",
                [{"id": "b", "activation": "auto", "reset_on_player_death": "never"}],
                {"lobby": "b"},
            )
        swapped = edit_catalog(
            root,
            "switches",
            [
                {"id": "b", "activation": "auto", "reset_on_player_death": "never"},
                {"id": "lobby", "activation": "toggle", "reset_on_player_death": "never"},
            ],
            {"lobby": "b", "b": "lobby"},
        )
        self.assertEqual([plate["switch"] for plate in swapped["pressure_plates"]], ["b", "lobby"])
        self.assertEqual(swapped["fireworks"]["switch"], "b")
        self.assertEqual(swapped["nested_geometry"]["room"]["pressure_plates"][1]["switch"], "b")

    def test_hover_shows_a_pieces_field_with_that_fields_switch_and_initial_state(self):
        data = empty_map(3, 3)
        data["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "field": "gate"}]
        data["levels"][0]["light_bridges"] = [{"col": 2, "row": 0, "field": "walk"}]
        fields = {
            "gate": {"id": "gate", "color": "#ff0000", "switch": "lobby"},
            "walk": {"id": "walk", "color": "#00ff00", "initially_on": False},
        }
        barrier, bridge = (HIT_BARRIER, (0, 0, 1, 0)), (HIT_LIGHT_BRIDGE, (2, 0))
        self.assertEqual(
            element_hover_text(data, 0, barrier, fields), "Barrier: gate\nSwitch: lobby · Initial state: On"
        )
        self.assertEqual(element_hover_text(data, 0, bridge, fields), "Light bridge: walk\nInitial state: Off")
        self.assertEqual(element_hover_text(data, 0, barrier, {"gate": {"id": "gate"}}), "Barrier: gate")

    def test_the_fireworks_dialog_returns_its_switch_and_cooldown(self):
        fireworks = FireworksDialog(None, ["lobby"], {"switch": "lobby", "cooldown_secs": 3})
        self.assertEqual(fireworks.value(), {"switch": "lobby", "cooldown_secs": 3.0})
        fireworks.deleteLater()

    def test_the_fields_dialog_round_trips_switch_and_initial_state_and_omits_defaults(self):
        entries = [
            {"id": "red", "color": "#ff0000"},
            {"id": "door", "color": "#00ff00", "switch": "lobby", "initially_on": False},
        ]
        dialog = ControlCatalogDialog(None, "Fields", "fields", entries, switches=["lobby", "gate"])
        self.assertEqual(
            [dialog.table.horizontalHeaderItem(column).text() for column in range(dialog.table.columnCount())],
            ["Name", "Color", "Switch", "Initial state", "Used by"],
        )
        self.assertEqual(dialog.values(), (entries, {}))
        switch, state = dialog.table.cellWidget(0, 2), dialog.table.cellWidget(0, 3)
        self.assertEqual([switch.itemText(index) for index in range(switch.count())], ["(none)", "lobby", "gate"])
        switch.setCurrentIndex(switch.findData("gate"))
        state.setCurrentIndex(state.findData("Off"))
        switch, state = dialog.table.cellWidget(1, 2), dialog.table.cellWidget(1, 3)
        switch.setCurrentIndex(switch.findData(""))
        state.setCurrentIndex(state.findData("On"))
        self.assertEqual(
            dialog.values()[0],
            [
                {"id": "red", "color": "#ff0000", "switch": "gate", "initially_on": False},
                {"id": "door", "color": "#00ff00"},
            ],
        )
        dialog.deleteLater()

    def test_catalog_dialogs_edit_colours_through_swatches(self):
        dialog = ControlCatalogDialog(None, "Fields", "fields", [{"id": "green", "color": "#22cc33"}])
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
        switches = ControlCatalogDialog(
            None,
            "Switches",
            "switches",
            [{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}],
        )
        override = switches.table.cellWidget(0, 4)
        self.assertEqual((override.color, override.button.text()), (None, "Inherit"))
        override.set_color("#9b5de5")
        self.assertEqual(switches.values()[0][0]["color"], "#9b5de5")
        self.assertTrue(override.clear_button.isVisibleTo(switches))
        override.clear_button.click()
        self.assertNotIn("color", switches.values()[0][0])
        switches.deleteLater()

    def test_catalog_dialogs_show_what_uses_each_entry_across_nested_geometry(self):
        root = empty_map(4, 4)
        root["switches"] = [{"id": name, "activation": "toggle", "reset_on_player_death": "never"} for name in "ab"]
        root["fields"] = [{"id": "green", "color": "#22cc33", "switch": "a"}]
        root["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "switch": "a"}]
        root["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "field": "green"}]
        root["items"] = [{"level": 0, "col": 2, "row": 2, "type": "key", "field": "green"}]
        room = empty_map(2, 2)
        room["pressure_plates"] = [{"level": 0, "col": 0, "row": 0, "switch": "a"}]
        room["levels"][0]["barriers"] = [{"c0": 0, "r0": 1, "c1": 1, "r1": 1, "field": "green"}]
        room["levels"][0]["light_bridges"] = [{"col": 1, "row": 1, "field": "green"}]
        root["nested_geometry"] = {"room": room}
        root["fireworks"] = {"switch": "a", "cooldown_secs": 0}
        self.assertEqual(catalog_usage(root, "switches"), {"a": "2 plates, 1 field, 1 fireworks"})
        self.assertEqual(catalog_usage(root, "fields"), {"green": "2 barriers, 1 bridge cell, 1 key"})
        dialog = ControlCatalogDialog(None, "Switches", "switches", root["switches"], catalog_usage(root, "switches"))
        column = dialog.table.columnCount() - 1
        dialog.table.item(0, 0).setText("renamed")
        dialog.add_entry({"id": "fresh"})
        self.assertEqual(
            [dialog.table.item(row, column).text() for row in range(3)],
            ["2 plates, 1 field, 1 fireworks", "Unused", "Unused"],
        )
        self.assertEqual([entry["id"] for entry in dialog.values()[0]], ["renamed", "b", "fresh"])
        dialog.deleteLater()

    def test_catalog_dialog_accepts_a_name_still_being_edited(self):
        dialog = ControlCatalogDialog(
            None,
            "Switches",
            "switches",
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
            layout["fields"] = [{"id": "barrier_1", "color": "#f0c020"}, {"id": "bridge_1", "color": "#30d8ff"}]
            layout["levels"][0]["light_bridges"] = [{"col": 0, "row": 0, "field": "bridge_1"}]
            obby = self.map_folder(directory, "obby", {"portals": "single"}, layout)
            hotel = self.map_folder(directory, "hotel", {"portals": "both"}, empty_map(3, 3))
            settings = {path: path.with_name("settings.json").read_text() for path in (obby, hotel)}
            doc = MapDocument(obby)
            recolored = [{"id": "barrier_1", "color": "#123456"}, layout["fields"][1]]
            doc.apply_root_change("Recolor", edit_catalog(doc.root_data, "fields", recolored, {}), None)
            doc.write(hotel)
            written = read_map(hotel)
            self.assertEqual(written["fields"], recolored)
            self.assertEqual([bridge["field"] for bridge in written["levels"][0]["light_bridges"]], ["bridge_1"])
            self.assertEqual(doc.path, hotel)
            self.assertFalse(doc.dirty)
            for path, text in settings.items():
                self.assertEqual(path.with_name("settings.json").read_text(), text)

    def test_catalog_changes_share_undo_recovery_and_save_without_touching_settings(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            settings_path = path.with_name("settings.json")
            settings_path.write_text('{"movement": {"gravity": 12}}')
            write_map(path, self.root())
            doc = MapDocument(path)
            before = copy.deepcopy(doc.root_data)
            blue = {"id": "blue", "color": "#0000ff", "switch": "lobby", "initially_on": False}
            after = edit_catalog(before, "fields", [blue], {"green": "blue"})
            doc.apply_root_change("Rename field", after, None)
            doc.undo_stack.undo()
            self.assertEqual(doc.root_data, before)
            doc.undo_stack.redo()
            doc.write_autosave()
            self.assertEqual(read_map(doc.autosave_path()), after)
            doc.write()
            self.assertEqual(read_map(path)["fields"], [blue])
            self.assertEqual(settings_path.read_text(), '{"movement": {"gravity": 12}}')
            self.assertFalse(doc.dirty)


class ControlWindowTests(WindowTestCase):
    def test_catalog_renames_and_deletions_follow_into_the_toolbar_defaults(self):
        window = self.window
        window.doc.root_data["switches"] = [
            {"id": name, "activation": "toggle", "reset_on_player_death": "never"} for name in ("lobby", "door")
        ]
        window.switch_ids = ["lobby", "door"]
        window.recent_actor_spawn_switch = "lobby"
        window.recent_actor_spawn_initially_on = False
        window.recent_pressure_plate_switch = "door"
        window.recent_nested_map = NestedMotion(
            "tile", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), "lobby", False, "follow_switch"
        )
        renamed = [
            {"id": "entrance", "activation": "toggle", "reset_on_player_death": "never"},
            {"id": "door", "activation": "toggle", "reset_on_player_death": "never"},
        ]
        with patch(
            "map_editor.control_actions.ControlCatalogDialog.prompt", return_value=(renamed, {"lobby": "entrance"})
        ):
            window.edit_control_catalog("switches", "Switches")
        self.assertEqual(window.recent_actor_spawn_switch, "entrance")
        self.assertEqual(window.recent_nested_map.switch, "entrance")
        self.assertEqual(window.recent_nested_map.motion, "follow_switch")
        self.assertEqual(window.recent_pressure_plate_switch, "door")
        deleted = [{"id": "entrance", "activation": "toggle", "reset_on_player_death": "never"}]
        with patch("map_editor.control_actions.ControlCatalogDialog.prompt", return_value=(deleted, {})):
            window.edit_control_catalog("switches", "Switches")
        self.assertEqual(window.recent_pressure_plate_switch, "entrance")
        self.assertEqual(window.switches, ["entrance"])
        with patch("map_editor.control_actions.ControlCatalogDialog.prompt", return_value=([], {})):
            window.edit_control_catalog("switches", "Switches")
        self.assertEqual(window.recent_actor_spawn_switch, "")
        self.assertTrue(window.recent_actor_spawn_initially_on)
        self.assertIsNone(window.recent_pressure_plate_switch)
        self.assertIsNone(window.recent_nested_map.switch)
        self.assertTrue(window.recent_nested_map.initially_on)
        self.assertEqual(window.recent_nested_map.motion, "cycle")

    def test_a_field_rename_follows_into_the_barrier_bridge_and_key_defaults(self):
        window = self.window
        window.recent_barrier_field = window.recent_item_key_field = "treasure"
        window.recent_bridge_field = "lobby"
        fields = [{"id": "vault", "color": "#ff3333"}]
        with patch(
            "map_editor.control_actions.ControlCatalogDialog.prompt", return_value=(fields, {"treasure": "vault"})
        ):
            window.edit_control_catalog("fields", "Fields")
        self.assertEqual(window.fields, ["vault"])
        self.assertEqual((window.recent_barrier_field, window.recent_item_key_field), ("vault", "vault"))
        self.assertIsNone(window.recent_bridge_field)

    def test_the_fields_dialog_offers_the_maps_switches(self):
        window = self.window
        window.doc.root_data["switches"] = [{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}]
        window.refresh_ui()
        with patch("map_editor.control_actions.ControlCatalogDialog.prompt", return_value=None) as prompt:
            window.edit_control_catalog("fields", "Fields")
        self.assertEqual(prompt.call_args.kwargs["switches"], ["door"])
