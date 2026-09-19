from __future__ import annotations
from PySide6.QtCore import Qt

from PySide6.QtWidgets import QComboBox, QDialog, QDialogButtonBox, QFormLayout, QMessageBox, QVBoxLayout

from ..catalogs import load_actor_kinds
from .controls import BeamInSpinBox, CourseControl, RespawnSpinBox, SwitchControl
from .spawn_volume import SpawnVolumeControl
from .spawn_count import SpawnCountControl
from ..constants import ITEM_KEY_TYPE, ITEM_TYPES
from ..display import color_icon


class ActorSpawnFieldsDialog(QDialog):
    """Modal dialog with a searchable actor catalog, a count field, the
    respawn delay, the beam-in time, whether the zone starts out spawning,
    the map switch that flips that, if any, and the checkpoint that ends it,
    if any.

    Used both when painting a new actor zone and when editing an existing
    one.
    """

    NO_SWITCH = "(none)"

    def __init__(
        self,
        parent,
        kind: str,
        count: list[int],
        respawn_secs: int | None,
        beam_in_secs: float,
        switches: list[str],
        switch: str | None,
        initially_on: bool = True,
        *,
        level=0,
        levels=1,
        roam_distance=0.0,
        level_names=None,
        until_checkpoint=None,
        on_checkpoint=None,
    ):
        super().__init__(parent)
        self.setWindowTitle("Actor Spawn Zone")

        self._kind_edit = QComboBox()
        self._kind_edit.setEditable(True)
        self._kind_edit.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
        self._kind_edit.addItems(load_actor_kinds())
        self._kind_edit.setCurrentText(kind)
        self._kind_edit.completer().setFilterMode(Qt.MatchFlag.MatchContains)
        self._kind_edit.completer().setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        self.count_control = SpawnCountControl(count)
        self._respawn_spin = RespawnSpinBox(respawn_secs)
        self._beam_in_spin = BeamInSpinBox(beam_in_secs)
        self.control = SwitchControl(switches, switch, initially_on)
        self._switch_combo = self.control.switch
        self.volume = SpawnVolumeControl(level_names or ["Level 0"], level, levels, roam_distance)
        self.course = CourseControl(until_checkpoint, on_checkpoint)

        form = QFormLayout()
        form.addRow("Kind:", self._kind_edit)
        form.addRow(self.count_control)
        form.addRow("Respawn:", self._respawn_spin)
        form.addRow("Beam-in:", self._beam_in_spin)
        form.addRow(self.volume)
        form.addRow(self.control)
        form.addRow(self.course)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        self.count_control.validity_changed.connect(buttons.button(QDialogButtonBox.StandardButton.Ok).setEnabled)
        self.count_control.refresh()

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def accept(self):
        try:
            self.count_control.value()
        except ValueError as error:
            QMessageBox.warning(self, "Actor Spawn Zone", str(error))
            return
        super().accept()

    def values(self):
        switch, initially_on = self.control.state()
        return (
            self._kind_edit.currentText().strip(),
            self.count_control.value(),
            self._respawn_spin.secs(),
            self._beam_in_spin.secs(),
            switch,
            initially_on,
            *self.volume.values(),
            *self.course.state(),
        )

    @classmethod
    def prompt(
        cls,
        parent,
        kind: str,
        count: list[int],
        respawn_secs: int | None,
        beam_in_secs: float,
        switches: list[str],
        switch: str | None,
        initially_on: bool = True,
        **volume,
    ):
        dialog = cls(parent, kind, count, respawn_secs, beam_in_secs, switches, switch, initially_on, **volume)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        values = dialog.values()
        if values[0] not in load_actor_kinds():
            QMessageBox.warning(parent, "Actor Spawn Zone", "Choose an actor kind from the catalog.")
            return None
        return values


class KindDialog(QDialog):
    """Modal dialog asking which id to use from one of the map's catalogs
    (fields or switches).
    `noun` names one entry of that catalog in the empty-catalog warning.
    Returns the chosen id string on accept, None on cancel."""

    def __init__(self, parent, title: str, kinds: list[str], current: str | None, noun: str = "kind", colors=None):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._combo = QComboBox()
        for id_ in kinds:
            self._combo.addItem(color_icon((colors or {}).get(id_)), id_)
        if current and current in kinds:
            self._combo.setCurrentIndex(kinds.index(current))

        form = QFormLayout()
        form.addRow(f"{noun.capitalize()}:", self._combo)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def value(self) -> str:
        return self._combo.currentText()

    # `noun` is the catalog entry ("field", "switch"), pluralized with an s.
    @classmethod
    def prompt(cls, parent, title: str, kinds: list[str], current: str | None, noun: str, colors=None) -> str | None:
        if not kinds:
            QMessageBox.warning(
                parent,
                title,
                f"This map lists no {noun}s; add them in the Map menu first.",
            )
            return None
        dialog = cls(parent, title, kinds, current, noun, colors)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.value()


class ItemTypeDialog(QDialog):
    """Modal dialog asking which item type to place. Key items additionally
    pick a field; the field combo is disabled for every other type.
    Returns (type, field-or-None) on accept, None on cancel."""

    def __init__(
        self,
        parent,
        title: str,
        fields: list[str],
        current_type: str | None,
        current_field: str | None,
        colors=None,
        *,
        item_types=ITEM_TYPES,
    ):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._type_combo = QComboBox()
        for item_type in item_types:
            self._type_combo.addItem(item_type)
        if current_type and current_type in item_types:
            self._type_combo.setCurrentIndex(item_types.index(current_type))

        self._field_combo = QComboBox()
        for id_ in fields:
            self._field_combo.addItem(color_icon((colors or {}).get(id_)), id_)
        if current_field and current_field in fields:
            self._field_combo.setCurrentIndex(fields.index(current_field))
        self._type_combo.currentTextChanged.connect(self._update_field_enabled)
        self._update_field_enabled(self._type_combo.currentText())

        form = QFormLayout()
        form.addRow("Type:", self._type_combo)
        form.addRow("Field:", self._field_combo)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def _update_field_enabled(self, item_type: str) -> None:
        self._field_combo.setEnabled(item_type == ITEM_KEY_TYPE)

    def values(self) -> tuple[str, str | None]:
        item_type = self._type_combo.currentText()
        field = self._field_combo.currentText() if item_type == ITEM_KEY_TYPE else None
        return item_type, field

    @classmethod
    def prompt(
        cls,
        parent,
        title: str,
        fields: list[str],
        current_type: str | None,
        current_field: str | None,
        colors=None,
        *,
        item_types=ITEM_TYPES,
    ) -> tuple[str, str | None] | None:
        dialog = cls(parent, title, fields, current_type, current_field, colors, item_types=item_types)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        item_type, field = dialog.values()
        if item_type == ITEM_KEY_TYPE and not field:
            QMessageBox.warning(
                parent,
                title,
                "This map lists no fields. Add them in the Map menu first.",
            )
            return None
        return item_type, field
