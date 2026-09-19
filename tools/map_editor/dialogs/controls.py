from PySide6.QtWidgets import QComboBox, QDoubleSpinBox, QFormLayout, QSpinBox, QWidget

from ..constants import CHECKPOINT_RESPONSE_LABELS, INITIAL_STATE, INITIAL_STATE_LABELS
from ..display import color_icon


class RespawnSpinBox(QSpinBox):
    """Seconds before an actor zone refills a killed actor's slot; the minimum
    reads "Never" and stands for `null` in the map file."""

    NEVER = -1
    MAX_SECS = 9999

    def __init__(self, secs):
        super().__init__()
        self.setRange(self.NEVER, self.MAX_SECS)
        self.setSuffix(" s")
        self.setSpecialValueText("Never")
        self.set_secs(secs)

    def set_secs(self, secs) -> None:
        try:
            value = self.NEVER if secs is None else int(round(float(secs)))
        except (TypeError, ValueError):
            value = self.NEVER
        self.setValue(max(self.NEVER, min(self.MAX_SECS, value)))

    def secs(self) -> int | None:
        value = self.value()
        return None if value == self.NEVER else value


class BeamInSpinBox(QDoubleSpinBox):
    """Seconds an actor's ghost shows before it appears; zero pops it in at once."""

    MAX_SECS = 9999.0

    def __init__(self, secs):
        super().__init__()
        self.setRange(0.0, self.MAX_SECS)
        self.setDecimals(1)
        self.setSuffix(" s")
        self.setToolTip("Seconds the ghost shows before the actor appears; 0 pops it in at once.")
        self.set_secs(secs)

    def set_secs(self, secs) -> None:
        try:
            value = float(secs)
        except (TypeError, ValueError):
            value = 0.0
        if value != value:
            value = 0.0
        self.setValue(max(0.0, min(self.MAX_SECS, value)))

    def secs(self) -> float:
        return self.value()


def choice(values, current, *, optional=False, colors=None):
    box = QComboBox()
    if optional:
        box.addItem("(none)", "")
    for value in values:
        box.addItem(color_icon((colors or {}).get(value)), value, value)
    wanted = current or ""
    index = box.findData(wanted)
    if index < 0 and wanted:
        box.addItem(f"{wanted} (missing)", wanted)
        index = box.count() - 1
    box.setCurrentIndex(max(0, index))
    return box


class CourseControl(QWidget):
    """Where on the checkpoint course an actor zone ends: the checkpoint that
    closes it once any player reaches it (Always keeps it open) and what
    happens to its actors then."""

    ALWAYS = 0
    MAX_NUMBER = 999_999

    def __init__(self, until_checkpoint=None, on_checkpoint=None):
        super().__init__()
        self.until = QSpinBox()
        self.until.setRange(self.ALWAYS, self.MAX_NUMBER)
        self.until.setSpecialValueText("Always")
        valid = type(until_checkpoint) is int and until_checkpoint >= 1
        self.until.setValue(min(until_checkpoint, self.MAX_NUMBER) if valid else self.ALWAYS)
        self.response = QComboBox()
        for value, label in CHECKPOINT_RESPONSE_LABELS.items():
            self.response.addItem(label, value)
        self.response.setCurrentIndex(max(0, self.response.findData(on_checkpoint)))
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        form.addRow("Active until checkpoint:", self.until)
        form.addRow("Then:", self.response)
        self.until.valueChanged.connect(self.sync_enabled)
        self.sync_enabled()

    def sync_enabled(self):
        self.response.setEnabled(self.until.value() != self.ALWAYS)

    def state(self):
        """The closing checkpoint, `None` for always, and the response, `None` without one."""
        until = self.until.value() or None
        return until, self.response.currentData() if until else None


class SwitchControl(QWidget):
    """A switch target's controls: its state before any switch input, and
    the switch that flips that state while active, if any."""

    def __init__(self, switches, current=None, initially_on=True):
        super().__init__()
        self.switch = choice(switches, current, optional=True)
        self.initially_on = choice(INITIAL_STATE_LABELS.values(), INITIAL_STATE_LABELS[initially_on is not False])
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        form.addRow("Switch:", self.switch)
        form.addRow(INITIAL_STATE + ":", self.initially_on)

    def state(self):
        """The selected switch, `None` for none, and whether the target starts on."""
        return self.switch.currentData() or None, self.initially_on.currentData() != INITIAL_STATE_LABELS[False]
