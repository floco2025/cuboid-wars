import copy
from dataclasses import replace

from PySide6.QtWidgets import QDialog

from .control_catalogs import edit_catalog
from .dialogs.control_catalogs import ControlCatalogDialog, FireworksDialog


class ControlActionsMixin:
    def build_control_menu(self):
        menu = self.menuBar().addMenu("&Map")
        for catalog, title in (
            ("switch_kinds", "Pressure Plate Kinds"),
            ("barrier_kinds", "Barrier Kinds"),
            ("bridge_kinds", "Bridge Kinds"),
        ):
            self.add_menu_action(
                menu, title + "…", None, lambda checked=False, c=catalog, t=title: self.edit_control_catalog(c, t)
            )
        self.add_menu_action(menu, "Fireworks…", None, self.edit_fireworks)

    def edit_control_catalog(self, catalog, title):
        root = self.doc.root_data
        source = root if catalog == "switch_kinds" else root.get("_settings", {})
        if catalog != "switch_kinds" and catalog not in source:
            self.notify("Open a map with settings.json to edit its appearance kinds.")
            return
        result = ControlCatalogDialog.prompt(self, title, catalog, source.get(catalog, []))
        if result is None:
            return
        entries, renames = result
        try:
            after = edit_catalog(root, catalog, entries, renames)
        except ValueError as exc:
            self.notify(str(exc))
            return
        self.doc.apply_root_change("Edit " + title, after, self.doc.active_map)
        self.retarget_defaults(catalog, {entry["id"] for entry in entries}, renames)

    # The toolbar defaults name kinds like the document does, so a catalog
    # edit renames or drops them the same way.
    def retarget_defaults(self, catalog, remaining, renames):
        def follow(name):
            name = renames.get(name, name)
            return name if name in remaining else None

        if catalog == "switch_kinds":
            for attribute in ("recent_barrier_controls", "recent_bridge_controls"):
                controls = getattr(self, attribute)
                switch = follow(controls["switch"]) if controls.get("switch") else None
                setattr(self, attribute, {**controls, "switch": switch} if switch else {})
            self.recent_actor_spawn_switch = follow(self.recent_actor_spawn_switch) or ""
            if not self.recent_actor_spawn_switch:
                self.recent_actor_spawn_inverted = False
            switch = follow(self.recent_pressure_plate_switch) if self.recent_pressure_plate_switch else None
            self.recent_pressure_plate_switch = switch or (self.switches[0] if self.switches else None)
            motion = self.recent_nested_map
            if motion is not None and motion.switch:
                switch = follow(motion.switch)
                self.recent_nested_map = replace(
                    motion, switch=switch, switch_inverted=bool(switch) and motion.switch_inverted
                )
        elif catalog == "barrier_kinds":
            self.recent_barrier_kind = follow(self.recent_barrier_kind) if self.recent_barrier_kind else None
            self.recent_item_key_kind = follow(self.recent_item_key_kind) if self.recent_item_key_kind else None
        else:
            self.recent_bridge_kind = follow(self.recent_bridge_kind) if self.recent_bridge_kind else None
        self.tool_settings.refresh()

    def edit_fireworks(self):
        dialog = FireworksDialog(self, self.switches, self.doc.root_data.get("fireworks"))
        if dialog.exec() == QDialog.DialogCode.Accepted:
            after = copy.deepcopy(self.doc.root_data)
            after["fireworks"] = dialog.value()
            self.doc.apply_root_change("Edit Fireworks", after, self.doc.active_map)
