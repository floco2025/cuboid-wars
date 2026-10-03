"""Compose a map from named pieces in grid units; map_core validates it and
the editor's writer saves it, so a build script is the map's source."""

from __future__ import annotations

from dataclasses import dataclass
from math import isfinite

from map_editor.catalogs import MapCatalogs, load_actor_kinds, load_wall_light_kinds, map_layout_path
from map_editor.constants import ACTOR_ZONE_LIST, FACES, ITEM_KEY_TYPE, ITEM_TYPES, WALL_LIGHT_HEIGHT_FRACTION
from map_editor.editing import paint_bridges, paint_edges, paint_erasers, paint_floors, place_plate, place_ramp
from map_editor.editing import placement_materials
from map_editor.erasing import lights_off_edges
from map_editor.geometry import ramp_error, ramp_slope
from map_editor.io import write_map
from map_editor.normalization import (
    canonicalize_map,
    edge_key,
    empty_level,
    empty_map,
    expand_face_materials,
    item_cell_error,
    light_placement_error,
    normalize_map,
    plate_cell_error,
)
from map_editor.validation import validate_document

from .context import MapContext
from .edges import FACE_INTO_CELL, SIDES, STEP
from .frame import GridFrame, PortalFootprint

OPPOSITE_FACE = {"north": "south", "south": "north", "east": "west", "west": "east"}
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

    # The level a ramp arrives on, or a room's ceiling stands on.
    @property
    def top_level(self) -> int:
        return self.level + self.levels


