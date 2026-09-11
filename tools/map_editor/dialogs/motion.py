from __future__ import annotations

from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QFormLayout,
    QHBoxLayout,
    QMessageBox,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from ..nesting import NestedMotion, Nudge
from .controls import SwitchControl


def _nudge_spin_box(axis: str, value: float) -> QDoubleSpinBox:
    box = QDoubleSpinBox()
    box.setPrefix(f"{axis} ")
    box.setRange(-50.0, 50.0)
    box.setSingleStep(0.01)
    box.setValue(value)
    return box


def _row(widgets: list[QWidget]) -> QWidget:
    row = QWidget()
    layout = QHBoxLayout(row)
    layout.setContentsMargins(0, 0, 0, 0)
    for widget in widgets:
        layout.addWidget(widget)
    return row


class MotionDialog(QDialog):
    """Modal dialog asking which map to nest and how it travels: the level
    of its far end, the time one leg takes, the pause at each end, the phase
    offset of its cycle, each end's nudge, its displacement from the anchor,
    x and z in wall widths and y in floor widths, and the switch that runs
    it, if any. `recent` is the previous answer, remembered for the next
    placement."""

    NO_SWITCH = "(none)"

    def __init__(
        self,
        parent,
        level_count: int,
        current_level: int,
        recent: NestedMotion | None,
        title: str,
        map_names: list[str],
        switches: list[str],
    ):
        super().__init__(parent)
        self.setWindowTitle(title)
        recent_map = recent.map_name if recent else ""
        to_level, travel_secs, pause_secs, phase_secs, from_nudge, to_nudge, switch = (
            (
                recent.to_level,
                recent.travel_secs,
                recent.pause_secs,
                recent.phase_secs,
                recent.from_nudge,
                recent.to_nudge,
                recent.switch,
            )
            if recent
            else (current_level, 2.0, 1.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), None)
        )
        self._map = QComboBox()
        self._map.addItems(map_names)
        if recent_map in map_names:
            self._map.setCurrentText(recent_map)
        self.control = SwitchControl(switches, switch, recent.switch_inverted if recent else False)
        self._switch = self.control.kind

        self._to_level = QSpinBox()
        self._to_level.setRange(0, max(0, level_count - 1))
        self._to_level.setValue(min(max(0, to_level), max(0, level_count - 1)))
        self._travel = QDoubleSpinBox()
        self._travel.setRange(0.1, 120.0)
        self._travel.setSingleStep(0.5)
        self._travel.setValue(travel_secs)
        self._pause = QDoubleSpinBox()
        self._pause.setRange(0.0, 60.0)
        self._pause.setSingleStep(0.5)
        self._pause.setValue(pause_secs)
        self._phase = QDoubleSpinBox()
        self._phase.setRange(0.0, 120.0)
        self._phase.setSingleStep(0.5)
        self._phase.setValue(phase_secs)
        self._from_nudge = [_nudge_spin_box(axis, value) for axis, value in zip("xyz", from_nudge)]
        self._to_nudge = [_nudge_spin_box(axis, value) for axis, value in zip("xyz", to_nudge)]

        form = QFormLayout()
        form.addRow("Map:", self._map)
        form.addRow("To level:", self._to_level)
        form.addRow("Travel time, end to end (s):", self._travel)
        form.addRow("Pause at each end (s):", self._pause)
        form.addRow("Phase offset from the start (s):", self._phase)
        form.addRow("Nudge end 1 (x, z wall widths; y floor widths):", _row(self._from_nudge))
        form.addRow("Nudge end 2 (x, z wall widths; y floor widths):", _row(self._to_nudge))
        form.addRow(self.control)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def motion(self) -> tuple[int, float, float, float, Nudge, Nudge, str | None, bool]:
        switch, inverted = self.control.state()
        return (
            self._to_level.value(),
            self._travel.value(),
            self._pause.value(),
            self._phase.value(),
            tuple(box.value() for box in self._from_nudge),
            tuple(box.value() for box in self._to_nudge),
            switch,
            inverted,
        )

    @classmethod
    def prompt_nested(
        cls,
        parent,
        level_count: int,
        current_level: int,
        recent: NestedMotion | None,
        map_names: list[str],
        switches: list[str],
        title: str = "Place Nested Map",
    ) -> NestedMotion | None:
        if not map_names:
            QMessageBox.warning(parent, title, "Create nested geometry with Edit → New Nested Map first.")
            return None
        dialog = cls(parent, level_count, current_level, recent, title, map_names, switches)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return NestedMotion(dialog._map.currentText(), *dialog.motion())
