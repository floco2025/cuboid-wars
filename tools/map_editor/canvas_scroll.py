"""Scroll bars for the canvas's grid-to-screen transform."""

from PySide6.QtCore import QPointF, Qt
from PySide6.QtWidgets import QAbstractScrollArea, QFrame


class CanvasScrollArea(QAbstractScrollArea):
    def __init__(self, canvas):
        super().__init__()
        self.canvas = canvas
        self._syncing = False
        self.setFrameShape(QFrame.Shape.NoFrame)
        self.setViewport(canvas)
        canvas.setFocusProxy(None)
        self.setFocusProxy(canvas)
        self.horizontalScrollBar().setAccessibleName("Map horizontal scroll")
        self.verticalScrollBar().setAccessibleName("Map vertical scroll")
        canvas.view_changed.connect(self.sync_scroll_bars)

    def minimumSizeHint(self):
        return self.canvas.minimumSizeHint()

    def sizeHint(self):
        return self.canvas.sizeHint()

    def viewportEvent(self, event):
        # The canvas handles its own paint, input, and resize events.
        return False

    def scrollContentsBy(self, dx, dy):
        if not self._syncing:
            self.canvas.setFocus(Qt.FocusReason.MouseFocusReason)
            self.canvas.pan_by(QPointF(dx, dy))

    def sync_scroll_bars(self):
        if self._syncing:
            return
        self._syncing = True
        try:
            view = self.canvas.viewport
            for bar, offset, cells, size in (
                (self.horizontalScrollBar(), view.offset.x(), self.canvas.window.map_data["grid_cols"], self.canvas.width()),
                (self.verticalScrollBar(), view.offset.y(), self.canvas.window.map_data["grid_rows"], self.canvas.height()),
            ):
                extent = cells * view.cell
                bar.setPageStep(size)
                bar.setSingleStep(40)
                bar.setRange(0, 0 if view.fitted else max(0, round(extent - size)))
                bar.setValue(round(-offset))
        finally:
            self._syncing = False
