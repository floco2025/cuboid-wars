"""Modal dialogs used by the map editor."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QButtonGroup,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QGridLayout,
    QGroupBox,
    QLabel,
    QLineEdit,
    QMessageBox,
    QSpinBox,
    QTextBrowser,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

from .constants import ITEM_KEY_TYPE, ITEM_TYPES


class ActorSpawnFieldsDialog(QDialog):
    """Modal dialog with labeled Kind (text) + Count (integer spinbox) fields.

    Used both when painting a new actor zone and when editing an existing
    one. Returns (kind, count) on accept; None on cancel.
    """

    MAX_COUNT = 9999

    def __init__(self, parent, kind: str, count: int):
        super().__init__(parent)
        self.setWindowTitle("Actor Spawn Zone")

        self._kind_edit = QLineEdit(kind)
        self._count_spin = QSpinBox()
        self._count_spin.setRange(0, self.MAX_COUNT)
        self._count_spin.setValue(count)

        form = QFormLayout()
        form.addRow("Kind:", self._kind_edit)
        form.addRow("Count:", self._count_spin)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def values(self) -> tuple[str, int]:
        return self._kind_edit.text().strip(), self._count_spin.value()

    @classmethod
    def prompt(cls, parent, kind: str, count: int) -> tuple[str, int] | None:
        dialog = cls(parent, kind, count)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        new_kind, new_count = dialog.values()
        if not new_kind:
            QMessageBox.warning(parent, "Actor Spawn Zone", "Kind is required.")
            return None
        return new_kind, new_count


class KindDialog(QDialog):
    """Modal dialog asking which kind to use from one of the map's kind
    catalogs (barrier or bridge kinds, from its gameplay settings). `noun`
    names that catalog in the empty-catalog warning. Returns the chosen id
    string on accept, None on cancel."""

    def __init__(self, parent, title: str, kinds: list[str], current: str | None):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._combo = QComboBox()
        for id_ in kinds:
            self._combo.addItem(id_)
        if current and current in kinds:
            self._combo.setCurrentIndex(kinds.index(current))

        form = QFormLayout()
        form.addRow("Kind:", self._combo)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def value(self) -> str:
        return self._combo.currentText()

    @classmethod
    def prompt(cls, parent, title: str, kinds: list[str], current: str | None, noun: str) -> str | None:
        if not kinds:
            QMessageBox.warning(
                parent,
                title,
                f"This map lists no {noun} kinds; add them to its gameplay settings first.",
            )
            return None
        dialog = cls(parent, title, kinds, current)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.value()


class LadderLevelsDialog(QDialog):
    """Modal dialog asking how many storeys a new ladder spans. `max_levels`
    caps the spinbox so the span can never reach past the map's top level.
    Returns the storey count on accept, None on cancel."""

    def __init__(self, parent, max_levels: int, current: int):
        super().__init__(parent)
        self.setWindowTitle("Place Ladder")

        self._spin = QSpinBox()
        self._spin.setRange(1, max_levels)
        self._spin.setValue(min(max(1, current), max_levels))

        form = QFormLayout()
        form.addRow("Storeys:", self._spin)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def value(self) -> int:
        return self._spin.value()

    @classmethod
    def prompt(cls, parent, max_levels: int, current: int) -> int | None:
        dialog = cls(parent, max_levels, current)
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
                "This map lists no barrier kinds. Add them to its gameplay settings first.",
            )
            return None
        return item_type, kind


class ResizeMapDialog(QDialog):
    """Modal dialog to resize the map.

    Lets the user pick new column/row counts and an anchor — a 3x3 grid of
    radio buttons indicating where the existing content stays in the new
    canvas (top-left, center, etc.). Returns (new_cols, new_rows, anchor_x,
    anchor_y) on accept; None on cancel.
    """

    MIN_DIM = 1
    MAX_DIM = 256

    def __init__(self, parent, current_cols: int, current_rows: int):
        super().__init__(parent)
        self.setWindowTitle("Resize Map")

        self._cols_spin = QSpinBox()
        self._cols_spin.setRange(self.MIN_DIM, self.MAX_DIM)
        self._cols_spin.setValue(current_cols)
        self._rows_spin = QSpinBox()
        self._rows_spin.setRange(self.MIN_DIM, self.MAX_DIM)
        self._rows_spin.setValue(current_rows)

        form = QFormLayout()
        form.addRow("Current size:", QLabel(f"{current_cols} × {current_rows}"))
        form.addRow("New columns:", self._cols_spin)
        form.addRow("New rows:", self._rows_spin)

        anchor_box = QGroupBox("Anchor (where existing content stays)")
        anchor_grid = QGridLayout(anchor_box)
        anchor_grid.setSpacing(2)
        self._anchor_group = QButtonGroup(self)
        self._anchor_group.setExclusive(True)
        labels = [
            ["↖", "↑", "↗"],
            ["←", "•", "→"],
            ["↙", "↓", "↘"],
        ]
        for ay in range(3):
            for ax in range(3):
                button = QToolButton()
                button.setCheckable(True)
                button.setText(labels[ay][ax])
                button.setFixedSize(32, 32)
                anchor_id = ay * 3 + ax
                self._anchor_group.addButton(button, anchor_id)
                anchor_grid.addWidget(button, ay, ax)
        self._anchor_group.button(1 * 3 + 1).setChecked(True)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(anchor_box)
        layout.addWidget(buttons)

    def values(self) -> tuple[int, int, int, int]:
        anchor_id = self._anchor_group.checkedId()
        anchor_x = anchor_id % 3
        anchor_y = anchor_id // 3
        return self._cols_spin.value(), self._rows_spin.value(), anchor_x, anchor_y

    @classmethod
    def prompt(cls, parent, current_cols: int, current_rows: int) -> tuple[int, int, int, int] | None:
        dialog = cls(parent, current_cols, current_rows)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()


class MaterialAssignmentDialog(QDialog):
    """Modal dialog with one dropdown per face (top/bottom/N/S/E/W).

    `catalog` is the list of material names to choose from, sourced from
    `assets.json`. `initial` provides the starting selection per face;
    typically the materials of the first selected segment, so opening on a
    uniform region pre-fills with current values.

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

    def __init__(self, parent, title: str, scope_summary: str, catalog: list[str], initial: dict[str, str]):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._dropdowns: dict[str, QComboBox] = {}
        form = QFormLayout()
        form.addRow("Selection:", QLabel(scope_summary))
        for face, label in self.FACE_LABELS:
            combo = QComboBox()
            combo.addItems(catalog)
            current = initial.get(face)
            if current is not None and current in catalog:
                combo.setCurrentText(current)
            self._dropdowns[face] = combo
            form.addRow(label + ":", combo)

        apply_all_button = QToolButton()
        apply_all_button.setText("Apply Top to all faces")
        apply_all_button.clicked.connect(self._apply_top_to_all)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(apply_all_button)
        layout.addWidget(buttons)

    def _apply_top_to_all(self) -> None:
        top_value = self._dropdowns["top"].currentText()
        for face in ("bottom", "north", "south", "east", "west"):
            self._dropdowns[face].setCurrentText(top_value)

    def values(self) -> dict[str, str]:
        return {face: self._dropdowns[face].currentText() for face, _ in self.FACE_LABELS}

    @classmethod
    def prompt(
        cls,
        parent,
        title: str,
        scope_summary: str,
        catalog: list[str],
        initial: dict[str, str],
    ) -> dict[str, str] | None:
        if not catalog:
            QMessageBox.warning(parent, title, "No materials catalog loaded (assets.json missing or empty).")
            return None
        dialog = cls(parent, title, scope_summary, catalog, initial)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()


