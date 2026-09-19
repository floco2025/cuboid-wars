"""Tool defaults for repeated placement."""

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QSizePolicy,
    QSpinBox,
    QWidget,
)

from .constants import (
    CHECKPOINT_TYPE_LABELS,
    MODE_CHECKPOINT,
    MODE_SELECT,
    ITEM_KEY_TYPE,
    MODE_ACTOR_SPAWN_ZONE,
    MODE_BARRIER,
    MODE_FLOOR,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_LIGHT_BRIDGE,
    MODE_NESTED_MAP,
    MODE_PRESSURE_PLATE,
    MODE_RAMP,
    MODE_WALL,
    RAMP_SHAPE_LABELS,
    START_CHECKPOINT,
)
from .checkpoint_numbers import next_checkpoint_number, used_numbers
from .dialogs import ActorSpawnFieldsDialog, MotionDialog
from .display import color_icon, portal_label
from .compact_widgets import CompactComboBox
from .spawn_counts import actor_count_preview, actor_count_summary


class ToolSettings(QWidget):
    available_changed = Signal(bool)

    def __init__(self, window):
        super().__init__(window)
        self.window = window
        self.signature = None
        self.bindings = []
        self.data_choices = set()
        self.selection_buttons = []
        self.body = None
        self.key_controls = None
        self.material_permission = None
        self.actor_count = None
        self.checkpoint_type = None
        self.row = QHBoxLayout(self)
        self.row.setContentsMargins(8, 0, 0, 0)
        self.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Preferred)

    def sync_values(self) -> None:
        for button, action in self.selection_buttons:
            button.setEnabled(action.isEnabled())
        for widget, attribute in self.bindings:
            value = getattr(self.window, attribute)
            widget.blockSignals(True)
            if isinstance(widget, QComboBox):
                if attribute in self.data_choices:
                    widget.setCurrentIndex(widget.findData(value))
                else:
                    widget.setCurrentText(value or "")
                widget.setToolTip(widget.currentText())
                if attribute == "current_material":
                    widget.setToolTip(portal_label(self.window.texture_catalog.get(value, False)))
            elif isinstance(widget, QLineEdit):
                if widget.text() != value:
                    widget.setText(value)
            else:
                levels_above = len(self.window.map_data["levels"]) - self.window.current_level
                if attribute == "selection_levels":
                    widget.setMaximum(max(1, levels_above))
                elif attribute == "recent_ramp_levels":
                    widget.setMaximum(max(1, levels_above - 1))
                widget.setValue(value)
            widget.blockSignals(False)
        if self.actor_count is not None:
            self.actor_count.setText(actor_count_summary(self.window.recent_actor_spawn_count))
            self.actor_count.setToolTip(actor_count_preview(self.window.recent_actor_spawn_count))
        if self.material_permission is not None:
            self.material_permission.setText(
                portal_label(self.window.texture_catalog.get(self.window.current_material, False))
            )
        if self.key_controls is not None:
            for widget in self.key_controls:
                widget.setVisible(self.window.recent_item_type == ITEM_KEY_TYPE)
        self.sync_checkpoint_type()

    # The start has no type of its own.
    def sync_checkpoint_type(self) -> None:
        if self.checkpoint_type is not None:
            for widget in self.checkpoint_type:
                widget.setEnabled(self.window.recent_checkpoint_number != START_CHECKPOINT)

    def refresh(self) -> None:
        window = self.window
        signature = (
            window.mode,
            window.pickup_types,
            tuple(window.actor_kinds),
            tuple(window.wall_light_kinds),
            tuple(window.barrier_kinds),
            tuple(window.key_kinds),
            tuple(window.bridge_kinds),
            tuple(window.switches),
            tuple(window.materials_catalog),
            tuple(window.texture_catalog.items()),
            len(window.map_data["levels"]),
            window.recent_item_type if window.mode == MODE_ITEM else None,
        )
        if signature == self.signature:
            self.sync_values()
            return
        self.signature = signature
        self.bindings = []
        self.data_choices = set()
        self.selection_buttons = []
        self.key_controls = None
        self.material_permission = None
        self.actor_count = None
        self.checkpoint_type = None
        body = QWidget()
        form = QHBoxLayout(body)
        form.setContentsMargins(0, 0, 0, 0)
        form.setSpacing(6)
        mode = window.mode

        def field(label, box):
            caption = QLabel(label)
            caption.setBuddy(box)
            box.setAccessibleName(label)
            form.addWidget(caption)
            form.addWidget(box)
            return caption

        def combo(label, attribute, values, editable=False, required=False, colors=None):
            box = CompactComboBox()
            if not required:
                box.addItem("")
            for value in values:
                box.addItem(color_icon((colors or {}).get(value)), value)
            if attribute == "current_material":
                for index, alias in enumerate(values):
                    box.setItemData(index, portal_label(window.texture_catalog[alias]), Qt.ItemDataRole.ToolTipRole)
            box.setEditable(editable)
            box.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
            box.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
            box.setMinimumContentsLength(6 if mode == MODE_ACTOR_SPAWN_ZONE else 9)
            box.setMaximumWidth(110 if mode == MODE_ACTOR_SPAWN_ZONE else 150)
            box.setCurrentText(getattr(window, attribute) or "")
            box.setToolTip(box.currentText())
            if editable:
                box.completer().setFilterMode(Qt.MatchFlag.MatchContains)
                box.completer().setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
            if attribute == "current_material":
                box.currentTextChanged.connect(window.set_placement_material)
            else:
                box.currentTextChanged.connect(lambda text: setattr(window, attribute, text))
            if attribute == "current_material":
                box.currentTextChanged.connect(
                    lambda alias: box.setToolTip(portal_label(window.texture_catalog.get(alias, False)))
                )
            else:
                box.currentTextChanged.connect(box.setToolTip)
            self.bindings.append((box, attribute))
            return box, field(label, box)

        def number(label, attribute, minimum, maximum):
            box = QSpinBox()
            box.setRange(minimum, maximum)
            box.setValue(getattr(window, attribute))
            box.setMaximumWidth(75)
            box.valueChanged.connect(lambda value: setattr(window, attribute, value))
            self.bindings.append((box, attribute))
            field(label, box)
            return box

        # A choice stored by value and shown by label.
        def labelled_choice(label, attribute, labels):
            box = CompactComboBox()
            for value, text in labels.items():
                box.addItem(text, value)
            box.setCurrentIndex(box.findData(getattr(window, attribute)))
            box.currentIndexChanged.connect(lambda _: setattr(window, attribute, box.currentData()))
            self.bindings.append((box, attribute))
            self.data_choices.add(attribute)
            return box, field(label, box)

        def ramp_controls():
            material_controls()
            number(
                "Storeys", "recent_ramp_levels", 1, max(1, len(window.map_data["levels"]) - 1 - window.current_level)
            )
            labelled_choice("Shape", "recent_ramp_shape", RAMP_SHAPE_LABELS)

        def checkpoint_controls():
            # The next free number, unless the author typed one the map does
            # not use yet; a start stands, since starts come several at a time.
            recent = window.recent_checkpoint_number
            if recent != START_CHECKPOINT and recent in used_numbers(window.doc.root_data):
                window.recent_checkpoint_number = next_checkpoint_number(window.doc.root_data)
            box = number("Number", "recent_checkpoint_number", START_CHECKPOINT, 999_999)
            self.checkpoint_type = labelled_choice("Type", "recent_checkpoint_type", CHECKPOINT_TYPE_LABELS)
            box.valueChanged.connect(lambda _: self.sync_checkpoint_type())
            self.sync_checkpoint_type()

        def item_controls():
            item, _ = combo("Item", "recent_item_type", list(window.pickup_types), required=True)
            key, label = combo("Kind", "recent_item_key_kind", window.key_kinds, colors=window.barrier_kind_colors)
            self.key_controls = (key, label)

            def show_key_kind(item_type):
                key.setVisible(item_type == ITEM_KEY_TYPE)
                label.setVisible(item_type == ITEM_KEY_TYPE)

            item.currentTextChanged.connect(show_key_kind)
            show_key_kind(window.recent_item_type)

        def material_controls():
            box, _ = combo("Material", "current_material", window.materials_catalog, required=True)
            permission = QLabel(portal_label(window.texture_catalog.get(window.current_material, False)))
            self.material_permission = permission
            box.currentTextChanged.connect(
                lambda alias: permission.setText(portal_label(window.texture_catalog.get(alias, False)))
            )
            form.addWidget(permission)

        def motion_button():
            button = QPushButton("Settings…")
            button.setToolTip("Choose the nested map and its motion")
            button.clicked.connect(self.configure_motion)
            form.addWidget(button)

        def field_controls(barrier):
            prefix = "barrier" if barrier else "bridge"
            combo(
                "Kind",
                f"recent_{prefix}_kind",
                window.barrier_kinds if barrier else window.bridge_kinds,
                colors=window.barrier_kind_colors if barrier else window.bridge_kind_colors,
            )
            button = QPushButton("Controls…")
            button.clicked.connect(lambda: window.configure_field_defaults(barrier))
            form.addWidget(button)

        def actor_controls():
            combo("Actor", "recent_actor_spawn_kind", window.actor_kinds, editable=True)
            self.actor_count = QPushButton(actor_count_summary(window.recent_actor_spawn_count))
            self.actor_count.setToolTip(actor_count_preview(window.recent_actor_spawn_count))
            self.actor_count.setMaximumWidth(130)
            self.actor_count.clicked.connect(self.configure_actor)
            field("Count", self.actor_count)
            # Respawn lives in the dialog: a third inline field overflows the
            # toolbar at the default window width.
            button = QPushButton("Controls…")
            button.clicked.connect(self.configure_actor)
            form.addWidget(button)

        def selection_controls():
            scope = CompactComboBox()
            scope.setMinimumContentsLength(7)
            scope.addItems(["Objects", "Tiles"])
            scope.setCurrentText(window.selection_kind)
            scope.setToolTip("Objects edits only selected objects; Tiles replaces whole areas.")
            scope.currentTextChanged.connect(window.selection_kind_changed)
            field("Scope", scope)
            self.bindings.append((scope, "selection_kind"))
            levels = QSpinBox()
            levels.setRange(1, max(1, len(window.map_data["levels"]) - window.current_level))
            levels.setValue(min(window.selection_levels, levels.maximum()))
            levels.valueChanged.connect(window.selection_scope_changed)
            field("Levels", levels)
            self.bindings.append((levels, "selection_levels"))
            for text, action in (
                ("Duplicate", window.duplicate_action),
                ("Rotate", window.rotate_action),
                ("Mirror", window.mirror_x_action),
            ):
                button = QPushButton(text)
                button.setEnabled(action.isEnabled())
                button.clicked.connect(action.trigger)
                self.selection_buttons.append((button, action))
                form.addWidget(button)

        # The controls each tool needs, by mode.
        builders = {
            MODE_SELECT: selection_controls,
            **dict.fromkeys((MODE_FLOOR, MODE_INACCESSIBLE_FLOOR, MODE_WALL), material_controls),
            MODE_RAMP: ramp_controls,
            MODE_CHECKPOINT: checkpoint_controls,
            MODE_ACTOR_SPAWN_ZONE: actor_controls,
            MODE_BARRIER: lambda: field_controls(True),
            MODE_PRESSURE_PLATE: lambda: combo(
                "Switch", "recent_pressure_plate_switch", window.switches, colors=window.switch_colors
            ),
            MODE_LIGHT_BRIDGE: lambda: field_controls(False),
            MODE_LIGHT: lambda: combo("Style", "recent_light_kind", window.wall_light_kinds, required=True),
            MODE_ITEM: item_controls,
            MODE_LADDER: lambda: number(
                "Storeys", "recent_ladder_levels", 1, max(1, len(window.map_data["levels"]) - 1)
            ),
            MODE_NESTED_MAP: motion_button,
        }
        if mode in builders:
            builders[mode]()
        has_settings = form.count() > 0
        previous = self.body
        self.body = body
        self.row.addWidget(body)
        if previous is not None:
            self.row.removeWidget(previous)
            previous.hide()
            previous.setParent(None)
            previous.deleteLater()
        self.available_changed.emit(has_settings)

    def configure_actor(self) -> None:
        window = self.window
        result = ActorSpawnFieldsDialog.prompt(
            window,
            window.recent_actor_spawn_kind,
            window.recent_actor_spawn_count,
            window.recent_actor_spawn_respawn_secs,
            window.recent_actor_beam_in_secs,
            window.switches,
            window.recent_actor_spawn_switch or None,
            window.recent_actor_spawn_inverted,
            level=window.current_level,
            levels=window.recent_actor_spawn_levels,
            roam_distance=window.recent_actor_roam_distance,
            level_names=[entry.get("name", f"Level {index}") for index, entry in enumerate(window.map_data["levels"])],
            until_checkpoint=window.recent_actor_until_checkpoint,
            on_checkpoint=window.recent_actor_on_checkpoint,
        )
        if result is not None:
            kind, count, respawn_secs, beam_in_secs, switch, inverted, level, levels, roam_distance, until, response = (
                result
            )
            window.recent_actor_spawn_kind = kind
            window.recent_actor_spawn_count = count
            window.recent_actor_spawn_respawn_secs = respawn_secs
            window.recent_actor_beam_in_secs = beam_in_secs
            window.recent_actor_spawn_switch = switch or ""
            window.recent_actor_spawn_inverted = inverted
            window.recent_actor_spawn_levels = levels
            window.recent_actor_roam_distance = roam_distance
            window.recent_actor_until_checkpoint = until
            window.recent_actor_on_checkpoint = response or "stop"
            window.level_combo.setCurrentIndex(level)
            self.refresh()

    def configure_motion(self) -> None:
        window = self.window
        result = MotionDialog.prompt_nested(
            window,
            len(window.map_data["levels"]),
            window.current_level,
            window.recent_nested_map,
            window.nested_map_names(),
            window.switches,
            title="Nested Map Defaults",
        )
        if result is not None:
            window.recent_nested_map = result
            self.signature = None
            self.refresh()
