"""Validation results that navigate to their map objects."""

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QDialog, QDialogButtonBox, QLabel, QListWidget, QListWidgetItem, QPushButton, QVBoxLayout


class IssuesDialog(QDialog):
    focused = Signal(object)
    repair_requested = Signal()

    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("Map Issues")
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.Window)
        self.setSizeGripEnabled(True)
        self.resize(620, 380)
        layout = QVBoxLayout(self)
        self.summary = QLabel("No issues")
        layout.addWidget(self.summary)
        self.list = QListWidget()
        self.list.setWordWrap(True)
        self.list.itemClicked.connect(lambda item: self.focused.emit(item.data(Qt.ItemDataRole.UserRole)))
        self.list.itemActivated.connect(lambda item: self.focused.emit(item.data(Qt.ItemDataRole.UserRole)))
        repair = QPushButton("Review Repairs…")
        repair.clicked.connect(self.repair_requested.emit)
        layout.addWidget(self.list)
        layout.addWidget(repair)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    def set_issues(self, issues: list) -> None:
        self.list.clear()
        self.setWindowTitle(f"Map Issues ({len(issues)})" if issues else "Map Issues")
        warnings = sum(issue.warning for issue in issues)
        counts = [f"{len(issues) - warnings} error(s)"] if len(issues) > warnings else []
        if warnings:
            counts.append(f"{warnings} warning(s), which block nothing")
        self.summary.setText(", ".join(counts) if issues else "No issues")
        for issue in sorted(issues, key=lambda issue: issue.warning):
            item = QListWidgetItem(f"Warning: {issue.message}" if issue.warning else issue.message)
            item.setData(Qt.ItemDataRole.UserRole, issue)
            self.list.addItem(item)