class AutoPlaceLightsDialog(QDialog):
    """Modal dialog for the Map → Auto-Place Lights action. Captures four
    spinboxes (row stride/offset, column stride/offset) and returns them as a
    tuple. Lights are placed by the caller; this class only collects input."""

    def __init__(self, parent: QWidget, grid_cols: int, grid_rows: int,
                 initial: tuple[int, int, int, int] = (0, 0, 0, 0)):
        super().__init__(parent)
        self.setWindowTitle("Auto-Place Lights")

        init_row_spacing, init_row_offset, init_col_spacing, init_col_offset = initial
        self.row_spacing = QSpinBox()
        self.row_spacing.setRange(0, max(0, grid_rows))
        self.row_spacing.setValue(max(0, min(init_row_spacing, max(0, grid_rows))))
        self.row_offset = QSpinBox()
        self.row_offset.setRange(0, max(0, grid_rows))
        self.row_offset.setValue(max(0, min(init_row_offset, max(0, grid_rows))))
        self.col_spacing = QSpinBox()
        self.col_spacing.setRange(0, max(0, grid_cols))
        self.col_spacing.setValue(max(0, min(init_col_spacing, max(0, grid_cols))))
        self.col_offset = QSpinBox()
        self.col_offset.setRange(0, max(0, grid_cols))
        self.col_offset.setValue(max(0, min(init_col_offset, max(0, grid_cols))))

        form = QFormLayout()
        form.addRow("Row spacing (cells skipped between lights)", self.row_spacing)
        form.addRow("Row offset (starting row)", self.row_offset)
        form.addRow("Column spacing (cells skipped between lights)", self.col_spacing)
        form.addRow("Column offset (starting column)", self.col_offset)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        hint = QLabel(
            "Adds a light on every wall hit by the stride that has a floor on at "
            "least one side. Existing lights are kept (deduplicated)."
        )
        hint.setWordWrap(True)
        layout.addWidget(hint)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def values(self) -> tuple[int, int, int, int]:
        return (
            self.row_spacing.value(),
            self.row_offset.value(),
            self.col_spacing.value(),
            self.col_offset.value(),
        )

    @classmethod
    def prompt(cls, parent: QWidget, grid_cols: int, grid_rows: int,
               initial: tuple[int, int, int, int] = (0, 0, 0, 0)) -> tuple[int, int, int, int] | None:
        dialog = cls(parent, grid_cols, grid_rows, initial)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()


