import copy
from dataclasses import replace

from PySide6.QtWidgets import QDialog

import pathlib

from .checkpoint_numbers import next_checkpoint_number
from .control_catalogs import catalog_usage, edit_catalog
from .dialogs import CheckpointsDialog
from .dialogs.control_catalogs import ControlCatalogDialog, FireworksDialog
from .nesting import DEFAULT_MOTION


class ControlActionsMixin:
    def build_control_menu(self, menu):
        for catalog, title in (("switches", "Switches"), ("fields", "Fields")):
            self.add_menu_action(
                menu, title + "…", None, lambda checked=False, c=catalog, t=title: self.edit_control_catalog(c, t)
            )
        self.add_menu_action(menu, "Fireworks…", None, self.edit_fireworks)
        menu.addSeparator()
        self.add_menu_action(menu, "Check Map…", None, self.show_map_issues)

    def edit_control_catalog(self, catalog, title):
        root = self.doc.root_data
        result = ControlCatalogDialog.prompt(
            self,
            title,
            catalog,
            root.get(catalog, []),
            catalog_usage(root, catalog),
            switches=self.switches,
            switch_colors=self.switch_colors,
        )
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

    # The toolbar defaults name fields and switches like the document does, so
    # a catalog edit renames or drops them the same way.
    def retarget_defaults(self, catalog, remaining, renames):
        def follow(name):
            name = renames.get(name, name)
            return name if name in remaining else None

        if catalog == "switches":
            if self.recent_actor_spawn_switch:
                self.recent_actor_spawn_switch = follow(self.recent_actor_spawn_switch) or ""
                # An initial state was chosen for the switch that flips it, so a
                # default losing its switch returns to the placement default, On.
                if not self.recent_actor_spawn_switch:
                    self.recent_actor_spawn_initially_on = True
            switch = follow(self.recent_pressure_plate_switch) if self.recent_pressure_plate_switch else None
            self.recent_pressure_plate_switch = switch or (self.switches[0] if self.switches else None)
            motion = self.recent_nested_map
            if motion is not None and motion.switch:
                switch = follow(motion.switch)
                # A Follow switch never moves without its switch, and the next
                # placement reuses this default without a dialog to say so.
                self.recent_nested_map = replace(
                    motion,
                    switch=switch,
                    initially_on=motion.initially_on or not switch,
                    motion=motion.motion if switch else DEFAULT_MOTION,
                )
        else:
            for attribute in ("recent_barrier_field", "recent_bridge_field", "recent_item_key_field"):
                setattr(self, attribute, follow(getattr(self, attribute)))
        self.tool_settings.refresh()

    def edit_checkpoints(self):
        root = self.doc.root_data
        outer = pathlib.Path(self.path).parent.name if self.path else "Outer map"
        after = CheckpointsDialog.prompt(self, root, outer)
        if after is None or not self.doc.apply_root_change("Edit Checkpoints", after, self.doc.active_map):
            return
        self.recent_checkpoint_number = next_checkpoint_number(self.doc.root_data)
        self.tool_settings.refresh()

    def edit_fireworks(self):
        dialog = FireworksDialog(self, self.switches, self.doc.root_data.get("fireworks"))
        if dialog.exec() == QDialog.DialogCode.Accepted:
            after = copy.deepcopy(self.doc.root_data)
            after["fireworks"] = dialog.value()
            self.doc.apply_root_change("Edit Fireworks", after, self.doc.active_map)
