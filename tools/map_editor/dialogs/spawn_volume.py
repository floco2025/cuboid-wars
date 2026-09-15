from PySide6.QtWidgets import (
    QComboBox,
    QDoubleSpinBox,
    QFormLayout,
    QSpinBox,
    QWidget,
)


class SpawnVolumeControl(QWidget):
    def __init__(self, names, level, levels, roam_distance=None):
        super().__init__()
        self.first = QComboBox()
        self.first.addItems(names)
        self.first.setCurrentIndex(max(0, min(level, len(names) - 1)))
        self.span = QSpinBox()
        self.span.setRange(1, max(1, len(names) - self.first.currentIndex()))
        self.span.setValue(levels)
        self.first.currentIndexChanged.connect(lambda index: self.span.setMaximum(max(1, len(names) - index)))
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        form.addRow("First level:", self.first)
        form.addRow("Levels:", self.span)
        self.roam = None
        if roam_distance is not None:
            self.roam = QDoubleSpinBox()
            self.roam.setRange(0.0, 10000.0)
            self.roam.setDecimals(2)
            self.roam.setSuffix(" m")
            self.roam.setValue(roam_distance)
            self.roam.setToolTip(
                "Extension beyond the spawn volume in every direction. Zero keeps roaming inside the zone."
            )
            form.addRow("Roam extension:", self.roam)

    def values(self):
        return self.first.currentIndex(), self.span.value(), self.roam.value() if self.roam is not None else 0.0
