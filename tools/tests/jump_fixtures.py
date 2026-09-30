"""Test-owned physics, settings, and preview replies for the Jump Path tests."""

from map_editor.jump_settings import JumpSettings


# One metre a second, two with the speed pickup; a 2 m/s jump under 2 m/s² gravity; no air braking.
def jump_physics(**overrides):
    player = {
        "move_speed": 1,
        "move_speed_power_up": 2,
        "move_speed_ladder": 0.5,
        "jump_speed": 2,
        "ground_acceleration": 20,
        "ground_deceleration": 30,
        "ground_lateral_deceleration": 40,
        "air_acceleration": 0,
        "air_deceleration": 0,
        "air_lateral_deceleration": 0,
    }
    physics = {
        "server_hz": 30,
        "gravity": 2,
        "low_gravity": 1,
        "player": player,
        "player_fall": {"safe_distance": 4, "lethal_distance": 12},
        "max_health": 100,
        "body": {"diameter": 0.6, "height": 1.8},
        "portal_size": {"width": 1.4, "height": 2.6},
        "funnel": {"capture_margin": 0.6, "capture_growth": 1.0},
    }
    for key, value in overrides.items():
        (player if key in player else physics)[key] = value
    return physics


def jump_settings(cell_size=4, level_height=5, wall_thickness=0.4, **overrides):
    return JumpSettings(cell_size, level_height, wall_thickness, jump_physics(**overrides))


# One scenario of a `jump_preview` reply.
def scenario(path, crossings=(), **fields):
    return {
        "path": [list(point) for point in path],
        "hop": None,
        "hop_time": None,
        "end": "below",
        "crossings": [
            {"level": level, "phase": phase, "point": list(point), "time": time, "damage": damage}
            for level, phase, point, time, damage in crossings
        ],
        "entry": None,
        "capture": [],
        "range": [],
        "capture_steered": [],
        "exit_range": [],
        **fields,
    }
