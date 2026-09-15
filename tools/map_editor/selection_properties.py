"""A shared inspector for existing elements, with explicit undoable Apply."""

import copy
import math

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDockWidget,
    QFormLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from .elements import ELEMENT_MODES, element_refs
from .normalization import normalize_map
from .property_fields import fields_for, property_value
from .spawn_counts import actor_count_error

_MIXED = object()


class SelectionProperties(QDockWidget):
    def __init__(self, window):
        super().__init__("Selection properties", window)
        self.window = window
        self.setObjectName("selection_properties")
        self.setFeatures(QDockWidget.DockWidgetFeature.DockWidgetClosable)
        self.setAllowedAreas(Qt.DockWidgetArea.RightDockWidgetArea)
        self.refs = []
        self.signature = None
        self.widgets = {}
        self.fields = {}
        self.changed_keys = set()
        self.applying = False
        body = QWidget()
        layout = QVBoxLayout(body)
        self.summary = QLabel("Select an object or drag a selection.")
        self.summary.setWordWrap(True)
        layout.addWidget(self.summary)
        self.group = QComboBox()
        self.group.setAccessibleName("Selected element type")
        self.group.currentIndexChanged.connect(self.rebuild)
        layout.addWidget(self.group)
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        layout.addWidget(self.scroll)
        self.error = QLabel()
        self.error.setWordWrap(True)
        layout.addWidget(self.error)
        row = QHBoxLayout()
        self.apply_button = QPushButton("Apply")
        self.apply_button.clicked.connect(self.apply)
        row.addWidget(self.apply_button)
        revert = QPushButton("Revert")
        revert.clicked.connect(self.rebuild)
        row.addWidget(revert)
        layout.addLayout(row)
        self.connections_button = QPushButton("Connections")
        self.connections_button.clicked.connect(self.show_connections)
        layout.addWidget(self.connections_button)
        self.setWidget(body)
        self.setMinimumWidth(265)
        self.set_selection([])

    def set_selection(self, refs):
        if self.applying:
            return
        signature = (id(self.window.map_data), tuple(refs), frozenset(self.window.element_filters.excluded))
        if signature == self.signature:
            return
        self.signature = signature
        self.refs = list(refs)
        previous = self.group.currentData()
        self.group.blockSignals(True)
        self.group.clear()
        self.group.addItem("All selected", None)
        for name in ELEMENT_MODES:
            count = sum(ref.name == name for ref in refs)
            if count:
                self.group.addItem(f"{ELEMENT_MODES[name]} ({count})", name)
        self.group.setCurrentIndex(max(0, self.group.findData(previous)))
        self.group.blockSignals(False)
        self.group.setVisible(self.group.count() > 2)
        self.rebuild()

    def targets(self):
        name = self.group.currentData()
        return [ref for ref in self.refs if name is None or ref.name == name]

    def rebuild(self, *_):
        refs = self.targets()
        locked = sum(ref.name in self.window.element_filters.excluded for ref in refs)
        self.summary.setText(
            f"{len(refs)} selected" + (f" · {locked} locked" if locked else "")
            if refs
            else "Select an object or drag a selection."
        )
        self.error.hide()
        self.changed_keys.clear()
        self.widgets.clear()
        self.fields.clear()
        self.apply_button.setEnabled(False)
        form_body = QWidget()
        form = QFormLayout(form_body)
        form.setContentsMargins(0, 0, 0, 0)
        form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.AllNonFixedFieldsGrow)
        names = list(dict.fromkeys(ref.name for ref in refs))
        fields = fields_for(self.window, names[0]) if names else []
        for name in names[1:]:
            compatible = {field.key: field for field in fields_for(self.window, name)}
            fields = [field for field in fields if compatible.get(field.key) == field]
        for field in fields:
            values = [property_value(ref.get(self.window.map_data), field.key) for ref in refs]
            value = values[0] if all(value == values[0] for value in values) else _MIXED
            if field.kind == "choice":
                widget = QComboBox()
                if value is _MIXED:
                    widget.addItem("Mixed / unchanged", _MIXED)
                for choice, label in field.choices:
                    widget.addItem("None" if choice is None else label, choice)
                index = widget.findData(value)
                if index < 0:
                    widget.addItem(str(value), value)
                    index = widget.count() - 1
                widget.setCurrentIndex(index)
                widget.currentIndexChanged.connect(lambda _index, key=field.key: self.mark_changed(key))
            else:
                widget = QLineEdit()
                if value is _MIXED:
                    widget.setPlaceholderText("Mixed / unchanged")
                else:
                    text = (
                        ", ".join(map(str, value))
                        if field.kind == "counts" and isinstance(value, list)
                        else "Never"
                        if value is None
                        else str(value)
                    )
                    widget.setText(text)
                widget.textEdited.connect(lambda _text, key=field.key: self.mark_changed(key))
            widget.setAccessibleName(field.label)
            widget.setEnabled(locked < len(refs))
            self.widgets[field.key] = widget
            self.fields[field.key] = field
            form.addRow(field.label, widget)
        if refs and not fields:
            form.addRow(QLabel("Choose an element type." if len(names) > 1 else "No editable properties."))
        previous = self.scroll.takeWidget()
        if previous:
            previous.deleteLater()
        self.scroll.setWidget(form_body)
        self.connections_button.setVisible(any(ref.get(self.window.map_data).get("switch") for ref in refs))

    def mark_changed(self, key):
        self.changed_keys.add(key)
        self.apply_button.setEnabled(True)
        self.error.hide()

    def value(self, key):
        widget, field = self.widgets[key], self.fields[key]
        if isinstance(widget, QComboBox):
            return widget.currentData()
        text = widget.text().strip()
        if field.kind == "counts":
            try:
                value = [int(part.strip()) for part in text.split(",")]
            except ValueError:
                raise ValueError("Count needs comma-separated whole numbers.") from None
            if error := actor_count_error(value):
                raise ValueError(error)
            return value
        if field.kind == "respawn" and text.casefold() == "never":
            return None
        if field.kind in ("number", "positive", "nonnegative", "positive_int", "respawn"):
            try:
                value = int(text) if field.kind == "positive_int" else float(text)
            except ValueError:
                raise ValueError(f"{field.label}: enter a number.") from None
            if (
                not math.isfinite(value)
                or (field.kind in ("positive", "positive_int") and value <= 0)
                or (field.kind in ("nonnegative", "respawn") and value < 0)
            ):
                raise ValueError(f"{field.label}: value is outside the allowed range.")
            return value
        return text

    def apply(self):
        try:
            values = {key: self.value(key) for key in self.changed_keys}
            after = copy.deepcopy(self.window.map_data)
            for ref in self.targets():
                if ref.name in self.window.element_filters.excluded:
                    continue
                entry = ref.get(after)
                for key, value in values.items():
                    if value is _MIXED:
                        continue
                    if len(key) == 2:
                        entry[key[0]][key[1]] = value
                    elif value is None and key[0] in ("switch", "kind"):
                        entry.pop(key[0], None)
                    else:
                        entry[key[0]] = value
                if not entry.get("switch"):
                    entry.pop("switch_inverted", None)
                if ref.name == "items" and entry["type"] != "key":
                    entry.pop("kind", None)
            before_issues = {issue.identity() for issue in self.window.validate(self.window.map_data).issues}
            errors = [
                issue.message for issue in self.window.validate(after).issues if issue.identity() not in before_issues
            ]
            if errors:
                raise ValueError(errors[0])
            self.applying = True
            applied = self.window.apply_change("Edit Selection Properties", after)
        except ValueError as error:
            self.error.setText(str(error))
            self.error.show()
            return
        finally:
            self.applying = False
        if applied:
            # The document normalizes and reorders what it applies, so the
            # selection is found again by its normalized records.
            normalized = normalize_map(after)
            selected = [(ref.name, ref.level, ref.get(normalized)) for ref in self.refs]
            self.window.inspected_refs = [
                ref for ref, entry in element_refs(self.window.map_data) if (ref.name, ref.level, entry) in selected
            ]
        self.signature = None
        self.window.refresh_inspection()

    def show_connections(self):
        self.window.connections_panel.show()
        self.window.connections_panel.raise_()
