"""Window operations that publish a complete selection in one step."""

from . import constants as c
from .elements import element_refs, refs_for_hit, refs_in_region
from .regions import TileRegion
from .selection import Selection


class SelectionMixin:
    @property
    def definitions(self):
        return self.doc.nested_geometry if self.doc else {}

    def selection_refs(self):
        return self.selection.refs(self.map_data)

    def selection_region(self):
        return self.selection.region(self.map_data, self.definitions)

    # The Scope control follows what Select picks; a tool that publishes
    # objects for Properties leaves the user's scope alone.
    def set_selection(self, selection, *, focus=False):
        self.selection = selection
        if not selection.empty and self.mode == c.MODE_SELECT:
            self.selection_kind = "Tiles" if selection.area is not None else "Objects"
        self.refresh_inspection(show=focus)
        self.update_selection_actions()
        self.canvas.update()

    def inspect_refs(self, refs, *, show=False):
        refs = tuple(dict.fromkeys(refs))
        region = Selection(refs).region(self.map_data, self.definitions)
        self.set_selection(Selection(refs, anchor=region.rect[:2] if region else None), focus=show)

    def inspect_hit(self, hit, *, show=False):
        self.inspect_refs(refs_for_hit(self.map_data, self.current_level, hit), show=show)

    def open_properties_for(self, names, matches=lambda entry: True):
        names = (names,) if isinstance(names, str) else names
        refs = [
            ref
            for ref, entry in element_refs(self.map_data)
            if ref.name in names and (ref.level is None or ref.level == self.current_level) and matches(entry)
        ]
        if refs:
            self.inspect_refs(refs, show=True)
        else:
            self.notify("No matching objects selected.")

    def set_tile_selection(self, rect, *, objects=False):
        if rect is None:
            self.set_selection(Selection())
            return
        region = TileRegion(rect, self.current_level, self.selection_levels)
        if objects:
            refs = tuple(refs_in_region(self.map_data, region))
            self.set_selection(Selection(refs, anchor=rect[:2]))
        else:
            self.set_selection(Selection(area=region, anchor=rect[:2]))

    def selection_scope_changed(self, levels):
        self.cancel_interaction()
        self.selection_levels = levels
        if self.selection.area is not None:
            self.set_tile_selection(self.selection.area.rect)

    def selection_kind_changed(self, kind):
        self.cancel_interaction()
        self.selection_kind = kind
        self.set_selection(Selection())

    def clear_selection(self):
        self.cancel_interaction()
        self.set_selection(Selection())

    def escape_selection(self):
        if self.canvas.input.gesture is not None or self.pending_block is not None:
            self.canvas.input.cancel()
            if self.pending_block is not None:
                self.pending_block = None
                self.notify("Pending selection cancelled")
            self.update_selection_actions()
            self.canvas.update()
        else:
            self.clear_selection()

    def cancel_interaction(self):
        if self.pending_block is not None:
            self.notify("Pending selection cancelled")
        self.pending_block = None
        self.canvas.cancel()

    def select_all_tiles(self):
        self.set_mode(c.MODE_SELECT)
        self.set_tile_selection(
            (0, 0, self.map_data["grid_cols"], self.map_data["grid_rows"]), objects=self.selection_kind == "Objects"
        )

    def refresh_inspection(self, *, show=False):
        if not hasattr(self, "properties_panel"):
            return
        self.properties_panel.flush(self.selection_refs())
        refs = self.selection_refs()
        self.properties_panel.set_selection(refs)
        self.connection_overlay.set_selection(refs)
        if show and refs:
            field = next(iter(self.properties_panel.widgets.values()), None)
            if field is not None:
                field.setFocus()
