#!/usr/bin/env python3
"""ASCII preview of a Cuboid Wars map JSON file."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MAP = REPO_ROOT / "server" / "assets" / "maps" / "default.json"

FLOOR = "·"
NO_FLOOR = "█"
RAMP_NORTH_UP = "▲"
RAMP_SOUTH_UP = "▼"
RAMP_EAST_UP = "▶"
RAMP_WEST_UP = "◀"
RAMP_NORTH_DOWN = "△"
RAMP_SOUTH_DOWN = "▽"
RAMP_EAST_DOWN = "▷"
RAMP_WEST_DOWN = "◁"
PLAYER_SPAWN = "S"

WRAPPER_KEYS = {"version", "map"}
MAP_KEYS = {"grid_cols", "grid_rows", "player_spawn_fields", "levels", "ramps"}
LEVEL_KEYS = {"name", "floors", "inaccessible_floors", "walls"}
RAMP_KEYS = {"lower_level", "low", "high"}

RAMP_UP = {
    "north": RAMP_NORTH_UP,
    "south": RAMP_SOUTH_UP,
    "east": RAMP_EAST_UP,
    "west": RAMP_WEST_UP,
}
RAMP_DOWN = {
    "north": RAMP_NORTH_DOWN,
    "south": RAMP_SOUTH_DOWN,
    "east": RAMP_EAST_DOWN,
    "west": RAMP_WEST_DOWN,
}
RAMP_GLYPHS = set(RAMP_UP.values()) | set(RAMP_DOWN.values())

FILLABLE = {
    NO_FLOOR,
    *RAMP_GLYPHS,
}

HORIZ = "─"
VERT = "│"
NONE = " "

CORNERS = {
    (False, False, False, False): " ",
    (True, False, False, False): "╵",
    (False, True, False, False): "╷",
    (False, False, True, False): "╶",
    (False, False, False, True): "╴",
    (True, True, False, False): "│",
    (False, False, True, True): "─",
    (True, False, True, False): "└",
    (True, False, False, True): "┘",
    (False, True, True, False): "┌",
    (False, True, False, True): "┐",
    (True, True, True, False): "├",
    (True, True, False, True): "┤",
    (True, False, True, True): "┴",
    (False, True, True, True): "┬",
    (True, True, True, True): "┼",
}


def load_map(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if data.get("version") != 1:
        sys.exit(f"unsupported map file version {data.get('version')!r}")
    warn_unknown_keys("file", data, WRAPPER_KEYS)
    map_data = data["map"]
    warn_unknown_keys("map", map_data, MAP_KEYS)
    for idx, level in enumerate(map_data.get("levels", [])):
        warn_unknown_keys(f"map.levels[{idx}]", level, LEVEL_KEYS)
    for idx, ramp in enumerate(map_data.get("ramps", [])):
        warn_unknown_keys(f"map.ramps[{idx}]", ramp, RAMP_KEYS)
    return map_data


def warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)


def warn_unknown_keys(path: str, value: object, known_keys: set[str]) -> None:
    if not isinstance(value, dict):
        warn(f"{path} is {type(value).__name__}, expected object")
        return
    for key in sorted(set(value) - known_keys):
        warn(f"{path}: unknown key {key!r}")


def floor_set(level: dict) -> set[tuple[int, int]]:
    floors = {(col, row) for col, row in level.get("floors", [])}
    floors.update((col, row) for col, row in level.get("inaccessible_floors", []))
    return floors


def ramp_rect(ramp: dict) -> tuple[int, int, int, int]:
    low = ramp["low"]
    high = ramp["high"]
    return min(low[0], high[0]), min(low[1], high[1]), max(low[0], high[0]), max(low[1], high[1])


def ramp_up_direction(ramp: dict) -> str:
    low = ramp["low"]
    high = ramp["high"]
    dx = high[0] - low[0]
    dy = high[1] - low[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def opposite_direction(direction: str) -> str:
    return {
        "north": "south",
        "south": "north",
        "east": "west",
        "west": "east",
    }[direction]


def render_level(map_data: dict, level_idx: int) -> list[list[str]]:
    level = map_data["levels"][level_idx]
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]

    h = 2 * rows + 1
    w = 2 * cols + 1
    grid = [[NONE] * w for _ in range(h)]
    floors = floor_set(level)
    visible_ramps: list[tuple[int, int, int, int, str, str]] = []

    for row in range(rows):
        for col in range(cols):
            grid[2 * row + 1][2 * col + 1] = FLOOR if (col, row) in floors else NO_FLOOR

    paint_player_spawn_fields(grid, map_data, level_idx)

    for ramp_idx, ramp in enumerate(map_data.get("ramps", [])):
        if not ramp_is_previewable(ramp, ramp_idx, len(map_data["levels"])):
            continue
        lower = ramp["lower_level"]
        c0, r0, c1, r1 = ramp_rect(ramp)
        up = ramp_up_direction(ramp)
        if level_idx == lower:
            ch = RAMP_UP[up]
            paint_rect(grid, c0, r0, c1, r1, ch)
            visible_ramps.append((c0, r0, c1, r1, ch, up))
        elif level_idx == lower + 1:
            down = opposite_direction(up)
            ch = RAMP_DOWN[down]
            paint_rect(grid, c0, r0, c1, r1, ch)
            visible_ramps.append((c0, r0, c1, r1, ch, down))

    for wall_idx, wall in enumerate(level.get("walls", [])):
        if not wall_is_previewable(wall, level_idx, wall_idx):
            continue
        c0, r0, c1, r1 = wall
        if r0 == r1:
            grid[2 * r0][2 * min(c0, c1) + 1] = HORIZ
        else:
            grid[2 * min(r0, r1) + 1][2 * c0] = VERT

    paint_corners(grid, cols, rows)
    fill_regions(grid, cols, rows)
    for c0, r0, c1, r1, ch, direction in visible_ramps:
        fill_ramp_ends(grid, c0, r0, c1, r1, ch, direction)
    return grid


def paint_rect(grid: list[list[str]], c0: int, r0: int, c1: int, r1: int, ch: str) -> None:
    for row in range(r0, r1):
        for col in range(c0, c1):
            grid[2 * row + 1][2 * col + 1] = ch


def paint_player_spawn_fields(grid: list[list[str]], map_data: dict, level_idx: int) -> None:
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    for idx, field in enumerate(map_data.get("player_spawn_fields", [])):
        parsed = parse_spawn_field(field)
        if parsed is None:
            warn(f"map.player_spawn_fields[{idx}] must be [col, row] or [level, col, row]: {field!r}")
            continue
        level, col, row = parsed
        if level != level_idx:
            continue
        if not (0 <= col < cols and 0 <= row < rows):
            warn(f"map.player_spawn_fields[{idx}] is outside the grid: {field!r}")
            continue
        grid[2 * row + 1][2 * col + 1] = PLAYER_SPAWN


def wall_is_previewable(wall: object, level_idx: int, wall_idx: int) -> bool:
    if not (isinstance(wall, list) and len(wall) == 4 and all(isinstance(v, int) for v in wall)):
        warn(f"map.levels[{level_idx}].walls[{wall_idx}] has unsupported shape {wall!r}")
        return False
    c0, r0, c1, r1 = wall
    if c0 != c1 and r0 != r1:
        warn(f"map.levels[{level_idx}].walls[{wall_idx}] is diagonal and cannot be previewed: {wall!r}")
        return False
    return True


def ramp_is_previewable(ramp: object, ramp_idx: int, level_count: int) -> bool:
    if not isinstance(ramp, dict):
        warn(f"map.ramps[{ramp_idx}] is {type(ramp).__name__}, expected object")
        return False
    required_keys = RAMP_KEYS
    missing_keys = sorted(required_keys - set(ramp))
    if missing_keys:
        warn(f"map.ramps[{ramp_idx}] is missing keys: {', '.join(missing_keys)}")
        return False
    lower = ramp["lower_level"]
    if not isinstance(lower, int):
        warn(f"map.ramps[{ramp_idx}].lower_level is not an integer: {lower!r}")
        return False
    if not (0 <= lower + 1 < level_count):
        warn(f"map.ramps[{ramp_idx}].lower_level is out of range: {lower!r}")
        return False
    if not is_int_point(ramp["low"]) or not is_int_point(ramp["high"]):
        warn(f"map.ramps[{ramp_idx}] has unsupported ramp points: {ramp!r}")
        return False
    c0, r0, c1, r1 = ramp_rect(ramp)
    if c0 == c1 or r0 == r1:
        warn(f"map.ramps[{ramp_idx}] has no previewable area: {ramp!r}")
        return False
    return True


def is_int_point(value: object) -> bool:
    return isinstance(value, list) and len(value) == 2 and all(isinstance(v, int) for v in value)


def parse_spawn_field(value: object) -> tuple[int, int, int] | None:
    if not isinstance(value, list) or not all(isinstance(v, int) for v in value):
        return None
    if len(value) == 2:
        col, row = value
        return 0, col, row
    if len(value) == 3:
        level, col, row = value
        return level, col, row
    return None


def fill_ramp_ends(grid: list[list[str]], c0: int, r0: int, c1: int, r1: int, ch: str, direction: str) -> None:
    if direction in ("north", "south"):
        for grid_row in (2 * r0, 2 * r1):
            for grid_col in range(2 * c0 + 1, 2 * c1, 2):
                if grid[grid_row][grid_col] == NONE:
                    grid[grid_row][grid_col] = ch
    else:
        for grid_col in (2 * c0, 2 * c1):
            for grid_row in range(2 * r0 + 1, 2 * r1, 2):
                if grid[grid_row][grid_col] == NONE:
                    grid[grid_row][grid_col] = ch


def paint_corners(grid: list[list[str]], cols: int, rows: int) -> None:
    h = 2 * rows + 1
    w = 2 * cols + 1
    for r in range(0, h, 2):
        for c in range(0, w, 2):
            n = r > 0 and grid[r - 1][c] == VERT
            s = r < h - 1 and grid[r + 1][c] == VERT
            e = c < w - 1 and grid[r][c + 1] == HORIZ
            w_ = c > 0 and grid[r][c - 1] == HORIZ
            grid[r][c] = CORNERS[(n, s, e, w_)]


def fill_regions(grid: list[list[str]], cols: int, rows: int) -> None:
    h = 2 * rows + 1
    w = 2 * cols + 1

    def cell_at(cell_row: int, cell_col: int) -> str:
        if not (0 <= cell_row < rows and 0 <= cell_col < cols):
            return NO_FLOOR
        return grid[2 * cell_row + 1][2 * cell_col + 1]

    for r in range(h):
        for c in range(w):
            if grid[r][c] != NONE:
                continue
            r_odd, c_odd = r % 2 == 1, c % 2 == 1
            if r_odd and c_odd:
                continue
            if not r_odd and c_odd:
                touching = [(r // 2 - 1, (c - 1) // 2), (r // 2, (c - 1) // 2)]
            elif r_odd and not c_odd:
                touching = [((r - 1) // 2, c // 2 - 1), ((r - 1) // 2, c // 2)]
            else:
                touching = [
                    (r // 2 - 1, c // 2 - 1),
                    (r // 2 - 1, c // 2),
                    (r // 2, c // 2 - 1),
                    (r // 2, c // 2),
                ]
            types = {cell_at(rr, cc) for rr, cc in touching}
            if len(types) == 1:
                t = next(iter(types))
                if t in FILLABLE:
                    grid[r][c] = t


def widen(row: list[str]) -> str:
    out = []
    for i, ch in enumerate(row):
        if i % 2 == 1:
            if ch == HORIZ:
                filler = HORIZ
                out.append(filler + ch + filler)
            elif ch in FILLABLE:
                out.append(format_fillable_cell(ch))
            else:
                out.append(" " + ch + " ")
        else:
            out.append(format_even_column(ch))
    return "".join(out)


def format_fillable_cell(ch: str) -> str:
    return ch * 3


def format_even_column(ch: str) -> str:
    return ch


def render_map(map_data: dict, level_filter: int | None = None) -> str:
    out = [
        "Legend: S = player spawn; filled ramp arrows go up; hollow ramp arrows go down.",
        "",
    ]
    for idx, level in enumerate(map_data["levels"]):
        if level_filter is not None and idx != level_filter:
            continue
        name = level.get("name") or f"Level {idx}"
        out.append(f"=== Level {idx} ({name}) ===")
        for row in render_level(map_data, idx):
            out.append(widen(row))
        out.append("")
    return "\n".join(out)


def main() -> None:
    parser = argparse.ArgumentParser(description="ASCII preview of a Cuboid Wars map JSON file.")
    parser.add_argument("-f", "--file", type=Path, default=DEFAULT_MAP, help="Path to the map JSON file.")
    parser.add_argument("-l", "--level", type=int, default=None, help="Render only this level index (0-based).")
    args = parser.parse_args()

    map_data = load_map(args.file)
    if args.level is not None and not (0 <= args.level < len(map_data["levels"])):
        sys.exit(f"--level {args.level} out of range (0..{len(map_data['levels']) - 1})")

    print(render_map(map_data, args.level))


if __name__ == "__main__":
    main()
