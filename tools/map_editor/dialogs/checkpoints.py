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
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from ..checkpoint_numbers import checkpoint_entries, renumber_checkpoints
from ..constants import CHECKPOINT_TYPE_LABELS

MAX_NUMBER = 999_999


class CheckpointsDialog(QDialog):
    """Renumbers and reorders every checkpoint of the document in one edit;
    the zones ending at a renumbered checkpoint follow it."""

    def __init__(self, parent, root, outer_name):
        super().__init__(parent)
        self.setWindowTitle("Edit Checkpoints")
        self.before = root
        self.after = None
        self.rows = [
            {
                "original": entry["number"] if type(entry.get("number")) is int else None,
                "number": entry["number"] if type(entry.get("number")) is int else 0,
                "map": name or outer_name,
                "type": CHECKPOINT_TYPE_LABELS.get(entry.get("type"), str(entry.get("type"))),
                "level": str(entry.get("level", "?")),
                "cells": f"cols {entry['cols'][0]}–{entry['cols'][1]}, rows {entry['rows'][0]}–{entry['rows'][1]}",
            }
            for name, entry in checkpoint_entries(root)
        ]
        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels(["Number", "Map", "Type", "Level", "Cells"])
        self.table.verticalHeader().hide()
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(4, QHeaderView.ResizeMode.Stretch)
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.up_button = QPushButton("Move Up")
        self.up_button.setAutoDefault(False)
        self.up_button.clicked.connect(lambda: self.move(-1))
        self.down_button = QPushButton("Move Down")
        self.down_button.setAutoDefault(False)
        self.down_button.clicked.connect(lambda: self.move(1))
        self.renumber_button = QPushButton("Renumber 1…N")
        self.renumber_button.setAutoDefault(False)
        self.renumber_button.clicked.connect(self.renumber)
        actions = QHBoxLayout()
        actions.addWidget(self.up_button)
        actions.addWidget(self.down_button)
        actions.addWidget(self.renumber_button)
        actions.addStretch()
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel("Edit numbers, or move a checkpoint to swap its number with its neighbour's."))
        layout.addWidget(self.table)
        layout.addLayout(actions)
        layout.addWidget(buttons)
        self.table.currentCellChanged.connect(self.sync_buttons)
        self.fill(0)
        self.resize(640, 420)

    def fill(self, selected):
        self.table.setRowCount(0)
        for row, data in enumerate(self.rows):
            self.table.insertRow(row)
            spin = QSpinBox()
            spin.setRange(1, MAX_NUMBER)
            spin.setValue(max(1, data["number"]))
            spin.valueChanged.connect(lambda value, index=row: self.rows[index].__setitem__("number", value))
            self.table.setCellWidget(row, 0, spin)
            for column, key in enumerate(("map", "type", "level", "cells"), start=1):
                item = QTableWidgetItem(data[key])
                item.setFlags(item.flags() & ~Qt.ItemFlag.ItemIsEditable)
                self.table.setItem(row, column, item)
        if self.rows:
            self.table.setCurrentCell(min(selected, len(self.rows) - 1), 1)
        self.sync_buttons()

    def sync_buttons(self, *_):
        row = self.table.currentRow()
        self.up_button.setEnabled(row > 0)
        self.down_button.setEnabled(0 <= row < len(self.rows) - 1)
        self.renumber_button.setEnabled(bool(self.rows))

    def spin(self, row):
        return self.table.cellWidget(row, 0)

    def move(self, delta):
        row = self.table.currentRow()
        other = row + delta
        if row < 0 or not 0 <= other < len(self.rows):
            return
        self.rows[row]["number"], self.rows[other]["number"] = self.rows[other]["number"], self.rows[row]["number"]
        self.rows[row], self.rows[other] = self.rows[other], self.rows[row]
        self.fill(other)

    def renumber(self):
        for index, data in enumerate(self.rows, start=1):
            data["number"] = index
        self.fill(max(0, self.table.currentRow()))

    def mapping(self):
        return {
            data["original"]: data["number"]
            for data in self.rows
            if data["original"] is not None and data["number"] != data["original"]
        }

    def accept(self):
        try:
            self.after = renumber_checkpoints(self.before, self.mapping())
        except ValueError as exc:
            QMessageBox.warning(self, self.windowTitle(), str(exc))
            return
        super().accept()

    @classmethod
    def prompt(cls, *args):
        dialog = cls(*args)
        return dialog.after if dialog.exec() == QDialog.DialogCode.Accepted else None
