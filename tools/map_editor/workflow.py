"""Selection inspection and sampling for the editor window."""

import copy

from PySide6.QtCore import QPointF
from PySide6.QtGui import QCursor

from . import constants as c
from .elements import ELEMENT_MODES, refs_for_hit
from .nesting import NestedMotion


class WorkflowMixin:
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
        self.sample_ref(refs[0])

    def sample_ref(self, ref):
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
                ("until_checkpoint", "recent_actor_until_checkpoint", None),
                ("on_checkpoint", "recent_actor_on_checkpoint", "stop"),
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
