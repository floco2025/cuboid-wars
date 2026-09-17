from __future__ import annotations

from PySide6.QtWidgets import (
    QButtonGroup,
    QCheckBox,
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
    canvas (top-left, center, etc.), or trim empty borders to the content
    bounds. Returns (new_cols, new_rows, column_offset, row_offset) on
    accept; None on cancel.
    """

    MIN_DIM = 1
    MAX_DIM = 256

    def __init__(self, parent, current_cols: int, current_rows: int, *, content_bounds=None):
        super().__init__(parent)
        self.setWindowTitle("Resize Map")
        self.current_size = current_cols, current_rows
        self.content_bounds = content_bounds
        self.manual_size = self.current_size

        self._cols_spin = QSpinBox()
        self._cols_spin.setRange(self.MIN_DIM, self.MAX_DIM)
        self._cols_spin.setValue(current_cols)
        self._rows_spin = QSpinBox()
        self._rows_spin.setRange(self.MIN_DIM, self.MAX_DIM)
        self._rows_spin.setValue(current_rows)

        form = QFormLayout()
        form.addRow("Current size:", QLabel(f"{current_cols} × {current_rows}"))
        self.shrink = QCheckBox("Shrink to fit", self)
        self.shrink.setToolTip("Trim empty rows and columns around the contents on all levels.")
        if content_bounds is not None:
            form.addRow(self.shrink)
        else:
            self.shrink.hide()
        form.addRow("New columns:", self._cols_spin)
        form.addRow("New rows:", self._rows_spin)

        self.anchor_box = QGroupBox("Anchor (where existing content stays)")
        anchor_grid = QGridLayout(self.anchor_box)
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
        self.shrink.toggled.connect(self.set_shrink_to_fit)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(self.anchor_box)
        layout.addWidget(buttons)

    def set_shrink_to_fit(self, enabled):
        if enabled:
            self.manual_size = self._cols_spin.value(), self._rows_spin.value()
            left, top, right, bottom = self.content_bounds
            cols, rows = right - left, bottom - top
        else:
            cols, rows = self.manual_size
        self._cols_spin.setValue(cols)
        self._rows_spin.setValue(rows)
        self._cols_spin.setEnabled(not enabled)
        self._rows_spin.setEnabled(not enabled)
        self.anchor_box.setEnabled(not enabled)

    def values(self) -> tuple[int, int, int, int]:
        cols, rows = self._cols_spin.value(), self._rows_spin.value()
        if self.shrink.isChecked():
            left, top, _, _ = self.content_bounds
            return cols, rows, -left, -top
        anchor_id = self._anchor_group.checkedId()
        anchor_x = anchor_id % 3
        anchor_y = anchor_id // 3
        return cols, rows, (cols - self.current_size[0]) * anchor_x // 2, (rows - self.current_size[1]) * anchor_y // 2

    @classmethod
    def prompt(cls, parent, current_cols: int, current_rows: int, **kwargs) -> tuple[int, int, int, int] | None:
        dialog = cls(parent, current_cols, current_rows, **kwargs)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()
