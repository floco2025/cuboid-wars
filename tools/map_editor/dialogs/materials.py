from __future__ import annotations

from PySide6.QtCore import Qt
from ..textures import portal_label

from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QLabel,
    QMessageBox,
    QToolButton,
    QVBoxLayout,
)


class MaterialAssignmentDialog(QDialog):
    """Modal dialog with one dropdown per face (top/bottom/N/S/E/W).

    `catalog` is the list of material names to choose from, sourced from
    the host map’s `gameplay.json` texture catalog. `initial` provides the starting selection per face;
    uniform faces start with their value, and mixed faces stay unchanged.

    `Apply to all` copies the Top dropdown's value into the other five.
    """

    FACE_LABELS = (
        ("top", "Top"),
        ("bottom", "Bottom"),
        ("north", "North"),
        ("south", "South"),
        ("east", "East"),
        ("west", "West"),
    )

    def __init__(
        self,
        parent,
        title: str,
        scope_summary: str,
        catalog: list[str],
        initial: dict[str, str | None],
        *,
        source: dict[str, str | None] | None = None,
        portalability: dict[str, bool] | None = None,
    ):
        super().__init__(parent)
        self.setWindowTitle(title)
        self._source = dict(source or {})

        self._dropdowns: dict[str, QComboBox] = {}
        form = QFormLayout()
        form.addRow("Selection:", QLabel(scope_summary))
        if portalability is not None:
            form.addRow("Portals:", QLabel("Permissions come from the selected texture host."))
        for face, label in self.FACE_LABELS:
            combo = QComboBox()
            combo.addItem("Mixed / leave unchanged", None)
            for alias in catalog:
                status = portal_label(portalability[alias]) if portalability and alias in portalability else ""
                combo.addItem(f"{alias} — {status}" if status else alias, alias)
                combo.setItemData(combo.count() - 1, status, Qt.ItemDataRole.ToolTipRole)
            current = initial.get(face)
            if current is not None and current in catalog:
                combo.setCurrentIndex(combo.findData(current))
            self._dropdowns[face] = combo
            form.addRow(label + ":", combo)

        apply_all_button = QToolButton()
        apply_all_button.setText("Apply Top to all faces")
        apply_all_button.clicked.connect(self._apply_top_to_all)

        self.source_button = QToolButton()
        self.source_button.setText("Use top-left materials")
        self.source_button.setToolTip("Fill all six face fields from the top-left selected element. Apply with OK.")
        self.source_button.setEnabled(source is not None)
        self.source_button.clicked.connect(self._use_source)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(self.source_button)
        layout.addWidget(apply_all_button)
        layout.addWidget(buttons)

    def _apply_top_to_all(self) -> None:
        top_value = self._dropdowns["top"].currentText()
        for face in ("bottom", "north", "south", "east", "west"):
            self._dropdowns[face].setCurrentText(top_value)

    def _use_source(self) -> None:
        for face, combo in self._dropdowns.items():
            value = self._source.get(face)
            index = 0 if value is None else combo.findData(value)
            if index < 0:
                combo.addItem(value, value)
                index = combo.count() - 1
            combo.setCurrentIndex(index)

    def values(self) -> dict[str, str]:
        return {face: combo.currentData() for face, combo in self._dropdowns.items() if combo.currentData() is not None}

    @classmethod
    def prompt(
        cls,
        parent,
        title: str,
        scope_summary: str,
        catalog: list[str],
        initial: dict[str, str | None],
        *,
        source: dict[str, str | None] | None = None,
        portalability: dict[str, bool] | None = None,
    ) -> dict[str, str] | None:
        if not catalog:
            QMessageBox.warning(parent, title, "The selected host has no textures in gameplay.json.")
            return None
        dialog = cls(parent, title, scope_summary, catalog, initial, source=source, portalability=portalability)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()
