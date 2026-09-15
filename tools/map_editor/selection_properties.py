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
from .compact_widgets import CompactComboBox
from .constants import FACES
from .display import color_icon, portal_label
from .transforms import record_rect
from .tool_catalog import TOOLS
from .normalization import normalize_map
from .property_fields import fields_for, property_value
from .spawn_counts import actor_count_error

_MIXED = object()


class SelectionProperties(QDockWidget):
    def __init__(self, window):
        super().__init__("Properties", window)
        self.window = window
        self.setObjectName("selection_properties")
        self.setFeatures(QDockWidget.DockWidgetFeature.NoDockWidgetFeatures)
        self.setAllowedAreas(Qt.DockWidgetArea.RightDockWidgetArea)
        self.refs = []
        self.signature = None
        self.widgets = {}
        self.fields = {}
        self.changed_keys = set()
        self.applying = False
        body = QWidget()
        layout = QVBoxLayout(body)
        layout.setContentsMargins(5, 4, 5, 4)
        layout.setSpacing(4)
        self.summary = QLabel("Select an object or drag a selection.")
        self.summary.setWordWrap(True)
        layout.addWidget(self.summary)
        self.group = CompactComboBox()
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
        self.setWidget(body)
        self.setMinimumWidth(170)
        self.set_selection([])

    def set_selection(self, refs):
        if self.applying:
            return
        signature = (id(self.window.map_data), tuple(refs))
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
                self.group.addItem(f"{TOOLS[ELEMENT_MODES[name]].label} ({count})", name)
        self.group.setCurrentIndex(max(0, self.group.findData(previous)))
        self.group.blockSignals(False)
        self.group.setVisible(self.group.count() > 2)
        self.rebuild()

    def targets(self):
        name = self.group.currentData()
        return [ref for ref in self.refs if name is None or ref.name == name]

    def rebuild(self, *_):
        refs = self.targets()
        self.summary.setText(f"{len(refs)} selected" if refs else "Select an object or drag a selection.")
        self.error.hide()
        self.changed_keys.clear()
        self.widgets.clear()
        self.fields.clear()
        self.apply_button.setEnabled(False)
        form_body = QWidget()
        form = QFormLayout(form_body)
        form.setContentsMargins(0, 0, 0, 0)
        form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.ExpandingFieldsGrow)
        form.setRowWrapPolicy(QFormLayout.RowWrapPolicy.WrapLongRows)
        form.setHorizontalSpacing(5)
        form.setVerticalSpacing(4)
        names = list(dict.fromkeys(ref.name for ref in refs))
        fields = fields_for(self.window, names[0]) if names else []
        for name in names[1:]:
            compatible = {field.key: field for field in fields_for(self.window, name)}
            fields = [field for field in fields if compatible.get(field.key) == field]
        for field in fields:
            values = [property_value(ref.get(self.window.map_data), field.key) for ref in refs]
            value = values[0] if all(value == values[0] for value in values) else _MIXED
            if field.kind == "choice":
                widget = CompactComboBox()
                if value is _MIXED:
                    widget.addItem("Mixed / unchanged", _MIXED)
                colors = dict(field.colors)
                for choice, label in field.choices:
                    caption = "None" if choice is None else label
                    if field.key[0] in FACES and choice in self.window.texture_catalog:
                        caption = f"{label} — {portal_label(self.window.texture_catalog[choice])}"
                    widget.addItem(color_icon(colors.get(choice)), caption, choice)
                    widget.setItemData(widget.count() - 1, caption, Qt.ItemDataRole.ToolTipRole)
                    widget.setItemData(
                        widget.count() - 1, "None" if choice is None else label, Qt.ItemDataRole.UserRole + 1
                    )
                index = widget.findData(value)
                if index < 0:
                    widget.addItem(str(value), value)
                    index = widget.count() - 1
                widget.setCurrentIndex(index)
                widget.refresh_tooltip()
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
            if field.key == ("count",):
                widget.setToolTip("Counts for one, two, three, etc. players. The last count repeats.")
            elif field.kind == "respawn":
                widget.setToolTip("Seconds before refilling a slot; Never fills it once.")
            elif field.key[0] in ("from_nudge", "to_nudge"):
                widget.setToolTip(
                    "Floor thicknesses upward" if field.key[1] == 1 else "Wall widths across columns (X) or rows (Z)"
                )
            if field.key == ("kind",) and names == ["actor_spawn_zones"]:
                widget.setEditable(True)
                widget.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
                widget.completer().setFilterMode(Qt.MatchFlag.MatchContains)
                widget.completer().setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
                widget.editTextChanged.connect(lambda _text, key=field.key: self.mark_changed(key))
            widget.setAccessibleName(field.label)
            self.widgets[field.key] = widget
            self.fields[field.key] = field
            form.addRow(field.label, widget)
        face_keys = [field.key for field in fields if field.key[0] in FACES]
        self.material_source = {}
        if face_keys:
            if refs:
                source = min(
                    refs,
                    key=lambda ref: (
                        record_rect(ref.name, ref.get(self.window.map_data))[1],
                        record_rect(ref.name, ref.get(self.window.map_data))[0],
                        ref.level or 0,
                    ),
                )
                self.material_source = {key: property_value(source.get(self.window.map_data), key) for key in face_keys}
            self.source_button = QPushButton("Use top-left materials")
            self.source_button.clicked.connect(self.use_material_source)
            form.addRow(self.source_button)
            self.apply_all_button = QPushButton(f"Apply {face_keys[0][0]} to all faces")
            self.apply_all_button.clicked.connect(self.apply_material_to_all)
            form.addRow(self.apply_all_button)
        if refs and not fields:
            form.addRow(QLabel("Choose an element type." if len(names) > 1 else "No editable properties."))
        previous = self.scroll.takeWidget()
        if previous:
            previous.deleteLater()
        self.scroll.setWidget(form_body)
        self.sync_dependencies()

    def mark_changed(self, key):
        self.changed_keys.add(key)
        self.apply_button.setEnabled(True)
        self.error.hide()
        self.sync_dependencies()

    def value(self, key):
        widget, field = self.widgets[key], self.fields[key]
        if isinstance(widget, QComboBox):
            if widget.isEditable():
                if widget.currentText() == "Mixed / unchanged" and widget.currentData() is _MIXED:
                    return _MIXED
                if widget.currentText() not in self.window.actor_kinds:
                    raise ValueError("Choose an actor kind from the catalog.")
                return widget.currentText()
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
            errors = self.window.added_issues(after)
            if errors:
                raise ValueError(errors[0])
            selection = self.window.selection
            self.applying = True
            applied = self.window.apply_change("Edit Selection Properties", after)
        except ValueError as error:
            self.error.setText(str(error))
            self.error.show()
            return
        finally:
            self.applying = False
        self.signature = None
        if applied and selection.area is None:
            normalized = normalize_map(after)
            selected = [(ref.name, ref.level, ref.get(normalized)) for ref in self.refs]
            refs = [
                ref for ref, entry in element_refs(self.window.map_data) if (ref.name, ref.level, entry) in selected
            ]
            self.window.inspect_refs(refs)
        else:
            self.window.refresh_inspection()

    def sync_dependencies(self):
        names = {ref.name for ref in self.targets()}
        if names == {"items"} and ("kind",) in self.widgets:
            kind = self.widgets[("type",)].currentData()
            self.widgets[("kind",)].setEnabled(kind == "key" or kind is _MIXED)
        if ("switch_inverted",) in self.widgets:
            switch = self.widgets[("switch",)].currentData()
            self.widgets[("switch_inverted",)].setEnabled(switch is not None)

    def set_material_choice(self, key, value):
        box = self.widgets[key]
        index = box.findData(value)
        if index < 0:
            box.addItem(str(value), value)
            index = box.count() - 1
        box.setCurrentIndex(index)
        self.mark_changed(key)

    def use_material_source(self):
        for key, value in self.material_source.items():
            self.set_material_choice(key, value)

    def apply_material_to_all(self):
        keys = [key for key in self.widgets if key[0] in FACES]
        value = self.widgets[keys[0]].currentData()
        if value is _MIXED:
            return
        for key in keys:
            self.set_material_choice(key, value)
