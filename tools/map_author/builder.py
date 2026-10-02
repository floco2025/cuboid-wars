"""Compose a map from named pieces in grid units; map_core validates it and
the editor's writer saves it, so a build script is the map's source."""

from __future__ import annotations

from dataclasses import dataclass

from map_editor.catalogs import MapCatalogs, load_actor_kinds, load_wall_light_kinds, map_layout_path
from map_editor.constants import FACES, ITEM_KEY_TYPE, ITEM_TYPES
from map_editor.editing import paint_bridges, paint_edges, paint_erasers, paint_floors, place_plate, place_ramp
from map_editor.editing import placement_materials
from map_editor.geometry import ramp_error, ramp_slope
from map_editor.io import write_map
from map_editor.normalization import (
    canonicalize_map,
    edge_key,
    empty_level,
    empty_map,
    item_cell_error,
    normalize_map,
    plate_cell_error,
)
from map_editor.validation import validate_document

from .context import MapContext
from .frame import GridFrame

SIDES = ("N", "S", "E", "W")
# A piece's `side` names the edge of its cell; the portal face on that edge
# looks back into the cell, so a player standing there sees it.
FACE_INTO_CELL = {"N": "south", "S": "north", "E": "west", "W": "east"}
RELATIVE = ("east_of", "west_of", "north_of", "south_of")
FIELD_COLORS = ("#00ccff", "#ffcc00", "#ff5533", "#33dd66", "#cc66ff", "#ff9900")


class BuildError(ValueError):
    pass


# A named footprint later pieces are placed against. For a portal wall the
# footprint is the cells in front of it and `face` the way its portal looks.
@dataclass(frozen=True)
class Piece:
    name: str
    kind: str
    level: int
    c0: int
    r0: int
    c1: int
    r1: int
    face: str | None = None
    levels: int = 1

    @property
    def rect(self) -> tuple[int, int, int, int]:
        return self.c0, self.r0, self.c1, self.r1

    @property
    def size(self) -> tuple[int, int]:
        return self.c1 - self.c0, self.r1 - self.r0

    @property
    def center(self) -> tuple[float, float]:
        return (self.c0 + self.c1) / 2, (self.r0 + self.r1) / 2

    # The level a ramp arrives on.
    @property
    def top_level(self) -> int:
        return self.level + self.levels


