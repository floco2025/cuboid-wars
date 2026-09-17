"""Small vector tool silhouettes, rendered at multiple display scales."""

from PySide6.QtCore import QByteArray, Qt
from PySide6.QtGui import QIcon, QPainter, QPalette, QPixmap
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtWidgets import QApplication

from . import constants as c
from .tool_catalog import ERASE_TOOLS, MODE_TO_TOOL

_TILE = '<path d="M3 9 12 4 21 9 12 14Z M3 9v6l9 5 9-5V9 M12 14v6"/>'
_RAMP = '<path d="M3 18 18 5 21 8v11H3Z M18 5v11L3 18 M18 16l3 3"/>'
_BRUSH = '<path d="m13 11 6-7 3 3-7 6Z" fill="#b78ce8"/><path d="M13 12c-5-2-3 6-8 5 4 6 12 0 10-4" fill="#b78ce8"/>'
_ICONS = {
    c.MODE_SAMPLE: '<path d="m5 16 11-11 4 4-11 11H5Z M13 5l6 6M3 22l2-2" fill="#67b8ef"/>',
    c.MODE_SELECT: '<path d="m5 3 14 10-7 1-3 7Z" fill="#67b8ef"/>',
    c.MODE_ERASE: '<path d="m4 14 10-11 8 7-10 11H9Z" fill="#eaaa89"/><path d="m8 10 8 7M12 21h10"/>',
    c.MODE_FLOOR: _TILE,
    c.MODE_INACCESSIBLE_FLOOR: _TILE + '<path d="m9 6 6 5m0-5-6 5" stroke="#e57878"/>',
    c.MODE_TERRAIN: '<path d="m2 19 6-12 4 6 4-10 6 16Z" fill="#7cad7b"/><path d="m6 11 2 2 2-2m4-3 2 2 2-2"/>',
    c.MODE_WALL: '<path d="M3 5h18v15H3Z M3 10h18M3 15h18M9 5v5m7 0v5m-8 0v5"/>',
    c.MODE_RAMP_UP: _RAMP + '<path d="M4 10V3m-3 3 3-3 3 3" stroke="#60b89d"/>',
    c.MODE_RAMP_DOWN: _RAMP + '<path d="M4 3v7m-3-3 3 3 3-3" stroke="#eaaa89"/>',
    c.MODE_LADDER: '<path d="M6 2v20M18 2v20M6 5h12M6 10h12M6 15h12M6 20h12"/>',
    c.MODE_NESTED_MAP: '<rect x="2" y="3" width="19" height="18" rx="2"/><path d="M2 9h19M9 3v18"/><rect x="12" y="12" width="7" height="7" fill="#67b8ef"/>',
    c.MODE_ACTOR_SPAWN_ZONE: '<rect x="2" y="3" width="20" height="18" rx="3" stroke-dasharray="2 3"/><path d="M7 8h10v8H7Z M9 5v3m6-3v3" fill="#eaaa89"/><path d="M10 11v1m4-1v1m-4 3h4"/>',
    c.MODE_CHECKPOINT: '<path d="M5 22V3m0 1c5-5 8 5 15 0v10c-7 5-10-5-15 0" fill="#60b89d"/>',
    c.MODE_ITEM: '<circle cx="12" cy="12" r="9" fill="#e9bd55"/><circle cx="12" cy="12" r="6"/><path d="M12 8v8m-2-2 2 2 2-2"/>',
    c.MODE_BARRIER: '<path d="M3 21V3m18 0v18"/><path d="M6 5h12v14H6Z" fill="#67b8ef"/><path d="m6 11 6-6m-6 12L18 5m-4 14 4-4"/>',
    c.MODE_EQUIPMENT_ERASER: '<path d="M3 21V3m18 0v18"/><path d="m12 4-5 9h5l-1 7 6-10h-5Z" fill="#b78ce8"/>',
    c.MODE_LIGHT_BRIDGE: '<path d="M2 15 10 5h12l-8 10Z" fill="#67b8ef"/><path d="m5 11 12 0M2 15v5m12-5v5m8-15v5"/>',
    c.MODE_PRESSURE_PLATE: '<path d="m2 16 10-5 10 5-10 5Z" fill="#eaaa89"/><path d="M12 2v9m-4-4 4 4 4-4"/>',
    c.MODE_FLOOR_MATERIAL: '<path d="M2 15h9v7H2Z" fill="#60b89d"/>' + _BRUSH,
    c.MODE_WALL_MATERIAL: '<path d="M2 3h7v19H2Z M2 9h7m-7 6h7" fill="#eaaa89"/>' + _BRUSH,
    c.MODE_RAMP_MATERIAL: '<path d="m2 22 9-9v9Z" fill="#67b8ef"/>' + _BRUSH,
    c.MODE_LIGHT: '<path d="M9 18v3h6v-3m-3-16V0M4 5 2 3m18 2 2-2M3 12H1m20 0h2"/><path d="M9 18v-2a7 7 0 1 1 6 0v2Z" fill="#e9bd55"/>',
    c.MODE_PORTAL_JUMP: '<ellipse cx="6" cy="17" rx="4" ry="2" stroke="#60a5fa"/><ellipse cx="19" cy="9" rx="2" ry="6" stroke="#fb923c"/><path d="M6 14V5m-3 3 3-3 3 3m5 4h8m-3-3 3 3-3 3"/>',
    c.MODE_JUMP_REACH: '<path d="M2 21h5m10 0h5M4 17C4 0 20 0 20 17m-4-4 4 4 3-5" stroke="#60b89d"/>',
    c.MODE_RUN_TIME: '<circle cx="12" cy="14" r="8"/><path d="M12 2v4m-3-4h6m3 5 2-2M12 9v5l4 2" stroke="#67b8ef"/>',
}


def tool_icon(mode: str) -> QIcon:
    base = MODE_TO_TOOL[mode]
    body = _ICONS[base]
    if mode in ERASE_TOOLS:
        body += '<circle cx="18" cy="18" r="6" fill="#ad443e" stroke="none"/><path d="M15 18h6" stroke="white"/>'
    elif mode == c.MODE_ERASE_KEEP_FLOORS:
        body += '<path d="M2 22h20" stroke="#60b89d" stroke-width="3"/>'
    outline = QApplication.palette().color(QPalette.ColorRole.WindowText).name()
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="-1 -1 26 26"><g fill="none" stroke="{outline}" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round">{body}</g></svg>'
    renderer = QSvgRenderer(QByteArray(svg.encode()))
    icon = QIcon()
    for scale in (1, 2, 3):
        pixmap = QPixmap(24 * scale, 24 * scale)
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        renderer.render(painter)
        painter.end()
        pixmap.setDevicePixelRatio(scale)
        icon.addPixmap(pixmap)
    return icon
