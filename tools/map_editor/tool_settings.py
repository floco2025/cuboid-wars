"""Tool defaults for repeated placement."""

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSizePolicy,
    QSpinBox,
    QWidget,
)

from .constants import (
    CHECKPOINT_TYPE_LABELS,
    MODE_CHECKPOINT,
    ITEM_KEY_TYPE,
    ITEM_TYPES,
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
    MODE_WALL,
    RAMP_MODES,
)
from .dialogs import MotionDialog
from .display import portal_label


class ToolSettings(QWidget):
    available_changed = Signal(bool)

    def __init__(self, window):
        super().__init__(window)
        self.window = window
        self.signature = None
        self.bindings = []
        self.body = None
        self.key_controls = None
        self.material_permission = None
        self.row = QHBoxLayout(self)
        self.row.setContentsMargins(8, 0, 0, 0)
        self.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Preferred)

    def sync_values(self) -> None:
        for widget, attribute in self.bindings:
            value = getattr(self.window, attribute)
            widget.blockSignals(True)
            if isinstance(widget, QComboBox):
                if attribute == "recent_checkpoint_type":
                    widget.setCurrentIndex(widget.findData(value))
                else:
                    widget.setCurrentText(value or "")
                widget.setToolTip(widget.currentText())
                if attribute == "current_material":
                    widget.setToolTip(portal_label(self.window.texture_catalog.get(value, False)))
            else:
                widget.setValue(value)
            widget.blockSignals(False)
        if self.material_permission is not None:
            self.material_permission.setText(portal_label(self.window.texture_catalog.get(self.window.current_material, False)))
        if self.key_controls is not None:
            for widget in self.key_controls:
                widget.setVisible(self.window.recent_item_type == ITEM_KEY_TYPE)

    def refresh(self) -> None:
        window = self.window
        signature = (
            window.mode,
            tuple(window.actor_kinds),
            tuple(window.wall_light_kinds),
            tuple(window.barrier_kinds),
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
        self.key_controls = None
        self.material_permission = None
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

        def combo(label, attribute, values, editable=False, required=False):
            box = QComboBox()
            if not required:
                box.addItem("")
            box.addItems(values)
            if attribute == "current_material":
                for index, alias in enumerate(values):
                    box.setItemData(index, portal_label(window.texture_catalog[alias]), Qt.ItemDataRole.ToolTipRole)
            box.setEditable(editable)
            box.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
            box.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
            box.setMinimumContentsLength(9)
            box.setMaximumWidth(150)
            box.setCurrentText(getattr(window, attribute) or "")
            box.setToolTip(box.currentText())
            if editable:
                box.completer().setFilterMode(Qt.MatchFlag.MatchContains)
                box.completer().setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
            box.currentTextChanged.connect(lambda text: setattr(window, attribute, text))
            if attribute == "current_material":
                box.currentTextChanged.connect(lambda alias: box.setToolTip(portal_label(window.texture_catalog.get(alias, False))))
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

        def checkpoint_controls():
            box = QComboBox()
            for kind, label in CHECKPOINT_TYPE_LABELS.items():
                box.addItem(label, kind)
            box.setCurrentIndex(box.findData(window.recent_checkpoint_type))
            box.currentIndexChanged.connect(lambda _: setattr(window, "recent_checkpoint_type", box.currentData()))
            self.bindings.append((box, "recent_checkpoint_type"))
            field("Type", box)

        def item_controls():
            item, _ = combo("Item", "recent_item_type", list(ITEM_TYPES), required=True)
            key, label = combo("Kind", "recent_item_key_kind", window.barrier_kinds)
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
            box.currentTextChanged.connect(lambda alias: permission.setText(portal_label(window.texture_catalog.get(alias, False))))
            form.addWidget(permission)

        def motion_button():
            button = QPushButton("Settings…")
            button.setToolTip("Choose the nested map and its motion")
            button.clicked.connect(self.configure_motion)
            form.addWidget(button)

        # The controls each tool needs, by mode.
        builders = {
            **dict.fromkeys(
                (MODE_FLOOR, MODE_INACCESSIBLE_FLOOR, MODE_WALL, *RAMP_MODES),
                material_controls,
            ),
            MODE_CHECKPOINT: checkpoint_controls,
            MODE_ACTOR_SPAWN_ZONE: lambda: (
                combo("Actor", "recent_actor_spawn_kind", window.actor_kinds, editable=True),
                number("Count", "recent_actor_spawn_count", 0, 9999),
                combo("Switch", "recent_actor_spawn_switch", window.switches),
            ),
            MODE_BARRIER: lambda: combo("Kind", "recent_barrier_kind", window.barrier_kinds),
            MODE_PRESSURE_PLATE: lambda: combo("Switch", "recent_pressure_plate_switch", window.switches),
            MODE_LIGHT_BRIDGE: lambda: combo("Kind", "recent_bridge_kind", window.bridge_kinds),
            MODE_LIGHT: lambda: combo("Style", "recent_light_kind", window.wall_light_kinds, required=True),
            MODE_ITEM: item_controls,
            MODE_LADDER: lambda: number("Storeys", "recent_ladder_levels", 1, max(1, len(window.map_data["levels"]) - 1)),
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
            previous.deleteLater()
        self.available_changed.emit(has_settings)

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
