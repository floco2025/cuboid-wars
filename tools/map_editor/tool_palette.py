"""Visible tools with stable positions and contextual erasing."""

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtWidgets import (
    QButtonGroup,
    QCheckBox,
    QDockWidget,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QScrollArea,
    QSizePolicy,
    QStackedWidget,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

from . import constants as c
from .tool_catalog import PINNED_TOOLS, TOOL_GROUPS, TOOLS, tool_for_mode
from .tool_icons import tool_icon


class ToolPalette(QDockWidget):
    mode_requested = Signal(str)
    search_requested = Signal()

    def __init__(self, parent):
        super().__init__("Tools", parent)
        self.setObjectName("tool_palette")
        self.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea)
        self.setFeatures(QDockWidget.DockWidgetFeature.DockWidgetClosable)
        self.tool = TOOLS[c.MODE_SELECT]
        self.mode = c.MODE_SELECT
        self.buttons = {}
        self.button_group = QButtonGroup(self)
        body = QWidget()
        layout = QVBoxLayout(body)
        layout.setContentsMargins(6, 4, 6, 6)
        layout.setSpacing(4)
        body.setStyleSheet(
            "QToolButton[paletteTool=true] { text-align: left; padding: 4px; border: 1px solid transparent; border-radius: 4px; }"
            "QToolButton[paletteTool=true]:hover { background: palette(alternate-base); border-color: palette(mid); }"
            "QToolButton[paletteTool=true]:checked { background: palette(highlight); color: palette(highlighted-text); }"
            "QToolButton[paletteTool=true]:focus { border-color: palette(text); }"
        )
        layout.addWidget(self._grid(PINNED_TOOLS))

        # Reserve this row even for tools without an erase operation, so the
        # palette does not shift when choosing a different tool.
        self.operations = QStackedWidget()
        self.operations.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        pair = QWidget()
        row = QHBoxLayout(pair)
        row.setContentsMargins(0, 0, 0, 0)
        self.place_button = QToolButton()
        self.place_button.setText("Place")
        self.erase_button = QToolButton()
        self.erase_button.setText("Erase")
        self.operation_group = QButtonGroup(self)
        for button in (self.place_button, self.erase_button):
            button.setCheckable(True)
            button.setProperty("paletteTool", True)
            button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
            self.operation_group.addButton(button)
            row.addWidget(button)
        self.place_button.clicked.connect(lambda: self.mode_requested.emit(self.tool.mode))
        self.erase_button.clicked.connect(self._erase)
        self.operations.addWidget(pair)
        self.keep_floors = QCheckBox("Keep floors")
        self.keep_floors.setToolTip("Keep floors, light bridges, nested map anchors, and the items and plates on them")
        self.keep_floors.clicked.connect(
            lambda checked: self.mode_requested.emit(c.MODE_ERASE_KEEP_FLOORS if checked else c.MODE_ERASE)
        )
        self.operations.addWidget(self.keep_floors)
        layout.addWidget(self.operations)

        scroll = QScrollArea()
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        groups = QWidget()
        groups_layout = QVBoxLayout(groups)
        groups_layout.setContentsMargins(0, 0, 0, 0)
        groups_layout.setSpacing(4)
        for title, tools in TOOL_GROUPS:
            heading = QLabel(title)
            font = heading.font()
            font.setBold(True)
            heading.setFont(font)
            heading.setContentsMargins(4, 8, 0, 0)
            groups_layout.addWidget(heading)
            groups_layout.addWidget(self._grid(tools))
        groups_layout.addStretch()
        scroll.setWidget(groups)
        layout.addWidget(scroll, 1)
        self.setWidget(body)
        # Include the scroll bar width so labels still fit on small screens.
        self.setMinimumWidth(groups.minimumSizeHint().width() + 32)

        # This button lives in the top toolbar, so tool search and the active
        # mode remain accessible when the palette is closed.
        self.current_button = QToolButton(parent)
        self.current_button.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
        self.current_button.setIconSize(QSize(24, 24))
        self.current_button.setAccessibleName("Current tool; search tools")
        self.current_button.clicked.connect(self.search_requested.emit)
        self.set_mode(c.MODE_SELECT)

    def _grid(self, tools) -> QWidget:
        widget = QWidget()
        widget.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Maximum)
        grid = QGridLayout(widget)
        grid.setContentsMargins(0, 0, 0, 0)
        grid.setSpacing(2)
        for index, tool in enumerate(tools):
            button = QToolButton()
            button.setProperty("paletteTool", True)
            button.setText(tool.label)
            button.setAccessibleName(tool.mode)
            button.setToolTip(tool.mode)
            button.setIcon(tool_icon(tool.mode))
            button.setIconSize(QSize(24, 24))
            button.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
            button.setCheckable(True)
            button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
            button.clicked.connect(lambda checked=False, mode=tool.mode: self.mode_requested.emit(mode))
            self.button_group.addButton(button)
            self.buttons[tool.mode] = button
            grid.addWidget(button, index // 2, index % 2)
        grid.setColumnStretch(0, 1)
        grid.setColumnStretch(1, 1)
        return widget

    def set_mode(self, mode: str) -> None:
        self.tool = tool_for_mode(mode, self.tool.mode)
        self.mode = mode
        button = self.buttons[self.tool.mode]
        button.setChecked(True)
        self.current_button.setText(mode)
        self.current_button.setIcon(tool_icon(mode))
        self.current_button.setToolTip(f"{mode} — Search tools")
        self.operations.setCurrentIndex(1 if self.tool.mode == c.MODE_ERASE else 0)
        self.keep_floors.setChecked(mode == c.MODE_ERASE_KEEP_FLOORS)
        for operation in (self.place_button, self.erase_button):
            operation.setEnabled(self.tool.erase is not None)
        self.operation_group.setExclusive(False)
        self.place_button.setChecked(self.tool.erase is not None and mode == self.tool.mode)
        self.erase_button.setChecked(self.tool.erase is not None and mode == self.tool.erase)
        self.operation_group.setExclusive(True)
        self.erase_button.setToolTip(self.tool.erase or "")

    def _erase(self) -> None:
        if self.tool.erase:
            self.mode_requested.emit(self.tool.erase)

    def toggle_erase(self) -> None:
        if self.tool.erase:
            self.mode_requested.emit(self.tool.mode if self.mode == self.tool.erase else self.tool.erase)

    def step(self, direction: int) -> None:
        modes = tuple(TOOLS)
        index = (modes.index(self.tool.mode) + direction) % len(modes)
        self.mode_requested.emit(modes[index])
