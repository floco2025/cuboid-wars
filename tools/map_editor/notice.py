"""Temporary canvas feedback that leaves editing and layout untouched."""

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import QLabel

from .constants import STATUS_TIMEOUT_MS


class CanvasNotice(QLabel):
    def __init__(self, canvas):
        super().__init__(canvas)
        self.setTextFormat(Qt.TextFormat.PlainText)
        self.setWordWrap(True)
        self.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.setStyleSheet(
            "background-color: rgba(15, 23, 42, 235); color: #f8fafc;"
            "border: 1px solid #64748b; border-radius: 5px; padding: 8px 12px;"
        )
        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.timeout.connect(self.hide)
        self.hide()

    def show_message(self, message: str) -> None:
        self.setText(message)
        self.reposition()
        self.show()
        self.raise_()
        self.timer.start(STATUS_TIMEOUT_MS)

    def reposition(self) -> None:
        canvas = self.parentWidget()
        width = min(560, canvas.width() - 24, self.fontMetrics().horizontalAdvance(self.text()) + 32)
        self.setFixedWidth(max(1, width))
        self.adjustSize()
        self.move((canvas.width() - self.width()) // 2, max(8, canvas.height() - self.height() - 16))