class MapBuilder:
    # `solid` takes no portal and `portal` does; `default`, one of the two
    # unless named, is what a surface gets when its piece names no material.
    def __init__(
        self,
        name: str,
        cols: int,
        rows: int,
        *,
        levels: int,
        solid: str = "solid",
        portal: str = "portal",
        default: str | None = None,
    ):
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
        self.default = self._alias(default, solid)
        data = empty_map(cols, rows)
        data["checkpoints"] = []
        data["levels"] = [empty_level(index) for index in range(levels)]
        self.data = {**data, "switches": [], "fields": [], "fireworks": None}
        self.frame = GridFrame.for_map(name, self.data)
        self.footprint = PortalFootprint.for_map(name, self.frame)
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

    def _alias(self, material: str | None, default: str) -> str:
        if material is None:
            return default
        if material not in self.catalogs.texture_catalog:
            known = ", ".join(self.catalogs.texture_catalog)
            raise BuildError(f"texture alias {material!r} is not in settings.json; known: {known}")
        return material

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
        self.data = paint_floors(self.data, index, rect, self._alias(material, self.default), blocked=blocked)
        return self._register(Piece(name, "platform", index, *rect))

    # A pad a floor portal fits on either way round, unless `size` says otherwise.
    def portal_floor(self, name: str, level, size=None, at=None, *, up=0, down=0, **where) -> Piece:
        along, across = self.footprint.along, self.footprint.across
        size = size or (along, along)
        if max(size) < along or min(size) < across:
            raise BuildError(f"{name}: a floor portal needs {along}x{across} cells on this grid")
        index = self._level(level, up, down)
        rect = self._place(size, at, **where)
        self._slab(index, rect, dict.fromkeys(FACES, self.portal), "top")
        return self._register(Piece(name, "portal_floor", index, *rect))

    def wall(self, level, start, end, *, material: str | None = None, storeys: int = 1) -> None:
        index = self._level(level)
        if index + storeys > self.level_count:
            raise BuildError(f"a wall of {storeys} storeys from level {index} leaves the map")
        material = self._alias(material, self.default)
        for storey in range(index, index + storeys):
            self.data = paint_edges(self.data, storey, tuple(start), tuple(end), material=material)

    # The face into cell (col, row) of the wall on its `side`, and of the
    # cells after it, in another material: `length` cells and `storeys` up.
    def face_wall(self, level, at, side: str, material: str, *, length: int = 1, storeys: int = 1) -> None:
        if side not in SIDES:
            raise BuildError(f"side must be one of {', '.join(SIDES)}")
        index = self._level(level)
        col, row = at
        rect = (col, row, col + length, row + 1) if side in "NS" else (col, row, col + 1, row + length)
        start, end = self._side_segment(rect, side)
        face = FACE_INTO_CELL[side]
        material = self._alias(material, self.default)
        for storey in range(index, index + storeys):
            if storey >= self.level_count:
                raise BuildError(f"a face {storeys} storeys from level {index} leaves the map")
            walls = self.data["levels"][storey]["walls"]
            found = [at for at, wall in enumerate(walls) if self._on_segment(wall, start, end)]
            if len(found) != length:
                raise BuildError(f"level {storey}: no wall along all of {start}..{end} to face")
            for at in found:
                walls[at] = self._refaced(walls[at], face, material)

    # The top of the slabs over a footprint, or their underside, in another material.
    def face_slab(self, level, size, at, material: str, *, face: str = "top") -> None:
        if face not in ("top", "bottom"):
            raise BuildError("a slab is faced on its top or its bottom")
        index = self._level(level)
        c0, r0, c1, r1 = self._place(size, at)
        material = self._alias(material, self.default)
        cells = {(col, row) for row in range(r0, r1) for col in range(c0, c1)}
        for name in ("floors", "inaccessible_floors"):
            slabs = self.data["levels"][index][name]
            for at, slab in enumerate(slabs):
                if (slab["col"], slab["row"]) in cells:
                    slabs[at] = self._refaced(slab, face, material)
                    cells.discard((slab["col"], slab["row"]))
        if cells:
            raise BuildError(f"level {index}: no slab under {sorted(cells)[0]} to face")

    # The wall on `side` of cell (col, row) and of the cells after it, as wide
    # and as many sections tall as a portal needs, portalable on the face into
    # those cells alone: a new wall's back is solid, and a wall already there
    # keeps its other faces, so only a player in front can use it.
    def portal_wall(self, name: str, level, at, side: str, *, length: int | None = None, up=0, down=0) -> Piece:
        if side not in SIDES:
            raise BuildError(f"side must be one of {', '.join(SIDES)}")
        length = length or self.footprint.across
        if length < self.footprint.across:
            raise BuildError(f"{name}: a portal is {self.footprint.across} cells wide on this grid")
        index = self._level(level, up, down)
        storeys = self.footprint.storeys
        if index + storeys > self.level_count:
            raise BuildError(f"{name}: a portal wall is {storeys} storeys tall and leaves the map from level {index}")
        col, row = at
        rect = (col, row, col + length, row + 1) if side in "NS" else (col, row, col + 1, row + length)
        start, end = self._side_segment(rect, side)
        face = FACE_INTO_CELL[side]
        materials = {**dict.fromkeys(FACES, self.solid), face: self.portal}
        for storey in range(index, index + storeys):
            self._walls(storey, start, end, materials, face)
        return self._register(Piece(name, "portal_wall", index, *rect, face=face))

    # The grid line along `side` of a footprint.
    @staticmethod
    def _side_segment(rect, side: str, offset: int = 0, width: int | None = None):
        c0, r0, c1, r1 = rect
        if side in "NS":
            line = r0 if side == "N" else r1
            a = c0 + offset
            return (a, line), (c1 if width is None else a + width, line)
        line = c0 if side == "W" else c1
        a = r0 + offset
        return (line, a), (line, r1 if width is None else a + width)

    @staticmethod
    def _on_segment(wall: dict, start, end) -> bool:
        c0, r0, c1, r1 = edge_key(wall)
        (sc, sr), (ec, er) = start, end
        if sr == er:
            return r0 == r1 == sr and min(sc, ec) <= c0 and c1 <= max(sc, ec)
        return c0 == c1 == sc and min(sr, er) <= r0 and r1 <= max(sr, er)

    # Walls on the unit edges from `start` to `end`: a new wall takes
    # `materials`, and one already there takes `face` from them alone.
    def _walls(self, level: int, start, end, materials: dict, face: str | None = None) -> None:
        data = paint_edges(self.data, level, start, end, material=materials)
        if face is not None:
            walls = data["levels"][level]["walls"]
            for at, wall in enumerate(walls):
                if self._on_segment(wall, start, end):
                    walls[at] = self._refaced(wall, face, materials[face])
        self.data = data

    @staticmethod
    def _refaced(record: dict, face: str, material: str) -> dict:
        faces = record if all(name in record for name in FACES) else expand_face_materials(record)
        refaced = {**record, **{name: faces[name] for name in FACES}, face: material}
        refaced.pop("all", None)
        return refaced

    # Floor slabs over `rect`, with the same rule for `face`: a room's ceiling
    # is the slab the storey above stands on.
    def _slab(self, level: int, rect, materials: dict, face: str) -> None:
        data = paint_floors(self.data, level, rect, materials)
        c0, r0, c1, r1 = rect
        for name in ("floors", "inaccessible_floors"):
            slabs = data["levels"][level][name]
            for at, slab in enumerate(slabs):
                if c0 <= slab["col"] < c1 and r0 <= slab["row"] < r1:
                    slabs[at] = self._refaced(slab, face, materials[face])
        self.data = data

    # A room: a floor, walls around it `storeys` tall, and a ceiling, which is
    # the floor slab of the level above. `inside` and `outside` are the wall
    # faces' texture aliases, one for all four walls or one per side; `ceiling`
    # is an alias, True for the solid one, or False for an open top. A wall
    # shared with an earlier room keeps that room's faces.
    def room(
        self,
        name: str,
        level,
        size,
        at=None,
        *,
        storeys: int = 1,
        floor: str | None = None,
        inside: str | dict[str, str] | None = None,
        outside: str | dict[str, str] | None = None,
        ceiling: str | bool = True,
        up=0,
        down=0,
        **where,
    ) -> Piece:
        index = self._level(level, up, down)
        top = index + storeys
        if storeys < 1 or top > self.level_count or (ceiling is not False and top >= self.level_count):
            raise BuildError(f"{name}: {storeys} storeys from level {index} leave no level for the walls and ceiling")
        rect = self._place(size, at, **where)
        self._slab(index, rect, dict.fromkeys(FACES, self._alias(floor, self.default)), "top")
        for side in SIDES:
            start, end = self._side_segment(rect, side)
            face = FACE_INTO_CELL[side]
            # A wall's ends and edges show in doorways, so they match its inside.
            materials = {
                **dict.fromkeys(FACES, self._side_alias(inside, side)),
                OPPOSITE_FACE[face]: self._side_alias(outside, side),
            }
            for storey in range(index, top):
                self._walls(storey, start, end, materials, face)
        if ceiling is not False:
            under = self.default if ceiling is True else self._alias(ceiling, self.default)
            self._slab(top, rect, {**dict.fromkeys(FACES, self.default), "bottom": under}, "bottom")
        return self._register(Piece(name, "room", index, *rect, levels=storeys))

    def _side_alias(self, material: str | dict[str, str] | None, side: str) -> str:
        if isinstance(material, dict):
            unknown = set(material) - set(SIDES)
            if unknown:
                raise BuildError(f"wall materials are keyed by side ({', '.join(SIDES)}), not {sorted(unknown)}")
            material = material.get(side)
        return self._alias(material, self.default)

    def _room(self, name: str) -> Piece:
        piece = self.piece(name)
        if piece.kind != "room":
            raise BuildError(f"{name!r} is a {piece.kind}, not a room")
        return piece

    # An opening in a room's wall, `width` cells wide and `storeys` tall from
    # its storey `storey`, `offset` cells from the wall's west or north end
    # and centred without one. `eraser` fills it with an equipment eraser
    # and `field` with a barrier of that field: a window is an opening a
    # storey up filled with a field.
    def doorway(
        self,
        room: str,
        side: str,
        offset: int | None = None,
        *,
        width: int = 1,
        storeys: int | None = None,
        storey: int = 0,
        eraser: bool = False,
        field: str | None = None,
    ) -> None:
        piece = self._room(room)
        if side not in SIDES:
            raise BuildError(f"side must be one of {', '.join(SIDES)}")
        span = piece.size[0] if side in "NS" else piece.size[1]
        offset = (span - width) // 2 if offset is None else offset
        if width < 1 or offset < 0 or offset + width > span:
            raise BuildError(f"{room}: a doorway {width} wide at offset {offset} leaves its {span}-cell wall")
        storeys = min(self.footprint.doorway, piece.levels - storey) if storeys is None else storeys
        if storey < 0 or storeys < 1 or storey + storeys > piece.levels:
            raise BuildError(f"{room}: an opening fits storeys 0 to {piece.levels - 1}")
        start, end = self._side_segment(piece.rect, side, offset, width)
        for storey in range(piece.level + storey, piece.level + storey + storeys):
            level = self.data["levels"][storey]
            removed = {edge_key(w) for w in level["walls"] if self._on_segment(w, start, end)}
            level["walls"] = [w for w in level["walls"] if edge_key(w) not in removed]
            level["lights"] = lights_off_edges(level["lights"], removed)
            if eraser:
                self.data = paint_erasers(self.data, storey, start, end)
            if field is not None:
                self.data = paint_edges(self.data, storey, start, end, field=field)

    # A wall light in cell (col, row) on its `side` wall, facing the cell,
    # `height` metres above that level's floor; a height past the storey
    # hangs on the section above. A portal keeps 0.4 m from a light
    # (PORTAL_LIGHT_CLEARANCE).
    def light(self, level, at, side: str, *, kind: str, height: float) -> None:
        kinds = load_wall_light_kinds()
        if kind not in kinds:
            raise BuildError(f"unknown light kind {kind!r}; one of {', '.join(kinds)}")
        index, local = self._light_level(level, height)
        col, row = at
        error = light_placement_error(self.data, index, col, row, side)
        if error:
            raise BuildError(f"light at ({col}, {row}) {side}: {error}")
        light = {"col": col, "row": row, "side": side, "kind": kind, "height": local}
        self.data["levels"][index]["lights"].append(light)

    def _light_level(self, level, height: float) -> tuple[int, float]:
        if not isfinite(height) or height <= 0:
            raise BuildError("a light hangs a finite positive number of metres above its floor")
        storeys, local = divmod(height, self.frame.level_height)
        # A boundary belongs to the section below, keeping the record's height positive.
        if local == 0:
            storeys -= 1
            local = self.frame.level_height
        index = self._level(self._level(level) + int(storeys))
        return index, round(local, 3) or local

    # Lights along a room's walls, one every `every` cells where a wall
    # stands, in one row `height` metres above the floor of its storey
    # `storey`: without one, part way up a single wall as tall as a room a
    # standing body needs, whatever the storeys. Without
    # `portal_faces` they keep off faces that take a portal, which a light
    # keeps 0.4 m away. Returns how many it placed.
    def room_lights(
        self,
        room: str,
        kind: str,
        *,
        every: int = 3,
        storey: int = 0,
        height: float | None = None,
        sides: str = "NSEW",
        portal_faces: bool = True,
    ) -> int:
        piece = self._room(room)
        if every < 1 or not 0 <= storey < piece.levels:
            raise BuildError(f"{room}: lights go every 1 or more cells on storeys 0 to {piece.levels - 1}")
        if height is None:
            height = WALL_LIGHT_HEIGHT_FRACTION * (
                self.footprint.doorway * self.frame.level_height - self.frame.floor_thickness
            )
        index, _ = self._light_level(piece.level + storey, height)
        walls = {edge_key(w): w for w in self.data["levels"][index]["walls"]}
        c0, r0, c1, r1 = piece.rect
        placed = 0
        for side in sides:
            if side not in SIDES:
                raise BuildError(f"side must be one of {', '.join(SIDES)}")
            if side in "NS":
                cells = [(col, r0 if side == "N" else r1 - 1) for col in range(c0, c1)]
            else:
                cells = [(c0 if side == "W" else c1 - 1, row) for row in range(r0, r1)]
            for col, row in cells[every // 2 :: every]:
                start, end = self._side_segment((col, row, col + 1, row + 1), side)
                wall = walls.get((*start, *end))
                if wall is None:
                    continue
                inside = expand_face_materials(wall)[FACE_INTO_CELL[side]]
                if not portal_faces and self.catalogs.texture_catalog.get(inside):
                    continue
                if light_placement_error(self.data, index, col, row, side) is None:
                    self.light(piece.level + storey, (col, row), side, kind=kind, height=height)
                    placed += 1
        return placed

    # Actors of one kind spawning in a rectangle. `count` is one target or a
    # nondecreasing list for one, two, three, and four or more players;
    # `respawn_secs` None never refills a killed actor's slot.
    def actor_zone(
        self,
        kind: str,
        level,
        size,
        at=None,
        *,
        count: int | list[int] = 1,
        respawn_secs: float | None = None,
        beam_in_secs: float = 0.0,
        roam: float = 0.0,
        levels: int = 1,
        switch: str | None = None,
        initially_on: bool = True,
        until_checkpoint: int | None = None,
        on_checkpoint: str | None = None,
        up=0,
        down=0,
        **where,
    ) -> None:
        kinds = load_actor_kinds()
        if kind not in kinds:
            raise BuildError(f"unknown actor kind {kind!r}; one of {', '.join(kinds)}")
        index = self._level(level, up, down)
        c0, r0, c1, r1 = self._place(size, at, **where)
        zone = {
            "level": index,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
            "count": [count] if isinstance(count, int) else list(count),
            "respawn_secs": respawn_secs,
            "beam_in_secs": beam_in_secs,
        }
        if levels != 1:
            zone["levels"] = levels
        if roam:
            zone["roam_distance"] = roam
        if switch is not None:
            zone["switch"] = switch
        if switch is not None or not initially_on:
            zone["initially_on"] = initially_on
        if until_checkpoint is not None:
            zone["until_checkpoint"] = until_checkpoint
        if on_checkpoint is not None:
            zone["on_checkpoint"] = on_checkpoint
        self.data[ACTOR_ZONE_LIST] = [*self.data[ACTOR_ZONE_LIST], zone]

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

    # A slab's side lies flush in the wall it meets: the band between that
    # wall's sections on the far side. Where no slab continues it, the side
    # takes the face of the wall under it, or else of the wall standing on it.
    def _dress_slab_sides(self) -> None:
        levels = self.data["levels"]
        walls = [{edge_key(wall): wall for wall in level["walls"]} for level in levels]
        for index, level in enumerate(levels):
            slabs = [slab for name in ("floors", "inaccessible_floors") for slab in level[name]]
            cells = {(slab["col"], slab["row"]) for slab in slabs}
            for name in ("floors", "inaccessible_floors"):
                for at, slab in enumerate(level[name]):
                    col, row = slab["col"], slab["row"]
                    for side, (dc, dr) in STEP.items():
                        if (col + dc, row + dr) in cells:
                            continue
                        start, end = self._side_segment((col, row, col + 1, row + 1), side)
                        wall = (walls[index - 1].get((*start, *end)) if index else None) or walls[index].get(
                            (*start, *end)
                        )
                        if wall is not None:
                            face = OPPOSITE_FACE[FACE_INTO_CELL[side]]
                            level[name][at] = slab = self._refaced(slab, face, expand_face_materials(wall)[face])

    # The map the game would load, normalized, canonical, and valid, with the validator's warnings.
    def document(self) -> tuple[dict, list[str]]:
        self._dress_slab_sides()
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
