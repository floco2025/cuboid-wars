"""File notifications for editor catalogs."""

from PySide6.QtCore import QFileSystemWatcher, QObject, QTimer, Signal

from .catalogs import map_settings_path
from .constants import ASSETS_PATH, GAMEPLAY_PATH


class MapDependencies(QObject):
    changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.watcher = QFileSystemWatcher(self)
        self.files = set()
        self.contents = {}
        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.setInterval(100)
        self.timer.timeout.connect(self.check)
        self.watcher.fileChanged.connect(lambda _: self.timer.start())
        self.watcher.directoryChanged.connect(lambda _: self.timer.start())

    def watch(self, map_name: str) -> None:
        files = {ASSETS_PATH, GAMEPLAY_PATH, map_settings_path(map_name)}
        if files != self.files:
            self.files = files
            self.contents = self.read_contents()
        self.refresh_watches()

    def read_contents(self):
        contents = {}
        for path in self.files:
            try:
                contents[path] = path.read_bytes()
            except OSError:
                contents[path] = None
        return contents

    def check(self):
        contents = self.read_contents()
        self.refresh_watches()
        if contents != self.contents:
            self.contents = contents
            self.changed.emit()

    def refresh_watches(self):
        # Directories catch atomic replacements, but autosaves and other
        # siblings must not masquerade as catalog changes.
        directories = {path.parent for path in self.files}
        desired = {str(path.resolve()) for path in self.files | directories if path.exists()}
        current = set(self.watcher.files()) | set(self.watcher.directories())
        if current - desired:
            self.watcher.removePaths(sorted(current - desired))
        if desired - current:
            self.watcher.addPaths(sorted(desired - current))
