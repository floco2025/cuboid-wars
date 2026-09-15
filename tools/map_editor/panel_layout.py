"""Persistent widths for the permanent editor panels."""

from PySide6.QtCore import QCoreApplication, QEvent, QObject, QTimer, Qt


class PanelLayout(QObject):
    def __init__(self, window):
        super().__init__(window)
        self.window = window
        self.restoring = True
        self.window_resizing = False
        self.docks = (
            window.tool_palette,
            window.properties_panel,
        )
        for dock in self.docks:
            dock.toggleViewAction().setEnabled(False)
            dock.toggleViewAction().setVisible(False)
        # Qt lays the docks out before the window's own filters see its
        # resize, so the resize is watched from the application instead.
        QCoreApplication.instance().installEventFilter(self)
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
        self.apply(self.docks)

    def reflow_tools(self):
        self.apply((self.window.tool_palette,))

    # Qt caches the dock minimum while its initial label layout is built, so
    # the palette reflows before a saved width is applied; the resizes the
    # reflow itself causes are not saved.
    def apply(self, docks):
        self.restoring = True
        self.window.tool_palette.reflow()
        for dock in docks:
            self.resize(dock)
        self.window.layout().activate()
        self.restoring = False

    # A width is saved when the user resizes a dock; the docks a window
    # resize squeezes keep the saved widths for a roomier window.
    def eventFilter(self, watched, event):
        if event.type() == QEvent.Type.Resize:
            if watched is self.window:
                self.window_resizing = True
                QTimer.singleShot(0, self.end_window_resize)
            elif watched in self.docks and not (self.restoring or self.window_resizing):
                if watched.isVisible() and not watched.visibleRegion().isEmpty():
                    key, _ = self.preference(watched)
                    self.window.preferences.setValue(key, watched.width())
        return super().eventFilter(watched, event)

    def end_window_resize(self):
        self.window_resizing = False
