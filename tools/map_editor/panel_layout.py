"""Persistent widths for the permanent editor panels."""

from PySide6.QtCore import QEvent, QObject, QTimer, Qt


class PanelLayout(QObject):
    def __init__(self, window):
        super().__init__(window)
        self.window = window
        self.restoring = True
        self.docks = (
            window.tool_palette,
            window.properties_panel,
        )
        for dock in self.docks:
            dock.installEventFilter(self)
            dock.toggleViewAction().setEnabled(False)
            dock.toggleViewAction().setVisible(False)
        QTimer.singleShot(0, self.restore)

    def preference(self, dock):
        if dock is self.window.tool_palette:
            return ("panels/tools/icons", 84) if dock.icon_only else ("panels/tools/labels", 220)
        return "panels/right/width", 235

    def resize(self, dock):
        key, default = self.preference(dock)
        width = self.window.preferences.value(key, default, type=int)
        width = max(dock.minimumSizeHint().width(), min(width, self.window.width() // 2))
        self.window.resizeDocks([dock], [width], Qt.Orientation.Horizontal)

    def restore(self):
        self.restoring = True
        # Qt caches the dock minimum while its initial label layout is built.
        self.window.tool_palette.reflow()
        for dock in self.docks:
            self.resize(dock)
        self.window.layout().activate()
        self.restoring = False

    def eventFilter(self, watched, event):
        if event.type() == QEvent.Type.Resize and not self.restoring:
            if watched.isVisible() and not watched.visibleRegion().isEmpty():
                key, _ = self.preference(watched)
                self.window.preferences.setValue(key, watched.width())
        return super().eventFilter(watched, event)
