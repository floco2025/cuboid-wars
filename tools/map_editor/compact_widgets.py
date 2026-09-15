"""Compact fields whose popup can still show full catalog names."""

from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import QComboBox, QSizePolicy, QStyle, QStyleOptionComboBox, QStylePainter


class CompactComboBox(QComboBox):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
        self.setMinimumContentsLength(12)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.setMinimumWidth(65)
        self.setMaximumWidth(220)
        self.currentIndexChanged.connect(self.refresh_tooltip)

    def minimumSizeHint(self):
        return QSize(65, super().minimumSizeHint().height())

    def refresh_tooltip(self, *_):
        detail = self.currentData(Qt.ItemDataRole.ToolTipRole)
        self.setToolTip(str(detail or self.currentText()))

    def showPopup(self):
        width = max(self.width(), self.view().sizeHintForColumn(0) + 36)
        screen = self.screen()
        if screen:
            width = min(width, screen.availableGeometry().width())
        self.view().setMinimumWidth(width)
        super().showPopup()

    def paintEvent(self, event):
        text = self.currentData(Qt.ItemDataRole.UserRole + 1)
        if text is None or self.isEditable():
            return super().paintEvent(event)
        painter = QStylePainter(self)
        option = QStyleOptionComboBox()
        self.initStyleOption(option)
        option.currentText = str(text)
        painter.drawComplexControl(QStyle.ComplexControl.CC_ComboBox, option)
        painter.drawControl(QStyle.ControlElement.CE_ComboBoxLabel, option)
