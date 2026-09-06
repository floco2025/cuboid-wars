"""No Tool actions: selecting, moving, and resizing what is already on the canvas."""

from __future__ import annotations

from .normalization import nested_map_key


class SelectMixin:
    # === No Tool ===

    def begin_select_press(self, pos, cell_size: float) -> bool:
        """A press in No Tool. Spawn zones take it first
        (`begin_spawn_zone_drag`); otherwise a nested map end square starts
        an end drag, which the canvas tracks by cells, so this returns
        whether to track one."""
        if self.begin_spawn_zone_drag(pos, cell_size):
            return False
        cell = (int(pos.x() // cell_size), int(pos.y() // cell_size))
        return self.nested_map_end_at(cell) is not None

    def update_select_drag(self, pos, cell_size: float) -> None:
        self.update_spawn_zone_edit_drag(pos, cell_size)

    def end_select_drag(self, start_cell: tuple[int, int] | None, end_cell: tuple[int, int] | None) -> None:
        """Commits a spawn zone drag, or moves the nested map end the drag
        started on; a press that did not move leaves everything as it is."""
        if self.spawn_zone_drag is not None:
            self.commit_spawn_zone_edit_drag()
            return
        if start_cell is None or end_cell is None or end_cell == start_cell:
            return
        hit = self.nested_map_end_at(start_cell)
        if hit is not None:
            entry, end = hit
            self.move_nested_map_end(nested_map_key(entry), end, end_cell)
