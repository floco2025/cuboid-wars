"""On-demand plate relationships, including targets in other geometry."""

from dataclasses import dataclass

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QPen
from PySide6.QtWidgets import QDockWidget, QLabel, QListWidget, QListWidgetItem, QVBoxLayout, QWidget

from .elements import ELEMENT_MODES, element_refs
from .transforms import record_levels, record_rect


@dataclass(frozen=True)
class Connection:
    map_name: str | None
    ref: object
    switch: str
    plate: bool
    lower: int
    upper: int
    rect: tuple


def connections_for(root, switches):
    if not switches:
        return []
    connections = []
    for name, data in [(None, root), *root.get("nested_geometry", {}).items()]:
        for ref, entry in element_refs(data):
            switch = entry.get("switch")
            if isinstance(switch, str) and switch in switches:
                lower, upper = record_levels(entry, ref.level)
                connections.append(
                    Connection(
                        name, ref, switch, ref.name == "pressure_plates", lower, upper, record_rect(ref.name, entry)
                    )
                )
    return connections


class PlateConnections(QDockWidget):
    def __init__(self, window):
        super().__init__("Connections", window)
        self.window = window
        self.setObjectName("plate_connections")
        self.setFeatures(QDockWidget.DockWidgetFeature.DockWidgetClosable)
        self.setAllowedAreas(Qt.DockWidgetArea.RightDockWidgetArea)
        self.connections = []
        body = QWidget()
        layout = QVBoxLayout(body)
        self.summary = QLabel("Select a plate or controlled object.")
        self.summary.setWordWrap(True)
        layout.addWidget(self.summary)
        self.list = QListWidget()
        self.list.setAccessibleName("Plate connections")
        self.list.itemClicked.connect(self.navigate)
        layout.addWidget(self.list)
        self.setWidget(body)

    def set_selection(self, refs):
        values = (ref.get(self.window.map_data).get("switch") for ref in refs)
        switches = {value for value in values if isinstance(value, str) and value}
        self.connections = connections_for(self.window.doc.root_data, switches)
        self.list.clear()
        self.summary.setText(", ".join(sorted(switches)) if switches else "Select a plate or controlled object.")
        for connection in self.connections:
            geometry = (
                self.window.doc.root_data
                if connection.map_name is None
                else self.window.doc.nested_geometry[connection.map_name]
            )
            level = "Missing level"
            if 0 <= connection.lower < len(geometry["levels"]):
                level = geometry["levels"][connection.lower].get("name") or f"Level {connection.lower}"
            role = "Plate" if connection.plate else ELEMENT_MODES[connection.ref.name]
            item = QListWidgetItem(f"{role} · {connection.map_name or 'Outer map'} · {level}")
            item.setData(Qt.ItemDataRole.UserRole, connection)
            item.setToolTip(connection.switch)
            self.list.addItem(item)
        self.window.canvas.update()

    def navigate(self, item):
        connection = item.data(Qt.ItemDataRole.UserRole)
        self.window.doc.select_map(connection.map_name)
        self.window.set_level_index(connection.lower)
        self.window.inspected_refs = [connection.ref]
        self.window.refresh_inspection()
        self.window.canvas.viewport.focus(connection.rect, self.window.canvas.width(), self.window.canvas.height())
        self.window.canvas.refresh_view()

    def paint(self, painter, cell):
        visible = [
            connection
            for connection in self.connections
            if connection.map_name == self.window.doc.active_map
            and connection.lower <= self.window.current_level <= connection.upper
            and connection.ref.name not in self.window.element_filters.hidden
        ]
        if not visible:
            return
        painter.save()
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(QPen(QColor("#fbbf24"), 2, Qt.PenStyle.DashLine))

        def center(connection):
            c0, r0, c1, r1 = connection.rect
            return QPointF((c0 + c1) * cell / 2, (r0 + r1) * cell / 2)

        for connection in visible:
            c0, r0, c1, r1 = connection.rect
            painter.drawRect(
                QRectF(c0 * cell - 2, r0 * cell - 2, max(4, (c1 - c0) * cell) + 4, max(4, (r1 - r0) * cell) + 4)
            )
        for plate in (connection for connection in visible if connection.plate):
            for target in visible:
                if not target.plate and target.switch == plate.switch:
                    painter.drawLine(center(plate), center(target))
        painter.restore()
