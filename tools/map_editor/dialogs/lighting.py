from __future__ import annotations

from PySide6.QtWidgets import QDialog, QDialogButtonBox, QFormLayout, QLabel, QSpinBox, QVBoxLayout, QWidget


class AutoPlaceLightsDialog(QDialog):
    """Modal dialog for the Map → Auto-Place Lights action. Captures four
    spinboxes (row stride/offset, column stride/offset) and returns them as a
    tuple. Lights are placed by the caller; this class only collects input."""

    def __init__(
        self, parent: QWidget, grid_cols: int, grid_rows: int, initial: tuple[int, int, int, int] = (0, 0, 0, 0)
    ):
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

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
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
    def prompt(
        cls, parent: QWidget, grid_cols: int, grid_rows: int, initial: tuple[int, int, int, int] = (0, 0, 0, 0)
    ) -> tuple[int, int, int, int] | None:
        dialog = cls(parent, grid_cols, grid_rows, initial)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()
