"""A shared inspector for existing elements; a finished field commits at once."""

import copy
import math

from PySide6.QtCore import QEvent, Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDockWidget,
    QFormLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from .checkpoint_numbers import is_start
from .elements import ELEMENT_MODES, element_refs
from .compact_widgets import CompactComboBox
from .constants import FACES, START_CHECKPOINT, START_CHECKPOINT_TYPE
from .display import color_icon, portal_label
from .geometry import ramp_slope
from .transforms import record_rect
from .tool_catalog import TOOLS
from .nesting import motion_uses_cycle
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
        self.data = None
        self.field_signature = None
        self.widgets = {}
        self.fields = {}
        self.slope_note = None
        self.changed_keys = set()
        self.applying = False
        self.loading = False
        self.visit = None
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
        self.setWidget(body)
        self.setMinimumWidth(170)
        self.set_selection([])

    def set_selection(self, refs):
        if self.applying:
            return
        refs = list(refs)
        if self.shows(refs):
            # A refresh must not replace a draft, its cursor, or its undo
            # history. Updated catalog choices appear once the draft is
            # committed or discarded.
            if self.changed_keys or self.field_signature == self.current_field_signature():
                return
        self.signature = (self.window.path, self.window.doc.active_map, tuple(refs))
        self.data = self.window.map_data
        self.refs = refs
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
        self.visit = object()
        self.rebuild()

    # Whether the panel already shows these records. A committed record is
    # never edited in place, every change installs a new root, so the root
    # the panel was built from vouches for its records, and a new root only
    # needs the selected records compared.
    def shows(self, refs):
        if self.signature != (self.window.path, self.window.doc.active_map, tuple(refs)):
            return False
        data = self.window.map_data
        return data is self.data or all(ref.get(data) == ref.get(self.data) for ref in refs)

    def targets(self):
        name = self.group.currentData()
        return [ref for ref in self.refs if name is None or ref.name == name]

    def current_field_signature(self):
        names = dict.fromkeys(ref.name for ref in self.refs)
        return (
            [fields_for(self.window, name) for name in names],
            dict(self.window.texture_catalog),
            (self.window.grid_cell_size, self.window.level_height),
        )

    def rebuild(self, *_):
        self.field_signature = self.current_field_signature()
        refs = self.targets()
        self.summary.setText(f"{len(refs)} selected" if refs else "Select an object or drag a selection.")
        self.error.hide()
        self.changed_keys.clear()
        self.widgets.clear()
        self.labels = {}
        self.fields.clear()
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
            if field.kind == "choice":
                widget = CompactComboBox()
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
                widget.installEventFilter(self)
                if field.key == ("kind",) and names == ["actor_spawn_zones"]:
                    widget.setEditable(True)
                    widget.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
                    widget.completer().setFilterMode(Qt.MatchFlag.MatchContains)
                    widget.completer().setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
                    widget.editTextChanged.connect(lambda _text, key=field.key: self.mark_changed(key))
                    widget.lineEdit().editingFinished.connect(lambda: self.commit())
                    widget.activated.connect(lambda _index: self.commit())
                else:
                    widget.currentIndexChanged.connect(lambda _index, key=field.key: self.choice_changed(key))
            else:
                widget = QLineEdit()
                widget.textEdited.connect(lambda _text, key=field.key: self.mark_changed(key))
                widget.editingFinished.connect(lambda: self.commit())
            if field.tooltip:
                widget.setToolTip(field.tooltip)
            widget.setAccessibleName(field.label)
            self.widgets[field.key] = widget
            self.fields[field.key] = field
            form.addRow(field.label, widget)
            self.labels[field.key] = form.labelForField(widget)
        face_keys = [field.key for field in fields if field.key[0] in FACES]
        if face_keys:
            self.source_button = QPushButton("Use top-left materials")
            self.source_button.clicked.connect(self.use_material_source)
            form.addRow(self.source_button)
            self.apply_all_button = QPushButton(f"Apply {face_keys[0][0]} to all faces")
            self.apply_all_button.clicked.connect(self.apply_material_to_all)
            form.addRow(self.apply_all_button)
        self.slope_note = None
        if names == ["ramps"]:
            self.slope_note = QLabel()
            self.slope_note.setWordWrap(True)
            form.addRow(self.slope_note)
        if refs and not fields:
            form.addRow(QLabel("Choose an element type." if len(names) > 1 else "No editable properties."))
        previous = self.scroll.takeWidget()
        if previous:
            previous.deleteLater()
        self.scroll.setWidget(form_body)
        self.load_values()

    # Every field without a draft takes the selected records' value, in place
    # so the focused field and an open dropdown survive a commit.
    def load_values(self):
        refs = self.targets()
        self.loading = True
        for key, field in self.fields.items():
            if key in self.changed_keys:
                continue
            widget = self.widgets[key]
            values = [property_value(ref.get(self.window.map_data), key) for ref in refs]
            value = values[0] if all(value == values[0] for value in values) else _MIXED
            if isinstance(widget, QComboBox):
                offers_mixed = widget.count() > 0 and widget.itemData(0) is _MIXED
                if value is _MIXED and not offers_mixed:
                    widget.insertItem(0, "Mixed / unchanged", _MIXED)
                elif value is not _MIXED and offers_mixed:
                    widget.removeItem(0)
                index = widget.findData(value)
                if index < 0:
                    widget.addItem(str(value), value)
                    index = widget.count() - 1
                widget.setCurrentIndex(index)
                widget.refresh_tooltip()
                continue
            if value is _MIXED:
                text = ""
            elif field.kind == "counts" and isinstance(value, list):
                text = ", ".join(map(str, value))
            elif value is None:
                text = {"respawn": "Never", "checkpoint": "Always"}.get(field.kind, "")
            else:
                text = str(value)
            widget.setPlaceholderText("Mixed / unchanged" if value is _MIXED else "")
            # Setting equal text would still drop the cursor and selection.
            if widget.text() != text:
                widget.setText(text)
        self.loading = False
        self.sync_dependencies()
        self.load_slope_note()

    # How steep the steepest selected ramp is, against what the character
    # motor climbs. A note only: a steep ramp is a valid one, so this never
    # joins the errors that hold back a commit.
    def load_slope_note(self):
        if self.slope_note is None:
            return
        cell_size, level_height = self.window.grid_cell_size, self.window.level_height
        slopes = [
            slope
            for ref in self.targets()
            if cell_size and level_height
            if (slope := ramp_slope(ref.get(self.window.map_data), cell_size, level_height)) is not None
        ]
        steepest = max(slopes, key=lambda slope: slope["degrees"], default=None)
        self.slope_note.setVisible(steepest is not None)
        if steepest is None:
            return
        text = f"Slope {steepest['degrees']:.1f}°"
        if not steepest["climbable"]:
            text += f" — too steep to walk up (limit {steepest['limit_degrees']:.0f}°)"
        self.slope_note.setText(text)
        self.slope_note.setStyleSheet("" if steepest["climbable"] else "color: #d97706;")

    def refresh(self):
        if self.field_signature != self.current_field_signature():
            self.rebuild()
        else:
            self.load_values()

    # The wheel scrolls the panel; over a dropdown it would commit a value
    # nobody chose.
    def eventFilter(self, watched, event):
        if event.type() == QEvent.Type.Wheel and isinstance(watched, QComboBox):
            event.ignore()
            return True
        return super().eventFilter(watched, event)

    def keyPressEvent(self, event):
        if event.key() == Qt.Key.Key_Escape and self.changed_keys:
            self.discard()
        else:
            super().keyPressEvent(event)

    def mark_changed(self, key):
        if self.loading:
            return
        self.changed_keys.add(key)
        self.error.hide()
        self.sync_dependencies()

    def choice_changed(self, key):
        self.mark_changed(key)
        self.commit()

    def discard(self):
        self.changed_keys.clear()
        self.error.hide()
        self.refresh()

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
        if field.kind == "optional_text":
            return text or None
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
        if field.kind == "checkpoint":
            if not text or text.casefold() == "always":
                return None
            try:
                number = int(text)
            except ValueError:
                raise ValueError(f"{field.label}: enter a checkpoint number or Always.") from None
            if number < 1:
                raise ValueError(f"{field.label}: checkpoint numbers start at 1.")
            return number
        if field.kind in ("number", "positive", "nonnegative", "positive_int", "nonnegative_int", "respawn"):
            try:
                value = int(text) if field.kind in ("positive_int", "nonnegative_int") else float(text)
            except ValueError:
                raise ValueError(f"{field.label}: enter a number.") from None
            if (
                not math.isfinite(value)
                or (field.kind in ("positive", "positive_int") and value <= 0)
                or (field.kind in ("nonnegative", "nonnegative_int", "respawn") and value < 0)
            ):
                raise ValueError(f"{field.label}: value is outside the allowed range.")
            return value
        return text

    # One undoable edit of every drafted field; whether the document changed.
    # A draft that fails stays in its field, and the next finished field
    # retries them together, so values only valid as a pair land as one edit.
    def commit(self, *, reselect=True):
        if self.loading or self.applying or not self.changed_keys:
            return False
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
                    elif value is None and key[0] in ("switch", "kind", "until_checkpoint"):
                        entry.pop(key[0], None)
                    else:
                        entry[key[0]] = value
                if entry.get("initially_on") is True:
                    del entry["initially_on"]
                if ref.name == "actor_spawn_zones":
                    if entry.get("until_checkpoint") is None:
                        entry.pop("until_checkpoint", None)
                        entry.pop("on_checkpoint", None)
                    else:
                        entry.setdefault("on_checkpoint", "stop")
                if ref.name == "items" and entry["type"] != "key":
                    entry.pop("kind", None)
                if ref.name == "checkpoints" and is_start(entry):
                    entry["type"] = START_CHECKPOINT_TYPE
            errors = self.window.added_issues(after)
            if errors:
                raise ValueError(errors[0])
            self.applying = True
            applied = self.window.apply_change("Edit Selection Properties", after, merge_key=self.visit)
        except ValueError as error:
            self.error.setText(str(error))
            self.error.show()
            return False
        finally:
            self.applying = False
        self.changed_keys.clear()
        if not reselect:
            return applied
        if applied and self.window.selection.area is None:
            normalized = normalize_map(after)
            selected = [(ref.name, ref.level, ref.get(normalized)) for ref in self.refs]
            refs = [
                ref for ref, entry in element_refs(self.window.map_data) if (ref.name, ref.level, entry) in selected
            ]
            self.adopt(refs)
            self.window.inspect_refs(refs)
        elif applied:
            self.adopt(self.window.selection_refs())
            self.window.refresh_inspection()
        self.refresh()
        return applied

    # The committed records become the ones shown, so publishing them again
    # keeps the form; a selection that came back smaller is rebuilt instead.
    def adopt(self, refs):
        if len(refs) != len(self.refs):
            self.signature = None
            return
        self.refs = list(refs)
        self.signature = (self.window.path, self.window.doc.active_map, tuple(refs))
        self.data = self.window.map_data

    # A draft is committed when the selection moves on, so a shortcut that
    # changes it while a field keeps the focus loses nothing. A draft whose
    # records changed underneath it is stale and is left to the rebuild.
    def flush(self, refs):
        if not self.changed_keys or self.applying or list(refs) == self.refs or not self.shows(self.refs):
            return
        self.commit(reselect=False)
        if self.changed_keys:
            self.window.notify(f"Property edit discarded: {self.error.text()}")

    def sync_dependencies(self):
        names = {ref.name for ref in self.targets()}
        if names == {"items"} and ("kind",) in self.widgets:
            kind = self.widgets[("type",)].currentData()
            self.widgets[("kind",)].setEnabled(kind == "key" or kind is _MIXED)
        if ("on_checkpoint",) in self.widgets:
            until = self.widgets[("until_checkpoint",)].text().strip()
            self.widgets[("on_checkpoint",)].setEnabled(bool(until) and until.casefold() != "always")
        if names == {"checkpoints"} and ("type",) in self.widgets:
            number = self.widgets[("number",)].text().strip()
            start = number.isdigit() and int(number) == START_CHECKPOINT
            self.widgets[("type",)].setEnabled(not start)
            self.labels[("type",)].setEnabled(not start)
        if ("motion",) in self.widgets:
            motion = self.widgets[("motion",)].currentData()
            cycle = motion is _MIXED or motion_uses_cycle(motion)
            for key in ("pause_secs", "phase_secs"):
                self.widgets[(key,)].setEnabled(cycle)
                self.labels[(key,)].setEnabled(cycle)

    def set_material_choices(self, choices):
        self.loading = True
        for key, value in choices.items():
            box = self.widgets[key]
            index = box.findData(value)
            if index < 0:
                box.addItem(str(value), value)
                index = box.count() - 1
            box.setCurrentIndex(index)
        self.loading = False
        self.changed_keys.update(choices)
        self.error.hide()
        self.commit()

    def use_material_source(self):
        data = self.window.map_data
        refs = self.targets()
        if not refs:
            return
        source = min(
            refs,
            key=lambda ref: (
                record_rect(ref.name, ref.get(data))[1],
                record_rect(ref.name, ref.get(data))[0],
                ref.level or 0,
            ),
        )
        keys = [key for key in self.widgets if key[0] in FACES]
        self.set_material_choices({key: property_value(source.get(data), key) for key in keys})

    def apply_material_to_all(self):
        keys = [key for key in self.widgets if key[0] in FACES]
        value = self.widgets[keys[0]].currentData()
        if value is _MIXED:
            return
        self.set_material_choices(dict.fromkeys(keys, value))
