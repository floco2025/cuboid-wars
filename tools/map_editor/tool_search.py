"""Keyboard tool selection, including the element-specific erase modes."""

from PySide6.QtCore import QEvent, QSize, Qt, Signal
from PySide6.QtWidgets import QDialog, QLabel, QLineEdit, QListWidget, QListWidgetItem, QVBoxLayout

from .tool_catalog import MODE_GROUPS, MODE_TO_TOOL, TOOLS
from .tool_icons import tool_icon


class ToolSearch(QDialog):
    mode_requested = Signal(str)

    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("Find tool")
        self.setWindowModality(Qt.WindowModality.WindowModal)
        self.resize(440, 440)
        layout = QVBoxLayout(self)
        self.query = QLineEdit()
        self.query.setPlaceholderText("Find tool…")
        self.query.setAccessibleName("Find tool")
        self.query.setClearButtonEnabled(True)
        self.query.installEventFilter(self)
        layout.addWidget(self.query)
        self.results = QListWidget()
        self.results.setAccessibleName("Matching tools")
        self.results.setIconSize(QSize(24, 24))
        layout.addWidget(self.results)
        self.empty = QLabel("No matching tools")
        layout.addWidget(self.empty)
        self.query.textChanged.connect(self.filter)
        self.query.returnPressed.connect(self.choose)
        self.results.itemClicked.connect(self.choose)
        self.results.itemActivated.connect(self.choose)
        self.filter("")

    def show_for(self, mode: str) -> None:
        self.query.clear()
        self.filter("")
        for row in range(self.results.count()):
            if self.results.item(row).data(Qt.ItemDataRole.UserRole) == mode:
                self.results.setCurrentRow(row)
                break
        self.open()
        self.query.setFocus()

    def filter(self, query: str) -> None:
        words = query.casefold().split()
        self.results.clear()
        for mode, base in MODE_TO_TOOL.items():
            group = MODE_GROUPS[mode]
            searchable = f"{mode} {TOOLS[base].label} {group}".casefold()
            if all(word in searchable for word in words):
                item = QListWidgetItem(tool_icon(mode), mode)
                item.setData(Qt.ItemDataRole.UserRole, mode)
                item.setToolTip(f"{group} — {mode}" if group else mode)
                item.setSizeHint(QSize(0, 34))
                self.results.addItem(item)
        self.empty.setVisible(self.results.count() == 0)
        self.results.setCurrentRow(0)

    def choose(self, *_args) -> None:
        item = self.results.currentItem()
        if item is not None:
            mode = item.data(Qt.ItemDataRole.UserRole)
            self.accept()
            self.mode_requested.emit(mode)

    def eventFilter(self, watched, event) -> bool:
        if watched is self.query and event.type() == QEvent.Type.KeyPress:
            if event.key() in (Qt.Key.Key_Up, Qt.Key.Key_Down):
                step = 1 if event.key() == Qt.Key.Key_Down else -1
                row = min(max(self.results.currentRow() + step, 0), self.results.count() - 1)
                self.results.setCurrentRow(row)
                return True
        return super().eventFilter(watched, event)
