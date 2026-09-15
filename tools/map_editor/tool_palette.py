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

    def __init__(self, parent):
        super().__init__("Tools", parent)
        self.setObjectName("tool_palette")
        self.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea)
        self.setFeatures(QDockWidget.DockWidgetFeature.NoDockWidgetFeatures)
        self.tool = TOOLS[c.MODE_SELECT]
        self.mode = c.MODE_SELECT
        self.buttons = {}
        self.grids = []
        self.headings = []
        self.icon_only = parent.preferences.value("tools/icons_only", False, type=bool)
        self.button_group = QButtonGroup(self)
        body = QWidget()
        layout = QVBoxLayout(body)
        layout.setContentsMargins(6, 4, 6, 6)
        layout.setSpacing(4)
        body.setStyleSheet(
            "QToolButton[paletteTool=true] { text-align: left; padding: 2px; border: 1px solid transparent; border-radius: 4px; }"
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
        row.setSpacing(2)
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
            heading.setContentsMargins(4, 4, 0, 0)
            self.headings.append(heading)
            groups_layout.addWidget(heading)
            groups_layout.addWidget(self._grid(tools))
        groups_layout.addStretch()
        scroll.setWidget(groups)
        layout.addWidget(scroll, 1)
        self.setWidget(body)
        self.reflow()

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
            button.setIconSize(QSize(20, 20))
            button.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
            button.setCheckable(True)
            button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
            button.clicked.connect(lambda checked=False, mode=tool.mode: self.mode_requested.emit(mode))
            self.button_group.addButton(button)
            self.buttons[tool.mode] = button
            grid.addWidget(button, index // 2, index % 2)
        self.grids.append((grid, list(tools)))
        return widget

    def set_mode(self, mode: str) -> None:
        self.tool = tool_for_mode(mode, self.tool.mode)
        self.mode = mode
        self.button_group.setExclusive(False)
        for button_mode, button in self.buttons.items():
            button.setChecked(button_mode == self.tool.mode)
        self.button_group.setExclusive(True)
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
        modes = tuple(self.buttons)
        current = modes.index(self.tool.mode) if self.tool.mode in modes else 0
        index = (current + direction) % len(modes)
        self.mode_requested.emit(modes[index])

    def set_icon_only(self, enabled):
        self.icon_only = enabled
        self.parent().preferences.setValue("tools/icons_only", enabled)
        self.parent().panel_layout.reflow_tools()

    def reflow(self):
        columns = 2 if self.icon_only else 1
        self.keep_floors.setText("" if self.icon_only else "Keep floors")
        self.place_button.setText("+" if self.icon_only else "Place")
        self.place_button.setToolTip("Place")
        self.erase_button.setText("−" if self.icon_only else "Erase")
        for heading in self.headings:
            heading.setVisible(not self.icon_only)
        for grid, tools in self.grids:
            for tool in tools:
                grid.removeWidget(self.buttons[tool.mode])
            for index, tool in enumerate(tools):
                button = self.buttons[tool.mode]
                button.setToolButtonStyle(
                    Qt.ToolButtonStyle.ToolButtonIconOnly
                    if self.icon_only
                    else Qt.ToolButtonStyle.ToolButtonTextBesideIcon
                )
                grid.addWidget(button, index // columns, index % columns)
            grid.setColumnStretch(0, 1)
            grid.setColumnStretch(1, int(self.icon_only))
        # The body's layout keeps its children's previous minimums until they
        # announce a change, and the dock's minimum follows the body's, so
        # both are refreshed here rather than by Qt's later layout pass.
        for grid, _ in self.grids:
            grid.parentWidget().updateGeometry()
        self.operations.updateGeometry()
        self.widget().layout().activate()
        self.setMinimumWidth(80 if self.icon_only else 180)
