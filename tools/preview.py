#!/usr/bin/env python3
"""ASCII preview of a Cuboid Wars building JSON file."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BUILDING = REPO_ROOT / "server" / "assets" / "buildings" / "default.json"

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
RAMP_EXIT_NORTH = "↑"
RAMP_EXIT_SOUTH = "↓"
RAMP_EXIT_EAST = "→"
RAMP_EXIT_WEST = "←"

FILLABLE = {
    NO_FLOOR,
    RAMP_NORTH_UP,
    RAMP_SOUTH_UP,
    RAMP_EAST_UP,
    RAMP_WEST_UP,
    RAMP_NORTH_DOWN,
    RAMP_SOUTH_DOWN,
    RAMP_EAST_DOWN,
    RAMP_WEST_DOWN,
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


def load_building(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if data.get("version") != 1:
        sys.exit(f"unsupported building file version {data.get('version')!r}")
    return data["building"]


def floor_set(level: dict) -> set[tuple[int, int]]:
    return {(col, row) for col, row in level.get("floors", [])}


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


def render_level(building: dict, level_idx: int) -> list[list[str]]:
    level = building["levels"][level_idx]
    cols = building["grid_cols"]
    rows = building["grid_rows"]

    h = 2 * rows + 1
    w = 2 * cols + 1
    grid = [[NONE] * w for _ in range(h)]
    floors = floor_set(level)

    for row in range(rows):
        for col in range(cols):
            grid[2 * row + 1][2 * col + 1] = FLOOR if (col, row) in floors else NO_FLOOR

    for ramp in building.get("ramps", []):
        lower = ramp["lower_level"]
        c0, r0, c1, r1 = ramp_rect(ramp)
        up = ramp_up_direction(ramp)
        if level_idx == lower:
            ch = {
                "north": RAMP_NORTH_UP,
                "south": RAMP_SOUTH_UP,
                "east": RAMP_EAST_UP,
                "west": RAMP_WEST_UP,
            }[up]
            paint_rect(grid, c0, r0, c1, r1, ch)
        elif level_idx == lower + 1:
            ch = {
                "north": RAMP_SOUTH_DOWN,
                "south": RAMP_NORTH_DOWN,
                "east": RAMP_WEST_DOWN,
                "west": RAMP_EAST_DOWN,
            }[up]
            paint_rect(grid, c0, r0, c1, r1, ch)
            paint_ramp_exit(grid, up, c0, r0, c1, r1, cols, rows)

    for c0, r0, c1, r1 in level.get("walls", []):
        if r0 == r1:
            grid[2 * r0][2 * min(c0, c1) + 1] = HORIZ
        else:
            grid[2 * min(r0, r1) + 1][2 * c0] = VERT

    paint_corners(grid, cols, rows)
    fill_regions(grid, cols, rows)
    return grid


def paint_rect(grid: list[list[str]], c0: int, r0: int, c1: int, r1: int, ch: str) -> None:
    for row in range(r0, r1):
        for col in range(c0, c1):
            grid[2 * row + 1][2 * col + 1] = ch


def paint_ramp_exit(grid: list[list[str]], up: str, c0: int, r0: int, c1: int, r1: int, cols: int, rows: int) -> None:
    if up == "north":
        exit_row = r0 - 1
        if 0 <= exit_row < rows:
            for col in range(c0, c1):
                grid[2 * exit_row + 1][2 * col + 1] = RAMP_EXIT_NORTH
    elif up == "south":
        exit_row = r1
        if 0 <= exit_row < rows:
            for col in range(c0, c1):
                grid[2 * exit_row + 1][2 * col + 1] = RAMP_EXIT_SOUTH
    elif up == "east":
        exit_col = c1
        if 0 <= exit_col < cols:
            for row in range(r0, r1):
                grid[2 * row + 1][2 * exit_col + 1] = RAMP_EXIT_EAST
    elif up == "west":
        exit_col = c0 - 1
        if 0 <= exit_col < cols:
            for row in range(r0, r1):
                grid[2 * row + 1][2 * exit_col + 1] = RAMP_EXIT_WEST


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
            elif ch in FILLABLE:
                filler = ch
            else:
                filler = " "
            out.append(filler + ch + filler)
        else:
            out.append(ch)
    return "".join(out)


def render_building(building: dict, level_filter: int | None = None) -> str:
    out = []
    for idx, level in enumerate(building["levels"]):
        if level_filter is not None and idx != level_filter:
            continue
        name = level.get("name") or f"Level {idx}"
        out.append(f"=== Level {idx} ({name}) ===")
        for row in render_level(building, idx):
            out.append(widen(row))
        out.append("")
    return "\n".join(out)


def main() -> None:
    parser = argparse.ArgumentParser(description="ASCII preview of a Cuboid Wars building JSON file.")
    parser.add_argument("-f", "--file", type=Path, default=DEFAULT_BUILDING, help="Path to the building JSON file.")
    parser.add_argument("-l", "--level", type=int, default=None, help="Render only this level index (0-based).")
    args = parser.parse_args()

    building = load_building(args.file)
    if args.level is not None and not (0 <= args.level < len(building["levels"])):
        sys.exit(f"--level {args.level} out of range (0..{len(building['levels']) - 1})")

    print(render_building(building, args.level))


if __name__ == "__main__":
    main()