class ToolReferenceDialog(QDialog):
    """Markdown-rendered reference dialog for the editor's tools and
    shortcuts. Content lives in `tool_reference.md` next to this module so it
    can be edited without touching Python. Non-modal so the user can keep it
    open as a side panel while working — re-invoking Help → Tool Reference
    raises the existing window and re-reads the file rather than stacking
    duplicate dialogs."""

    REFERENCE_PATH = Path(__file__).resolve().parent / "tool_reference.md"
    PARENT_ATTR = "_tool_reference_dialog"

    def __init__(self, parent: QWidget):
        super().__init__(parent)
        self.setWindowTitle("Tool Reference")
        self.resize(640, 560)
        # Non-modal so it can sit next to the main window while editing.
        self.setModal(False)
        # Promote to a regular top-level window. The default Qt.Dialog window
        # type drops the resize affordance on some platforms (notably macOS,
        # where it renders as a non-resizable panel). Qt.Window gives it full
        # window-manager decorations including resize handles.
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.Window)
        # Belt-and-braces: explicit resize grip so the corner is reachable
        # even when the title bar's resize chrome is platform-dependent.
        self.setSizeGripEnabled(True)

        self._browser = QTextBrowser(self)
        self._browser.setOpenExternalLinks(False)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        buttons.button(QDialogButtonBox.StandardButton.Close).clicked.connect(self.accept)

        layout = QVBoxLayout(self)
        layout.addWidget(self._browser)
        layout.addWidget(buttons)

        self.reload_content()

    def reload_content(self) -> None:
        try:
            markdown = self.REFERENCE_PATH.read_text(encoding="utf-8")
        except OSError as exc:
            markdown = f"# Tool Reference\n\nFailed to read `{self.REFERENCE_PATH}`:\n\n```\n{exc}\n```"
        self._browser.setMarkdown(markdown)

    @classmethod
    def open_for(cls, parent: QWidget) -> "ToolReferenceDialog":
        # Reuse a single instance per parent — opening Help → Tool Reference
        # while it's already visible should bring the same window forward,
        # not spawn duplicates. Re-read the .md on each invocation so edits
        # are picked up without restarting the editor.
        existing = getattr(parent, cls.PARENT_ATTR, None)
        if existing is None:
            existing = cls(parent)
            setattr(parent, cls.PARENT_ATTR, existing)
        existing.reload_content()
        existing.show()
        existing.raise_()
        existing.activateWindow()
        return existing