class MapBuilder:
    def __init__(self, name: str, cols: int, rows: int, *, levels: int, solid: str = "solid", portal: str = "portal"):
        self.name = name
        self.catalogs = MapCatalogs.load(name)
        textures = self.catalogs.texture_catalog
        if textures.get(portal) is not True:
            raise BuildError(f"{name}: texture alias {portal!r} must exist in settings.json with portalable true")
        if textures.get(solid) is not False:
            raise BuildError(f"{name}: texture alias {solid!r} must exist in settings.json with portalable false")
        if levels < 1:
            raise BuildError("a map needs at least one level")
        self.solid, self.portal = solid, portal
        data = empty_map(cols, rows)
        data["checkpoints"] = []
        data["levels"] = [empty_level(index) for index in range(levels)]
        self.data = {**data, "switches": [], "fields": [], "fireworks": None}
        self.frame = GridFrame.for_map(name, self.data)
        self.pieces: dict[str, Piece] = {}

    @property
    def level_count(self) -> int:
        return len(self.data["levels"])

    def piece(self, name: str) -> Piece:
        try:
            return self.pieces[name]
        except KeyError:
            raise BuildError(f"no piece named {name!r}") from None

    def _register(self, piece: Piece) -> Piece:
        if piece.name in self.pieces:
            raise BuildError(f"piece {piece.name!r} already exists")
        self.pieces[piece.name] = piece
        return piece

    def _level(self, level: int | str, up: int = 0, down: int = 0) -> int:
        index = (self.piece(level).level if isinstance(level, str) else level) + up - down
        if not 0 <= index < self.level_count:
            raise BuildError(f"level {index} is outside the map's {self.level_count} levels")
        return index

    # Where a footprint of `size` goes: `at` its corner, or beside a named
    # piece, `gap` cells away and `shift` cells along the shared edge.
    def _place(self, size, at=None, *, gap: int = 0, shift: int = 0, **relative) -> tuple[int, int, int, int]:
        unknown = set(relative) - set(RELATIVE)
        if unknown:
            raise BuildError(f"unknown placement keyword(s): {', '.join(sorted(unknown))}")
        anchors = {key: value for key, value in relative.items() if value is not None}
        if (at is None) == (not anchors) or len(anchors) > 1:
            raise BuildError("place a piece with at=(col, row) or exactly one of east_of, west_of, north_of, south_of")
        width, height = size
        if width < 1 or height < 1:
            raise BuildError("size must be at least one cell on each axis")
        if at is not None:
            c0, r0 = at
        else:
            ((key, anchor),) = anchors.items()
            a = self.piece(anchor)
            if key == "east_of":
                c0, r0 = a.c1 + gap, a.r0 + shift
            elif key == "west_of":
                c0, r0 = a.c0 - gap - width, a.r0 + shift
            elif key == "south_of":
                c0, r0 = a.c0 + shift, a.r1 + gap
            else:
                c0, r0 = a.c0 + shift, a.r0 - gap - height
        rect = c0, r0, c0 + width, r0 + height
        if not (0 <= c0 and rect[2] <= self.data["grid_cols"] and 0 <= r0 and rect[3] <= self.data["grid_rows"]):
            raise BuildError(f"footprint {rect} is outside the {self.data['grid_cols']}x{self.data['grid_rows']} grid")
        return rect

    def platform(
        self,
        name: str,
        level,
        size,
        at=None,
        *,
        material: str | None = None,
        blocked: bool = False,
        up=0,
        down=0,
        **where,
    ) -> Piece:
        index = self._level(level, up, down)
        rect = self._place(size, at, **where)
        self.data = paint_floors(self.data, index, rect, material or self.solid, blocked=blocked)
        return self._register(Piece(name, "platform", index, *rect))

    # Two portalable cells along the long axis are the least backing a floor portal has.
    def portal_floor(self, name: str, level, size=(2, 2), at=None, *, up=0, down=0, **where) -> Piece:
        if max(size) < 2:
            raise BuildError("a floor portal needs two cells along its long axis")
        index = self._level(level, up, down)
        rect = self._place(size, at, **where)
        self.data = paint_floors(self.data, index, rect, self.portal)
        return self._register(Piece(name, "portal_floor", index, *rect))

    def wall(self, level, start, end, *, material: str | None = None, storeys: int = 1) -> None:
        index = self._level(level)
        if index + storeys > self.level_count:
            raise BuildError(f"a wall of {storeys} storeys from level {index} leaves the map")
        for storey in range(index, index + storeys):
            self.data = paint_edges(self.data, storey, tuple(start), tuple(end), material=material or self.solid)

    # A wall on `side` of cell (col, row) and the `length` cells after it, two
    # storeys tall so a portal fits, portalable on the face into those cells
    # alone: the back stays solid, so only a player in front can use it.
    def portal_wall(self, name: str, level, at, side: str, *, length: int = 1, up=0, down=0) -> Piece:
        if side not in SIDES:
            raise BuildError(f"side must be one of {', '.join(SIDES)}")
        index = self._level(level, up, down)
        if index + 1 >= self.level_count:
            raise BuildError("a portal wall needs the storey above it: it cannot sit on the top level")
        col, row = at
        if side in "NS":
            line = row if side == "N" else row + 1
            start, end = (col, line), (col + length, line)
            rect = col, row, col + length, row + 1
        else:
            line = col if side == "W" else col + 1
            start, end = (line, row), (line, row + length)
            rect = col, row, col + 1, row + length
        face = FACE_INTO_CELL[side]
        materials = {**dict.fromkeys(FACES, self.solid), face: self.portal}
        for storey in (index, index + 1):
            painted = paint_edges(self.data, storey, start, end, material=materials)
            for wall in painted["levels"][storey]["walls"]:
                if wall[face] != self.portal and self._on_segment(wall, start, end):
                    raise BuildError(f"{name}: level {storey} already has a solid wall on that edge")
            self.data = painted
        return self._register(Piece(name, "portal_wall", index, *rect, face=face))

    @staticmethod
    def _on_segment(wall: dict, start, end) -> bool:
        c0, r0, c1, r1 = edge_key(wall)
        (sc, sr), (ec, er) = start, end
        if sr == er:
            return r0 == r1 == sr and min(sc, ec) <= c0 and c1 <= max(sc, ec)
        return c0 == c1 == sc and min(sr, er) <= r0 and r1 <= max(sr, er)

    def bridge(self, name: str, level, size, at=None, *, field: str, up=0, down=0, **where) -> Piece:
        index = self._level(level, up, down)
        rect = self._place(size, at, **where)
        self.data = paint_bridges(self.data, index, rect, field)
        return self._register(Piece(name, "bridge", index, *rect))

    def barrier(self, level, start, end, *, field: str) -> None:
        self.data = paint_edges(self.data, self._level(level), tuple(start), tuple(end), field=field)

    def eraser(self, level, start, end) -> None:
        self.data = paint_erasers(self.data, self._level(level), tuple(start), tuple(end))

    # `direction` is the side the ramp rises toward; the motor climbs 45° at most.
    def ramp(
        self,
        name: str,
        lower_level,
        size,
        at=None,
        *,
        direction: str,
        levels: int = 1,
        shape: str = "solid",
        material: str | None = None,
        allow_steep: bool = False,
        **where,
    ) -> Piece:
        index = self._level(lower_level)
        c0, r0, c1, r1 = self._place(size, at, **where)
        ramp = {
            "lower_level": index,
            "levels": levels,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "direction": direction,
            "shape": shape,
            **placement_materials(material or self.solid),
        }
        error = ramp_error(ramp, self.data["grid_cols"], self.data["grid_rows"], self.level_count)
        if error:
            raise BuildError(f"{name}: {error}")
        slope = ramp_slope(ramp, self.frame.cell, self.frame.level_height)
        if slope and not slope["climbable"] and not allow_steep:
            raise BuildError(
                f"{name}: {slope['degrees']:.1f}° exceeds the {slope['limit_degrees']:.0f}° a character climbs; "
                "lengthen the run or pass allow_steep=True"
            )
        self.data = place_ramp(self.data, ramp)
        return self._register(Piece(name, "ramp", index, c0, r0, c1, r1, levels=levels))

    # A ladder on `side` of its landing cell, climbed from the cell across that edge.
    def ladder(self, lower_level, landing, side: str, *, levels: int = 1) -> None:
        if side not in SIDES:
            raise BuildError(f"side must be one of {', '.join(SIDES)}")
        index = self._level(lower_level)
        if index + levels >= self.level_count:
            raise BuildError(f"a ladder of {levels} storeys from level {index} needs a level to arrive on")
        col, row = landing
        self.data["ladders"] = [
            *self.data["ladders"],
            {"lower_level": index, "col": col, "row": row, "side": side, "levels": levels},
        ]

    def checkpoint(
        self,
        number: int,
        level,
        size,
        at=None,
        *,
        type: str = "individual",
        name: str | None = None,
        up=0,
        down=0,
        **where,
    ) -> Piece:
        index = self._level(level, up, down)
        c0, r0, c1, r1 = self._place(size, at, **where)
        zone = {"level": index, "cols": [c0, c1], "rows": [r0, r1], "type": type, "number": number}
        self.data["checkpoints"] = [*self.data["checkpoints"], zone]
        return self._register(Piece(name or f"cp{number}", "checkpoint", index, c0, r0, c1, r1))

    def plate(self, level, at, *, switch: str) -> None:
        index = self._level(level)
        col, row = at
        error = plate_cell_error(self.data, index, col, row)
        if error:
            raise BuildError(f"plate at ({col}, {row}): {error}")
        try:
            self.data = place_plate(self.data, {"level": index, "col": col, "row": row, "switch": switch})
        except ValueError as error:
            raise BuildError(f"plate at ({col}, {row}): {error}") from None

    def item(self, type: str, level, at, *, field: str | None = None) -> None:
        if type not in ITEM_TYPES:
            raise BuildError(f"unknown item type {type!r}; one of {', '.join(ITEM_TYPES)}")
        if (type == ITEM_KEY_TYPE) != (field is not None):
            raise BuildError("a key names its field; no other item has one")
        index = self._level(level)
        col, row = at
        error = item_cell_error(self.data, index, col, row)
        if error:
            raise BuildError(f"{type} at ({col}, {row}): {error}")
        item = {"level": index, "col": col, "row": row, "type": type}
        if field is not None:
            item["field"] = field
        self.data["items"] = [
            *(i for i in self.data["items"] if (i["level"], i["col"], i["row"]) != (index, col, row)),
            item,
        ]

    def switch(
        self, id: str, *, activation: str = "toggle", reset: str = "solo", held: str = "any", color: str | None = None
    ) -> None:
        entry = {"id": id, "activation": activation, "reset_on_player_death": reset, "held": held}
        if color is not None:
            entry["color"] = color
        self.data["switches"] = [*self.data["switches"], entry]

    def field(self, id: str, *, switch: str | None = None, initially_on: bool = True, color: str | None = None) -> None:
        entry = {"id": id}
        if switch is not None:
            entry["switch"] = switch
        entry["initially_on"] = initially_on
        entry["color"] = color or FIELD_COLORS[len(self.data["fields"]) % len(FIELD_COLORS)]
        self.data["fields"] = [*self.data["fields"], entry]

    def fireworks(self, switch: str, *, cooldown_secs: float = 0.0) -> None:
        self.data["fireworks"] = {"switch": switch, "cooldown_secs": cooldown_secs}

    def level_name(self, level: int, name: str) -> None:
        self.data["levels"][self._level(level)]["name"] = name

    # The map the game would load, normalized, canonical, and valid, with the validator's warnings.
    def document(self) -> tuple[dict, list[str]]:
        root = canonicalize_map(normalize_map(self.data))
        issues = validate_document(
            root,
            self.catalogs.for_layout(root),
            actor_kinds=load_actor_kinds(),
            wall_light_kinds=load_wall_light_kinds(),
        )
        if issues:
            raise BuildError("\n".join(issues))
        return root, issues.warnings

    def save(self, path=None, *, quiet: bool = False) -> list[str]:
        from .describe import summary

        root, warnings = self.document()
        write_map(path or map_layout_path(self.name), root)
        if not quiet:
            for warning in warnings:
                print(f"warning: {warning}")
            print(summary(MapContext.load(self.name, root)))
        return warnings
