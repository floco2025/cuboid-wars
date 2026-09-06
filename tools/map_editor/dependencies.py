"""File notifications for nested geometry and editor catalogs."""

from PySide6.QtCore import QFileSystemWatcher, QObject, QTimer, Signal

from .constants import ASSETS_PATH, GAMEPLAY_PATH, MAPS_DIR


class MapDependencies(QObject):
    changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.watcher = QFileSystemWatcher(self)
        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.setInterval(100)
        self.timer.timeout.connect(self.changed.emit)
        self.watcher.fileChanged.connect(lambda _: self.timer.start())
        self.watcher.directoryChanged.connect(lambda _: self.timer.start())

    def watch(self, names) -> None:
        files = {GAMEPLAY_PATH, ASSETS_PATH}
        files.update(MAPS_DIR / f"{name}.json" for name in names)
        directories = {MAPS_DIR, *(path.parent for path in files)}
        desired = {str(path.resolve()) for path in files | directories if path.exists()}
        current = set(self.watcher.files()) | set(self.watcher.directories())
        if current - desired:
            self.watcher.removePaths(sorted(current - desired))
        if desired - current:
            self.watcher.addPaths(sorted(desired - current))
