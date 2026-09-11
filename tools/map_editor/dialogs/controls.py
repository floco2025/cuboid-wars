from PySide6.QtWidgets import QComboBox, QDialog, QDialogButtonBox, QFormLayout, QVBoxLayout, QWidget


def choice(values, current, *, optional=False, mixed=False):
    box = QComboBox()
    if mixed:
        box.addItem("Mixed / leave unchanged", None)
    if optional:
        box.addItem("(none)", "")
    for value in values:
        box.addItem(value, value)
    wanted = "" if current is None and not mixed else current
    index = box.findData(wanted)
    if index < 0 and wanted:
        box.addItem(f"{wanted} (missing)", wanted)
        index = box.count() - 1
    box.setCurrentIndex(max(0, index))
    return box


class SwitchControl(QWidget):
    def __init__(self, switches, current=None, inverted=False, *, mixed=False, response_mixed=False):
        super().__init__()
        self.kind = choice(switches, current, optional=True, mixed=mixed)
        self.response = choice(
            ["On", "Off"], None if response_mixed else "Off" if inverted else "On", mixed=response_mixed
        )
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        form.addRow("Pressure plate kind:", self.kind)
        form.addRow("Respond when:", self.response)
        self.kind.currentIndexChanged.connect(self.sync_enabled)
        self.sync_enabled()

    def sync_enabled(self):
        self.response.setEnabled(self.kind.currentData() != "")

    def values(self):
        result = {}
        kind, response = self.kind.currentData(), self.response.currentData()
        if kind is not None:
            result["switch"] = kind or None
        if kind == "":
            result["switch_inverted"] = False
        elif response is not None:
            result["switch_inverted"] = response == "Off"
        return result


class FieldPropertiesDialog(QDialog):
    def __init__(self, parent, title, kinds, switches, entries):
        super().__init__(parent)
        self.setWindowTitle(title)

        def initial(field, default=None):
            values = {entry.get(field, default) for entry in entries}
            return (next(iter(values)), False) if len(values) == 1 else (None, True)

        appearance, mixed = initial("kind")
        self.appearance = choice(kinds, appearance, mixed=mixed)
        switch, mixed = initial("switch")
        inverted, response_mixed = initial("switch_inverted", False)
        self.control = SwitchControl(switches, switch, inverted, mixed=mixed, response_mixed=response_mixed)
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
    def prompt(cls, *args):
        dialog = cls(*args)
        return dialog.values() if dialog.exec() == QDialog.DialogCode.Accepted else None
