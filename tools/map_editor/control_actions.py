import copy

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
        try:
            after = edit_catalog(root, catalog, *result)
        except ValueError as exc:
            self.notify(str(exc))
            return
        self.doc.apply_root_change("Edit " + title, after, self.doc.active_map)

    def edit_fireworks(self):
        dialog = FireworksDialog(self, self.switches, self.doc.root_data.get("fireworks"))
        if dialog.exec() == QDialog.DialogCode.Accepted:
            after = copy.deepcopy(self.doc.root_data)
            after["fireworks"] = dialog.value()
            self.doc.apply_root_change("Edit Fireworks", after, self.doc.active_map)
