from PySide6.QtCore import Signal
from PySide6.QtWidgets import QFormLayout, QLabel, QLineEdit, QWidget

from ..spawn_counts import actor_count_error


class SpawnCountControl(QWidget):
    validity_changed = Signal(bool)

    def __init__(self, count):
        super().__init__()
        self.counts = QLineEdit(", ".join(map(str, count)) if isinstance(count, list) else str(count))
        self.counts.setPlaceholderText("2, 3, 4, 6")
        self.counts.setToolTip("Counts for one, two, three, etc. logged-in players. The last count repeats.")
        self.error = QLabel()
        self.error.setWordWrap(True)
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        form.addRow("Counts by players:", self.counts)
        form.addRow(self.error)
        self.counts.textChanged.connect(self.refresh)
        self.refresh()

    def value(self):
        try:
            count = [int(part.strip()) for part in self.counts.text().split(",")]
        except ValueError:
            raise ValueError("Enter comma-separated whole numbers. One number keeps the count fixed.") from None
        if error := actor_count_error(count):
            raise ValueError(error)
        return count

    def refresh(self, *_):
        try:
            self.value()
            message = ""
        except ValueError as error:
            message = str(error)
        self.error.setText(message)
        self.error.setVisible(bool(message))
        self.validity_changed.emit(not message)
