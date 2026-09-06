from __future__ import annotations

from PySide6.QtWidgets import (
    QButtonGroup,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QGridLayout,
    QGroupBox,
    QLabel,
    QSpinBox,
    QToolButton,
    QVBoxLayout,
)


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

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
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
