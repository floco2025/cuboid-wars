"""Editor-wide window geometry persistence."""

from PySide6.QtCore import QByteArray, QEvent, QObject, QSettings, QTimer
from PySide6.QtWidgets import QWidget


class WindowGeometry(QObject):
    KEY = "window/geometry"
    SAVE_DELAY_MS = 500

    def __init__(self, window: QWidget, preferences: QSettings):
        super().__init__(window)
        self.window = window
        self.preferences = preferences
        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.setInterval(self.SAVE_DELAY_MS)
        self.timer.timeout.connect(self.save)

        window.resize(1000, 800)
        geometry = preferences.value(self.KEY)
        if isinstance(geometry, QByteArray):
            window.restoreGeometry(geometry)
        window.installEventFilter(self)

    def eventFilter(self, watched, event) -> bool:
        if watched is self.window and self.window.isVisible() and event.type() in (
            QEvent.Type.Move,
            QEvent.Type.Resize,
            QEvent.Type.WindowStateChange,
        ):
            self.timer.start()
        return super().eventFilter(watched, event)

    def save(self) -> None:
        self.timer.stop()
        self.preferences.setValue(self.KEY, self.window.saveGeometry())
        self.preferences.sync()
