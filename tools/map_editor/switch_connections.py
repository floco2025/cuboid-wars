"""Optional canvas visualization of selected switch relationships."""

from dataclasses import dataclass

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QPen

from .elements import ELEMENT_MODES, element_refs
from .transforms import record_levels, record_rect


@dataclass(frozen=True)
class Connection:
    map_name: str | None
    # None for the map-wide fireworks target, which has no place to draw or visit.
    ref: object | None
    switch: str
    role: str
    lower: int
    upper: int
    rect: tuple | None

    @property
    def plate(self):
        return self.role == "Plate"


def connections_for(root, switches):
    if not switches:
        return []
    connections = []
    for name, data in [(None, root), *root.get("nested_geometry", {}).items()]:
        for ref, entry in element_refs(data):
            switch = entry.get("switch")
            if isinstance(switch, str) and switch in switches:
                lower, upper = record_levels(entry, ref.level)
                role = "Plate" if ref.name == "pressure_plates" else ELEMENT_MODES[ref.name]
                connections.append(Connection(name, ref, switch, role, lower, upper, record_rect(ref.name, entry)))
    fireworks = root.get("fireworks")
    if isinstance(fireworks, dict) and fireworks.get("switch") in switches:
        connections.append(Connection(None, None, fireworks["switch"], "Fireworks", -1, -1, None))
    return connections


class ConnectionOverlay:
    def __init__(self, window):
        self.window = window
        self.enabled = False
        self.connections = []

    def set_enabled(self, enabled):
        self.enabled = enabled
        self.window.canvas.update()

    def set_selection(self, refs):
        values = (ref.get(self.window.map_data).get("switch") for ref in refs)
        switches = {value for value in values if isinstance(value, str) and value}
        self.connections = connections_for(self.window.doc.root_data, switches)
        self.window.canvas.update()

    def paint(self, painter, cell):
        if not self.enabled:
            return
        visible = [
            connection
            for connection in self.connections
            if connection.ref is not None
            and connection.map_name == self.window.doc.active_map
            and connection.lower <= self.window.current_level <= connection.upper
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
