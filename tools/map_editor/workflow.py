"""Selection inspection, sampling, and protected edits for the editor window."""

import copy

from PySide6.QtCore import QPointF
from PySide6.QtGui import QCursor

from . import constants as c
from .elements import ELEMENT_MODES, filtered_map, refs_for_hit, refs_in_region, restore_excluded
from .nesting import NestedMotion
from .regions import TileRegion
from .transforms import record_lists


class WorkflowMixin:
    def editable_map_data(self):
        return filtered_map(self.map_data, self.element_filters.excluded)

    def visible_map_data(self):
        return filtered_map(self.map_data, self.element_filters.hidden)

    def element_filters_changed(self):
        self.cancel_interaction()
        self.selected_spawn_zone_ref = None
        self.inspected_refs = []
        self.refresh_inspection()
        self.canvas._clear_hover()
        self.canvas.update()
        count = len(self.element_filters.excluded)
        label = f"Elements ({count})" if count else "Elements"
        self.element_filters.toggleViewAction().setText(label)
        self.elements_action.setText(label)

    def protect_change(self, after, *, validate_dependencies=True):
        excluded = self.element_filters.excluded
        if not excluded:
            return after
        if (after["grid_cols"], after["grid_rows"]) != (self.map_data["grid_cols"], self.map_data["grid_rows"]):
            raise ValueError("Show and unlock all element types before resizing the map.")
        protected = restore_excluded(self.map_data, after, excluded)
        if protected != self.map_data and validate_dependencies:
            before = {issue.identity() for issue in self.validate(self.map_data).issues}
            errors = [issue.message for issue in self.validate(protected).issues if issue.identity() not in before]
            if errors:
                raise ValueError("This edit would affect hidden or locked elements. Show and unlock them first.")
        elif protected == self.map_data and after != self.map_data:
            self.notify("The affected element types are hidden or locked.")
        # Both sides canonical: the loaded lists keep their authored order.
        final_lists = dict(record_lists(self.doc.maintain(protected)))
        current_lists = record_lists(self.doc.maintain(self.map_data))
        if any(entries != final_lists.get(key, []) for key, entries in current_lists if key[1] in excluded):
            raise ValueError("This edit would affect hidden or locked elements. Show and unlock them first.")
        return protected

    def selection_region(self):
        if self.tile_selection is None:
            return None
        return TileRegion(self.tile_selection, self.current_level, self.selection_levels)

    def selection_scope_changed(self, levels):
        self.cancel_interaction()
        self.selection_levels = levels
        self.inspected_refs = []
        self.refresh_inspection()
        self.canvas.update()

    def inspect_hit(self, hit):
        self.inspected_refs = refs_for_hit(self.map_data, self.current_level, hit)
        self.refresh_inspection(show=True)

    def refresh_inspection(self, *, show=False):
        if not hasattr(self, "properties_panel"):
            return
        region = self.selection_region()
        refs = self.inspected_refs
        if not refs and region is not None and self.mode == c.MODE_SELECT:
            refs = refs_in_region(self.map_data, region, self.element_filters.hidden)
        self.properties_panel.set_selection(refs)
        self.connections_panel.set_selection(refs)
        if show and refs:
            self.properties_panel.show()
            self.properties_panel.raise_()

    def sample_under_cursor(self):
        point = self.canvas.mapFromGlobal(QCursor.pos())
        if self.canvas.rect().contains(point):
            self.sample_at(self.canvas.grid_position(QPointF(point)))

    def sample_at(self, point):
        hit = self.hit_at(point)
        refs = refs_for_hit(self.map_data, self.current_level, hit)
        if not refs:
            self.notify("Nothing to sample here.")
            return
        ref = refs[0]
        entry = ref.get(self.map_data)
        name = ref.name
        mode = ELEMENT_MODES[name]
        self.sampled_materials = None
        if name in ("floors", "inaccessible_floors", "terrain", "walls", "ramps"):
            faces = c.TERRAIN_FACES if name == "terrain" else c.FACES
            self.sampled_materials = {face: entry.get(face, entry.get("all", "")) for face in faces}
            self.current_material = entry.get("top", entry.get("bottom", entry.get("all", "")))
            if name == "ramps" and self.current_level != entry["lower_level"]:
                mode = c.MODE_RAMP_DOWN
        if name in ("barriers", "light_bridges"):
            prefix = "barrier" if name == "barriers" else "bridge"
            setattr(self, f"recent_{prefix}_kind", entry["kind"])
            setattr(
                self,
                f"recent_{prefix}_controls",
                {key: entry[key] for key in ("switch", "switch_inverted") if key in entry},
            )
        elif name == "actor_spawn_zones":
            for key, attribute, default in (
                ("kind", "recent_actor_spawn_kind", ""),
                ("count", "recent_actor_spawn_count", [1]),
                ("respawn_secs", "recent_actor_spawn_respawn_secs", None),
                ("switch", "recent_actor_spawn_switch", ""),
                ("switch_inverted", "recent_actor_spawn_inverted", False),
                ("levels", "recent_actor_spawn_levels", 1),
                ("roam_distance", "recent_actor_roam_distance", 0.0),
            ):
                setattr(self, attribute, copy.deepcopy(entry.get(key, default)))
        elif name == "player_spawn_zones":
            self.recent_player_spawn_levels = entry.get("levels", 1)
        elif name == "checkpoints":
            self.recent_checkpoint_type = entry["type"]
        elif name == "items":
            self.recent_item_type = entry["type"]
            self.recent_item_key_kind = entry.get("kind")
        elif name == "pressure_plates" and entry.get("switch"):
            self.recent_pressure_plate_switch = entry["switch"]
        elif name == "lights":
            self.recent_light_kind = entry["kind"]
        elif name == "ladders":
            self.recent_ladder_levels = entry["levels"]
        elif name == "nested_maps":
            self.recent_nested_map = NestedMotion.from_entry(entry)
        self.activate_tool(mode)
        self.notify(f"Sampled {mode}")

    def set_placement_material(self, material):
        self.current_material = material
        self.sampled_materials = None
