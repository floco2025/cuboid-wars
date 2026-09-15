"""Visibility and edit protection by map element type."""

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QDockWidget, QHeaderView, QPushButton, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget

from .elements import ELEMENT_MODES
from .tool_catalog import TOOLS
from .tool_icons import tool_icon


class ElementFilters(QDockWidget):
    changed = Signal()

    def __init__(self, parent):
        super().__init__("Elements", parent)
        self.setObjectName("element_filters")
        self.setFeatures(QDockWidget.DockWidgetFeature.DockWidgetClosable)
        self.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea)
        self.hidden = set()
        self.locked = set()
        body = QWidget()
        layout = QVBoxLayout(body)
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Element", "Show", "Lock"])
        self.tree.setRootIsDecorated(False)
        self.tree.header().setStretchLastSection(False)
        self.tree.header().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        for column in (1, 2):
            self.tree.header().setSectionResizeMode(column, QHeaderView.ResizeMode.ResizeToContents)
        self.rows = {}
        for name, mode in ELEMENT_MODES.items():
            row = QTreeWidgetItem(["Ramp" if name == "ramps" else TOOLS[mode].label, "", ""])
            row.setToolTip(0, mode)
            row.setIcon(0, tool_icon(mode))
            row.setData(0, Qt.ItemDataRole.UserRole, name)
            row.setCheckState(1, Qt.CheckState.Checked)
            row.setCheckState(2, Qt.CheckState.Unchecked)
            row.setToolTip(1, "Hidden elements are excluded from selection and editing")
            row.setToolTip(2, "Locked elements remain visible; editing and clipboard operations skip them")
            self.tree.addTopLevelItem(row)
            self.rows[name] = row
        self.tree.itemChanged.connect(self._changed)
        layout.addWidget(self.tree)
        reset = QPushButton("Show and unlock all")
        reset.clicked.connect(self.reset)
        layout.addWidget(reset)
        self.setWidget(body)
        self.setMinimumWidth(280)

    @property
    def excluded(self):
        return self.hidden | self.locked

    def _changed(self, *_):
        self.hidden = {name for name, row in self.rows.items() if row.checkState(1) != Qt.CheckState.Checked}
        self.locked = {name for name, row in self.rows.items() if row.checkState(2) == Qt.CheckState.Checked}
        self.changed.emit()

    def reset(self):
        self.tree.blockSignals(True)
        for row in self.rows.values():
            row.setCheckState(1, Qt.CheckState.Checked)
            row.setCheckState(2, Qt.CheckState.Unchecked)
        self.tree.blockSignals(False)
        self._changed()
