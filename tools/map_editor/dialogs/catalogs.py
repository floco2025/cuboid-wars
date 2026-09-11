from __future__ import annotations
from PySide6.QtCore import Qt

from PySide6.QtWidgets import QComboBox, QDialog, QDialogButtonBox, QFormLayout, QMessageBox, QSpinBox, QVBoxLayout

from ..catalogs import load_actor_kinds
from .controls import SwitchControl
from ..constants import ITEM_KEY_TYPE, ITEM_TYPES


class ActorSpawnFieldsDialog(QDialog):
    """Modal dialog with a searchable actor catalog, a count field, and the
    map switch that activates the zone, if any.

    Used both when painting a new actor zone and when editing an existing
    one. Returns (kind, count, switch-or-None, inverted) on accept; None on cancel.
    """

    MAX_COUNT = 9999
    NO_SWITCH = "(none)"

    def __init__(self, parent, kind: str, count: int, switches: list[str], switch: str | None, inverted: bool = False):
        super().__init__(parent)
        self.setWindowTitle("Actor Spawn Zone")

        self._kind_edit = QComboBox()
        self._kind_edit.setEditable(True)
        self._kind_edit.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
        self._kind_edit.addItems(load_actor_kinds())
        self._kind_edit.setCurrentText(kind)
        self._kind_edit.completer().setFilterMode(Qt.MatchFlag.MatchContains)
        self._kind_edit.completer().setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        self._count_spin = QSpinBox()
        self._count_spin.setRange(0, self.MAX_COUNT)
        self._count_spin.setValue(count)
        self.control = SwitchControl(switches, switch, inverted)
        self._switch_combo = self.control.kind

        form = QFormLayout()
        form.addRow("Kind:", self._kind_edit)
        form.addRow("Count:", self._count_spin)
        form.addRow(self.control)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def values(self) -> tuple[str, int, str | None, bool]:
        control = self.control.values()
        return (
            self._kind_edit.currentText().strip(),
            self._count_spin.value(),
            control["switch"],
            control["switch_inverted"],
        )

    @classmethod
    def prompt(
        cls, parent, kind: str, count: int, switches: list[str], switch: str | None, inverted: bool = False
    ) -> tuple[str, int, str | None, bool] | None:
        dialog = cls(parent, kind, count, switches, switch, inverted)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        new_kind, new_count, new_switch, inverted = dialog.values()
        if new_kind not in load_actor_kinds():
            QMessageBox.warning(parent, "Actor Spawn Zone", "Choose an actor kind from the catalog.")
            return None
        return new_kind, new_count, new_switch, inverted


class KindDialog(QDialog):
    """Modal dialog asking which id to use from one of the map's catalogs
    (barrier kinds, bridge kinds, or pressure plate kinds).
    `noun` names one entry of that catalog in the empty-catalog warning.
    Returns the chosen id string on accept, None on cancel."""

    def __init__(self, parent, title: str, kinds: list[str], current: str | None, noun: str = "kind"):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._combo = QComboBox()
        for id_ in kinds:
            self._combo.addItem(id_)
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

    # `noun` is the catalog entry ("barrier kind", "switch"), pluralized with an s.
    @classmethod
    def prompt(cls, parent, title: str, kinds: list[str], current: str | None, noun: str) -> str | None:
        if not kinds:
            QMessageBox.warning(
                parent,
                title,
                f"This map lists no {noun}s; add them in the Map menu first.",
            )
            return None
        dialog = cls(parent, title, kinds, current, noun)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.value()


class ItemTypeDialog(QDialog):
    """Modal dialog asking which item type to place. Key items additionally
    pick a barrier kind; the kind combo is disabled for every other type.
    Returns (type, kind-or-None) on accept, None on cancel."""

    def __init__(self, parent, title: str, kinds: list[str], current_type: str | None, current_kind: str | None):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._type_combo = QComboBox()
        for item_type in ITEM_TYPES:
            self._type_combo.addItem(item_type)
        if current_type and current_type in ITEM_TYPES:
            self._type_combo.setCurrentIndex(ITEM_TYPES.index(current_type))

        self._kind_combo = QComboBox()
        for id_ in kinds:
            self._kind_combo.addItem(id_)
        if current_kind and current_kind in kinds:
            self._kind_combo.setCurrentIndex(kinds.index(current_kind))
        self._type_combo.currentTextChanged.connect(self._update_kind_enabled)
        self._update_kind_enabled(self._type_combo.currentText())

        form = QFormLayout()
        form.addRow("Type:", self._type_combo)
        form.addRow("Key kind:", self._kind_combo)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def _update_kind_enabled(self, item_type: str) -> None:
        self._kind_combo.setEnabled(item_type == ITEM_KEY_TYPE)

    def values(self) -> tuple[str, str | None]:
        item_type = self._type_combo.currentText()
        kind = self._kind_combo.currentText() if item_type == ITEM_KEY_TYPE else None
        return item_type, kind

    @classmethod
    def prompt(
        cls, parent, title: str, kinds: list[str], current_type: str | None, current_kind: str | None
    ) -> tuple[str, str | None] | None:
        dialog = cls(parent, title, kinds, current_type, current_kind)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        item_type, kind = dialog.values()
        if item_type == ITEM_KEY_TYPE and not kind:
            QMessageBox.warning(
                parent,
                title,
                "This map lists no barrier kinds. Add them in the Map menu first.",
            )
            return None
        return item_type, kind
