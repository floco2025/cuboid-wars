"""File notifications for editor catalogs."""

from PySide6.QtCore import QFileSystemWatcher, QObject, QTimer, Signal

from .catalogs import map_settings_path
from .constants import ASSETS_PATH, GAMEPLAY_PATH


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

    def watch(self, map_name: str) -> None:
        files = {ASSETS_PATH, GAMEPLAY_PATH, map_settings_path(map_name)}
        directories = {path.parent for path in files}
        desired = {str(path.resolve()) for path in files | directories if path.exists()}
        current = set(self.watcher.files()) | set(self.watcher.directories())
        if current - desired:
            self.watcher.removePaths(sorted(current - desired))
        if desired - current:
            self.watcher.addPaths(sorted(desired - current))
