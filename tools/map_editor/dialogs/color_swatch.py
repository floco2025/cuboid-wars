"""A colour cell for the catalog dialogs: a swatch that opens the colour picker."""

from PySide6.QtGui import QColor
from PySide6.QtWidgets import QColorDialog, QHBoxLayout, QSizePolicy, QToolButton, QWidget

from ..catalogs import HEX_COLOR
from ..display import contrasting_text_color


class ColorSwatch(QWidget):
    """Holds `#rrggbb`, an invalid authored value left for validation to
    refuse, or None; an optional swatch offers a clear button."""

    def __init__(self, color=None, *, optional=False):
        super().__init__()
        self.optional = optional
        self.button = QToolButton()
        self.button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self.button.clicked.connect(self.pick)
        self.clear_button = QToolButton()
        self.clear_button.setText("×")
        self.clear_button.setToolTip("Inherit the colour")
        self.clear_button.setAutoRaise(True)
        self.clear_button.clicked.connect(lambda: self.set_color(None))
        layout = QHBoxLayout(self)
        layout.setContentsMargins(2, 1, 2, 1)
        layout.setSpacing(2)
        layout.addWidget(self.button, 1)
        layout.addWidget(self.clear_button)
        self.set_color(color)

    def set_color(self, color):
        self.color = color if isinstance(color, str) and color else None
        if self.color is not None and HEX_COLOR.fullmatch(self.color):
            text = contrasting_text_color(QColor(self.color)).name()
            self.button.setStyleSheet(
                f"QToolButton {{ background-color: {self.color}; color: {text}; border: 1px solid palette(mid);"
                " border-radius: 3px; padding: 2px 6px; }"
            )
        else:
            self.button.setStyleSheet("")
        self.button.setText(self.color or ("Inherit" if self.optional else "Choose…"))
        self.clear_button.setVisible(self.optional and self.color is not None)

    def pick(self):
        current = QColor(self.color) if self.color else QColor()
        chosen = QColorDialog.getColor(current if current.isValid() else QColor("#ffffff"), self, "Choose a colour")
        if chosen.isValid():
            self.set_color(chosen.name())
