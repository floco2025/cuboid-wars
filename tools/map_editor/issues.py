"""Validation results that navigate to their map objects."""

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QDockWidget, QListWidget, QListWidgetItem, QPushButton, QVBoxLayout, QWidget


class IssuesPanel(QDockWidget):
    focused = Signal(object)
    repair_requested = Signal()

    def __init__(self, parent):
        super().__init__("Map Issues", parent)
        body = QWidget()
        layout = QVBoxLayout(body)
        self.list = QListWidget()
        self.list.setWordWrap(True)
        self.list.itemClicked.connect(lambda item: self.focused.emit(item.data(Qt.ItemDataRole.UserRole)))
        self.list.itemActivated.connect(lambda item: self.focused.emit(item.data(Qt.ItemDataRole.UserRole)))
        repair = QPushButton("Review Repairs…")
        repair.clicked.connect(self.repair_requested.emit)
        layout.addWidget(self.list)
        layout.addWidget(repair)
        self.setWidget(body)

    def set_issues(self, issues: list) -> None:
        self.list.clear()
        for issue in issues:
            item = QListWidgetItem(issue.message)
            item.setData(Qt.ItemDataRole.UserRole, issue)
            self.list.addItem(item)
