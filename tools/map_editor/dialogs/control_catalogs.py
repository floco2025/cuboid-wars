import copy

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QFormLayout,
    QHBoxLayout,
    QMessageBox,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from ..control_catalogs import validate_catalog
from .controls import SwitchControl


class ControlCatalogDialog(QDialog):
    def __init__(self, parent, title, catalog, entries):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.catalog = catalog
        self.fields = (
            ["id", "color"]
            if catalog != "switch_kinds"
            else ["id", "activation", "reset_on_player_death", "held", "plate_color"]
        )
        labels = (
            ["Name", "Color"]
            if catalog != "switch_kinds"
            else ["Name", "Activation", "Reset on player death", "Held", "Color override"]
        )
        self.table = QTableWidget(0, len(self.fields))
        self.table.setHorizontalHeaderLabels(labels)
        for entry in entries:
            self.add_entry(entry, entry["id"])
        add = QPushButton("Add")
        add.clicked.connect(
            lambda: self.add_entry(
                {
                    "id": "",
                    "color": "#ffffff",
                    "activation": "momentary",
                    "reset_on_player_death": "never",
                    "held": "any",
                }
            )
        )
        remove = QPushButton("Delete")
        remove.clicked.connect(lambda: self.table.removeRow(self.table.currentRow()))
        row = QHBoxLayout()
        row.addWidget(add)
        row.addWidget(remove)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout = QVBoxLayout(self)
        layout.addWidget(self.table)
        layout.addLayout(row)
        layout.addWidget(buttons)
        self.resize(760 if catalog == "switch_kinds" else 420, 360)

    def add_entry(self, entry, original=None):
        row = self.table.rowCount()
        self.table.insertRow(row)
        choices = {
            "activation": ["momentary", "toggle", "auto"],
            "reset_on_player_death": ["never", "solo", "any", "all"],
            "held": ["any", "everyone"],
        }
        for column, field in enumerate(self.fields):
            value = entry.get(field, "any" if field == "held" else "") or ""
            if field in choices:
                box = QComboBox()
                box.addItems(choices[field])
                if value not in choices[field]:
                    box.addItem(value)
                box.setCurrentText(value)
                self.table.setCellWidget(row, column, box)
            else:
                item = QTableWidgetItem(value)
                if field == "id":
                    item.setData(Qt.ItemDataRole.UserRole, original)
                self.table.setItem(row, column, item)

    def values(self):
        entries, renames = [], {}
        for row in range(self.table.rowCount()):
            entry = {}
            for column, field in enumerate(self.fields):
                widget = self.table.cellWidget(row, column)
                value = widget.currentText() if widget is not None else self.table.item(row, column).text()
                if field != "plate_color" or value:
                    entry[field] = value
            original = self.table.item(row, 0).data(Qt.ItemDataRole.UserRole)
            if original and original != entry["id"]:
                renames[original] = entry["id"]
            entries.append(entry)
        return entries, renames

    def accept(self):
        try:
            validate_catalog(self.catalog, self.values()[0])
        except ValueError as exc:
            QMessageBox.warning(self, self.windowTitle(), str(exc))
            return
        super().accept()

    @classmethod
    def prompt(cls, *args):
        dialog = cls(*args)
        return dialog.values() if dialog.exec() == QDialog.DialogCode.Accepted else None


class FireworksDialog(QDialog):
    def __init__(self, parent, switches, current):
        super().__init__(parent)
        self.setWindowTitle("Fireworks")
        current = copy.deepcopy(current or {})
        self.control = SwitchControl(switches, current.get("switch"), current.get("switch_inverted", False))
        self.cooldown = QDoubleSpinBox()
        self.cooldown.setRange(0, 86400)
        self.cooldown.setValue(current.get("cooldown_secs", 0))
        form = QFormLayout()
        form.addRow(self.control)
        form.addRow("Cooldown between shows (s):", self.cooldown)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def value(self):
        values = self.control.values()
        return {**values, "cooldown_secs": self.cooldown.value()} if values.get("switch") else None
