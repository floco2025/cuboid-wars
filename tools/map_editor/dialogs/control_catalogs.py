from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QFormLayout,
    QHBoxLayout,
    QHeaderView,
    QMessageBox,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from ..constants import INITIAL_STATE, INITIAL_STATE_LABELS
from ..control_catalogs import validate_catalog
from .color_swatch import ColorSwatch
from .controls import choice


SWITCH_COLUMNS = {
    "id": "Name",
    "activation": "Activation",
    "reset_on_player_death": "Reset on player death",
    "held": "Held",
    "color": "Color",
}
FIELD_COLUMNS = {"id": "Name", "color": "Color", "switch": "Switch", "initially_on": INITIAL_STATE}


class ControlCatalogDialog(QDialog):
    # `usage` maps an entry's name in the document to what names it (`catalog_usage`);
    # `switches` are the ones a field may name.
    def __init__(self, parent, title, catalog, entries, usage=None, *, switches=(), switch_colors=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.catalog = catalog
        self.usage = usage or {}
        self.switches = list(switches)
        self.switch_colors = switch_colors
        columns = SWITCH_COLUMNS if catalog == "switches" else FIELD_COLUMNS
        self.fields = list(columns)
        self.table = QTableWidget(0, len(self.fields) + 1)
        self.table.setHorizontalHeaderLabels([*columns.values(), "Used by"])
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        for entry in entries:
            self.add_entry(entry, entry["id"])
        add = QPushButton("Add")
        add.clicked.connect(
            lambda: self.add_entry(
                {
                    "id": "",
                    **({"color": self.fresh_color()} if self.catalog != "switches" else {}),
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
        self.resize(1040 if catalog == "switches" else 820, 360)

    # Fresh fields start apart on the hue wheel rather than all white.
    def fresh_color(self):
        return QColor.fromHsv((self.table.rowCount() * 137) % 360, 190, 235).name()

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
            elif field == "color":
                self.table.setCellWidget(row, column, ColorSwatch(value, optional=self.catalog == "switches"))
            elif field == "switch":
                box = choice(self.switches, entry.get("switch"), optional=True, colors=self.switch_colors)
                self.table.setCellWidget(row, column, box)
            elif field == "initially_on":
                state = INITIAL_STATE_LABELS[entry.get("initially_on") is not False]
                self.table.setCellWidget(row, column, choice(INITIAL_STATE_LABELS.values(), state))
            else:
                item = QTableWidgetItem(value)
                if field == "id":
                    item.setData(Qt.ItemDataRole.UserRole, original)
                self.table.setItem(row, column, item)
        used = QTableWidgetItem(self.usage.get(original, "Unused"))
        used.setFlags(Qt.ItemFlag.ItemIsEnabled if original in self.usage else Qt.ItemFlag.NoItemFlags)
        self.table.setItem(row, len(self.fields), used)

    def values(self):
        entries, renames = [], {}
        for row in range(self.table.rowCount()):
            entry = {}
            for column, field in enumerate(self.fields):
                widget = self.table.cellWidget(row, column)
                if isinstance(widget, ColorSwatch):
                    if widget.color is not None:
                        entry[field] = widget.color
                elif field == "switch":
                    if widget.currentData():
                        entry[field] = widget.currentData()
                elif field == "initially_on":
                    if widget.currentData() == INITIAL_STATE_LABELS[False]:
                        entry[field] = False
                else:
                    entry[field] = widget.currentText() if widget is not None else self.table.item(row, column).text()
            original = self.table.item(row, 0).data(Qt.ItemDataRole.UserRole)
            if original and original != entry["id"]:
                renames[original] = entry["id"]
            entries.append(entry)
        return entries, renames

    def accept(self):
        # OK takes no focus on macOS, so a name still being typed is committed here.
        self.table.setCurrentItem(None)
        try:
            validate_catalog(self.catalog, self.values()[0])
        except ValueError as exc:
            QMessageBox.warning(self, self.windowTitle(), str(exc))
            return
        super().accept()

    @classmethod
    def prompt(cls, *args, **kwargs):
        dialog = cls(*args, **kwargs)
        return dialog.values() if dialog.exec() == QDialog.DialogCode.Accepted else None


class FireworksDialog(QDialog):
    def __init__(self, parent, switches, current):
        super().__init__(parent)
        self.setWindowTitle("Fireworks")
        current = current or {}
        self.switch = choice(switches, current.get("switch"), optional=True)
        self.cooldown = QDoubleSpinBox()
        self.cooldown.setRange(0, 86400)
        self.cooldown.setValue(current.get("cooldown_secs", 0))
        form = QFormLayout()
        form.addRow("Switch:", self.switch)
        form.addRow("Cooldown between shows (s):", self.cooldown)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def value(self):
        switch = self.switch.currentData()
        return {"switch": switch, "cooldown_secs": self.cooldown.value()} if switch else None
