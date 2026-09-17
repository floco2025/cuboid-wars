"""Placement operations dispatched by the input controller on release."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .constants import (
    ERASE_MODES,
    MODE_ACTOR_SPAWN_ZONE,
    MODE_BARRIER,
    MODE_CHECKPOINT,
    MODE_EQUIPMENT_ERASER,
    MODE_ERASE_KEEP_FLOORS,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ITEM,
    MODE_JUMP_REACH,
    MODE_PORTAL_JUMP,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_LIGHT_BRIDGE,
    MODE_NESTED_MAP,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_MATERIAL,
    MODE_RUN_TIME,
    MODE_SAMPLE,
    MODE_TERRAIN,
    MODE_WALL,
    MODE_WALL_MATERIAL,
    RAMP_MODES,
)

if TYPE_CHECKING:
    from .canvas import Canvas
from .erasing import ERASE_GROUPS
from .geometry import snapped_wall_end


def _cell_rect_tool(method: str):
    """Tool for a completed cell-rect drag: `window.<method>(start, end)`."""

    def handler(canvas: "Canvas", event) -> None:
        if canvas.drag_start_cell and canvas.drag_current_cell:
            getattr(canvas.window, method)(canvas.drag_start_cell, canvas.drag_current_cell)

    return handler


def _erase_group_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.erase_group_rect(canvas.input.gesture.mode, canvas.drag_start_cell, canvas.drag_current_cell)


def _wall_line_tool(method: str):
    """Tool for a grid-point drag along a wall line; the end snaps axis-aligned."""

    def handler(canvas: "Canvas", event) -> None:
        if canvas.drag_start_point and canvas.drag_current_point:
            getattr(canvas.window, method)(
                canvas.drag_start_point,
                snapped_wall_end(canvas.drag_start_point, canvas.drag_current_point),
            )

    return handler


def _click_place_tool(add_method: str):
    """Click places on the released cell: `window.<add_method>(col, row)`."""

    def handler(canvas: "Canvas", event) -> None:
        cell = canvas.point_to_cell(event.position())
        if cell is not None:
            getattr(canvas.window, add_method)(*cell)

    return handler


def _ramp_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.add_ramp(canvas.drag_start_cell, canvas.drag_current_cell, canvas.input.gesture.mode)


def _nested_map_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.add_nested_map(canvas.drag_start_cell, canvas.drag_current_cell)


def _wall_material_tool(canvas: "Canvas", event) -> None:
    if not (canvas.drag_start_point and canvas.drag_current_point):
        return
    start, end = canvas.drag_start_point, canvas.drag_current_point
    if start == end:
        # Pure click (no drag): grab the wall under the cursor and use its
        # endpoints as the rectangle so the rect path applies to that single
        # wall.
        wall = canvas._wall_near_position(event.position())
        if wall is not None:
            start = (wall["c0"], wall["r0"])
            end = (wall["c1"], wall["r1"])
    if start != end:
        canvas.window.assign_wall_materials_rect(start, end)


def _light_tool(canvas: "Canvas", event) -> None:
    canvas.window.add_light_at(canvas.grid_position(event.position()))


def _ladder_tool(canvas: "Canvas", event) -> None:
    canvas.window.add_ladder_at(canvas.grid_position(event.position()))


def _erase_cells_tool(canvas: "Canvas", event) -> None:
    preserve_floors = canvas.input.gesture.mode == MODE_ERASE_KEEP_FLOORS
    if canvas.drag_start_cell and canvas.drag_current_cell and canvas.drag_start_cell != canvas.drag_current_cell:
        canvas.window.erase_cell_rect(canvas.drag_start_cell, canvas.drag_current_cell, preserve_floors)
    else:
        canvas.window.erase_at(canvas.input.gesture.start, preserve_floors)


CLICK_TOOLS = {
    MODE_SAMPLE: lambda canvas, event: canvas.window.sample_at(canvas.grid_position(event.position())),
    MODE_JUMP_REACH: lambda canvas, event: canvas.window.jump_reach.select(*canvas.point_to_cell(event.position())),
    MODE_PORTAL_JUMP: lambda canvas, event: canvas.window.portal_jump.select(canvas.grid_position(event.position())),
    MODE_RUN_TIME: lambda canvas, event: canvas.window.run_time.select(*canvas.point_to_cell(event.position())),
    MODE_LIGHT: _light_tool,
    MODE_LADDER: _ladder_tool,
    MODE_PRESSURE_PLATE: _click_place_tool("prompt_and_add_pressure_plate"),
    MODE_ITEM: _click_place_tool("prompt_and_add_item"),
}


RELEASE_TOOLS = {
    MODE_FLOOR: _cell_rect_tool("add_floor_rect"),
    MODE_INACCESSIBLE_FLOOR: _cell_rect_tool("add_inaccessible_floor_rect"),
    MODE_TERRAIN: _cell_rect_tool("add_terrain_rect"),
    MODE_ACTOR_SPAWN_ZONE: _cell_rect_tool("add_actor_spawn_zone_rect"),
    MODE_CHECKPOINT: _cell_rect_tool("add_checkpoint_rect"),
    MODE_WALL: _wall_line_tool("add_wall_line"),
    MODE_BARRIER: _wall_line_tool("prompt_and_add_barrier_line"),
    MODE_EQUIPMENT_ERASER: _wall_line_tool("add_equipment_eraser_line"),
    MODE_LIGHT_BRIDGE: _cell_rect_tool("prompt_and_add_light_bridge_rect"),
    MODE_FLOOR_MATERIAL: _cell_rect_tool("assign_floor_materials_rect"),
    MODE_WALL_MATERIAL: _wall_material_tool,
    MODE_RAMP_MATERIAL: _cell_rect_tool("assign_ramp_materials_rect"),
    MODE_NESTED_MAP: _nested_map_tool,
    **dict.fromkeys(RAMP_MODES, _ramp_tool),
    **dict.fromkeys(ERASE_GROUPS, _erase_group_tool),
    **dict.fromkeys(ERASE_MODES, _erase_cells_tool),
}
