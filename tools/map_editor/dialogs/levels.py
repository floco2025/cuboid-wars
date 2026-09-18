from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QAbstractItemView,
    QDialog,
    QDialogButtonBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QMessageBox,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from ..transforms import dropped_summary, edit_levels_data, map_content_bounds


class LevelsDialog(QDialog):
    def __init__(self, parent, map_data, current_level, *, maintain=None, nested_lookup=None, floor_height_levels=0.0):
        super().__init__(parent)
        self.setWindowTitle("Edit Levels")
        self.before = map_data
        self.maintain = maintain
        self.nested_lookup = nested_lookup
        self.floor_height_levels = floor_height_levels
        self.selected_level = current_level
        self.table = QTableWidget(0, 2)
        self.table.setHorizontalHeaderLabels(["Level", "Name"])
        self.table.verticalHeader().hide()
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        self.table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        for index, level in enumerate(map_data["levels"]):
            self.insert_row(index, level.get("name") or f"Level {index}", index)

        self.add_button = QPushButton("Add")
        self.add_button.setAutoDefault(False)
        self.add_button.clicked.connect(self.add_level)
        self.remove_button = QPushButton("Remove")
        self.remove_button.setAutoDefault(False)
        self.remove_button.clicked.connect(self.remove_level)
        self.up_button = QPushButton("Move Up")
        self.up_button.setAutoDefault(False)
        self.up_button.setToolTip("Move the selected row toward level 0, together with its contents.")
        self.up_button.clicked.connect(lambda: self.move_level(-1))
        self.down_button = QPushButton("Move Down")
        self.down_button.setAutoDefault(False)
        self.down_button.setToolTip("Move the selected row toward higher level numbers, together with its contents.")
        self.down_button.clicked.connect(lambda: self.move_level(1))
        self.shrink_button = QPushButton("Shrink to fit")
        self.shrink_button.setAutoDefault(False)
        self.shrink_button.setToolTip("Remove empty levels below and above the contents; keep empty levels in between.")
        self.shrink_button.clicked.connect(self.shrink_to_fit)
        actions = QHBoxLayout()
        actions.addWidget(self.add_button)
        actions.addWidget(self.remove_button)
        actions.addWidget(self.up_button)
        actions.addWidget(self.down_button)
        actions.addWidget(self.shrink_button)
        self.summary = QLabel()
        self.summary.setTextFormat(Qt.TextFormat.PlainText)
        self.summary.setWordWrap(True)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel("Edit names in the table. Add inserts above the selected level."))
        layout.addWidget(self.table)
        layout.addLayout(actions)
        layout.addWidget(self.summary)
        layout.addWidget(buttons)
        self.table.currentCellChanged.connect(self.sync_buttons)
        self.table.itemChanged.connect(self.update_summary)
        self.table.setCurrentCell(current_level, 1)
        self.update_summary()
        self.sync_buttons()
        self.resize(520, 420)

    def insert_row(self, index, name, original):
        self.table.insertRow(index)
        number = QTableWidgetItem(str(index))
        number.setFlags(number.flags() & ~Qt.ItemFlag.ItemIsEditable)
        number.setData(Qt.ItemDataRole.UserRole, original)
        self.table.setItem(index, 0, number)
        self.table.setItem(index, 1, QTableWidgetItem(name))

    def values(self):
        return [
            (self.table.item(row, 0).data(Qt.ItemDataRole.UserRole), self.table.item(row, 1).text())
            for row in range(self.table.rowCount())
        ]

    def edited_data(self):
        after = edit_levels_data(self.before, self.values())
        return self.maintain(after) if self.maintain else after

    def sync_buttons(self, *_):
        row = self.table.currentRow()
        self.remove_button.setEnabled(self.table.rowCount() > 1 and row >= 0)
        self.up_button.setEnabled(row > 0)
        self.down_button.setEnabled(0 <= row < self.table.rowCount() - 1)

    def update_summary(self, *_):
        summary = dropped_summary(self.before, self.edited_data())
        self.summary.setText(summary)
        self.summary.setVisible(bool(summary))

    def refresh_rows(self, selected):
        for row in range(self.table.rowCount()):
            self.table.item(row, 0).setText(str(row))
        self.table.blockSignals(False)
        self.table.setCurrentCell(selected, 1)
        self.sync_buttons()
        self.update_summary()

    def add_level(self):
        row = self.table.currentRow()
        self.table.setCurrentItem(None)
        index = row + 1 if row >= 0 else self.table.rowCount()
        self.table.blockSignals(True)
        self.insert_row(index, f"Level {index}", None)
        self.refresh_rows(index)
        self.table.editItem(self.table.item(index, 1))

    def remove_level(self):
        row = self.table.currentRow()
        if row < 0 or self.table.rowCount() <= 1:
            return
        self.table.setCurrentItem(None)
        self.table.blockSignals(True)
        self.table.removeRow(row)
        self.refresh_rows(min(row, self.table.rowCount() - 1))

    def move_level(self, delta):
        row = self.table.currentRow()
        target = row + delta
        if row < 0 or not 0 <= target < self.table.rowCount():
            return
        self.table.setCurrentItem(None)
        self.table.blockSignals(True)
        for column in range(self.table.columnCount()):
            selected = self.table.takeItem(row, column)
            neighbour = self.table.takeItem(target, column)
            self.table.setItem(row, column, neighbour)
            self.table.setItem(target, column, selected)
        self.refresh_rows(target)

    def shrink_to_fit(self):
        selected = max(0, self.table.currentRow())
        self.table.setCurrentItem(None)
        bounds = map_content_bounds(
            self.edited_data(), nested_lookup=self.nested_lookup, floor_height_levels=self.floor_height_levels
        )
        self.table.blockSignals(True)
        for row in reversed(range(self.table.rowCount())):
            if row < bounds.first_level or row > bounds.last_level:
                self.table.removeRow(row)
        self.refresh_rows(max(0, min(selected - bounds.first_level, self.table.rowCount() - 1)))

    def accept(self):
        self.selected_level = max(0, self.table.currentRow())
        # Commit a name still being typed even when OK takes no focus on macOS.
        self.table.setCurrentItem(None)
        self.table.setCurrentCell(self.selected_level, 1)
        summary = dropped_summary(self.before, self.edited_data())
        if summary:
            result = QMessageBox.question(
                self,
                self.windowTitle(),
                summary + "\n\nApply these level changes?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
                QMessageBox.StandardButton.Cancel,
            )
            if result != QMessageBox.StandardButton.Yes:
                return
        super().accept()

    @classmethod
    def prompt(cls, *args, **kwargs):
        dialog = cls(*args, **kwargs)
        return (dialog.edited_data(), dialog.selected_level) if dialog.exec() == QDialog.DialogCode.Accepted else None
