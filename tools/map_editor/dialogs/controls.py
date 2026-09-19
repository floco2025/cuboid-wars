from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QFormLayout,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from ..constants import CHECKPOINT_RESPONSE_LABELS
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


def choice(values, current, *, optional=False, mixed=False, colors=None):
    box = QComboBox()
    if mixed:
        box.addItem("Mixed / leave unchanged", None)
    if optional:
        box.addItem("(none)", "")
    for value in values:
        box.addItem(color_icon((colors or {}).get(value)), value, value)
    wanted = "" if current is None and not mixed else current
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
    def __init__(self, switches, current=None, inverted=False, *, mixed=False, response_mixed=False, colors=None):
        super().__init__()
        self.mixed = mixed
        self.initial = (None if mixed else current or None, None if response_mixed else inverted)
        self.switch = choice(switches, current, optional=True, mixed=mixed, colors=colors)
        self.response = choice(
            ["On", "Off"], None if response_mixed else "Off" if inverted else "On", mixed=response_mixed
        )
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        form.addRow("Switch:", self.switch)
        form.addRow("Respond when:", self.response)
        self.switch.currentIndexChanged.connect(self.sync_enabled)
        self.sync_enabled()

    def sync_enabled(self):
        self.response.setEnabled(self.switch.currentData() != "")

    def state(self):
        """The selected switch, `None` for none, and whether it responds Off."""
        return self.switch.currentData() or None, self.response.currentData() == "Off"

    def values(self):
        """What changed from the initial selection: `switch` None clears the
        assignment, an absent key leaves the records' value alone."""
        switch, inverted = self.state()
        initial_switch, initial_inverted = self.initial
        result = {}
        if self.switch.currentData() == "":
            if self.mixed or initial_switch is not None:
                result["switch"] = None
            return result
        if switch is not None and switch != initial_switch:
            result["switch"] = switch
        if self.response.currentData() is not None and inverted != initial_inverted:
            result["switch_inverted"] = inverted
        return result


class FieldPropertiesDialog(QDialog):
    def __init__(self, parent, title, kinds, switches, entries, *, kind_colors=None, switch_colors=None):
        super().__init__(parent)
        self.setWindowTitle(title)

        def initial(field, default=None):
            values = {entry.get(field, default) for entry in entries}
            return (next(iter(values)), False) if len(values) == 1 else (None, True)

        appearance, mixed = initial("kind")
        self.appearance = choice(kinds, appearance, mixed=mixed, colors=kind_colors)
        switch, mixed = initial("switch")
        inverted, response_mixed = initial("switch_inverted", False)
        self.control = SwitchControl(
            switches, switch, inverted, mixed=mixed, response_mixed=response_mixed, colors=switch_colors
        )
        form = QFormLayout()
        form.addRow("Appearance kind:", self.appearance)
        form.addRow(self.control)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def values(self):
        result = self.control.values()
        if self.appearance.currentData() is not None:
            result["kind"] = self.appearance.currentData()
        return result

    @classmethod
    def prompt(cls, *args, **kwargs):
        dialog = cls(*args, **kwargs)
        return dialog.values() if dialog.exec() == QDialog.DialogCode.Accepted else None
