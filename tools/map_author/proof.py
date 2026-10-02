"""An experiment report as one line per action: what happened and where the body ended up."""

from __future__ import annotations

from .frame import GridFrame

# Events worth a word; `player_step` and projectile samples are per tick and dropped.
KEPT = {
    "player_portal_crossing",
    "player_landed",
    "player_fall_damage",
    "player_hit",
    "checkpoint_reached",
    "item_collected",
    "equipment_erased",
    "player_died",
    "player_relocated",
    "player_crushed",
    "player_fell_out_of_world",
    "portal_opened",
    "portal_fizzled",
    "fireworks_started",
    "actor_died",
}


def summarize(report: dict, frame: GridFrame) -> str:
    lines = []
    totals = {"checks": 0, "passed": 0, "crossings": 0, "deaths": 0, "fall_damage": 0, "fireworks": False}
    checkpoints = []
    for step in report["steps"]:
        action, result, events, state = step["action"], step["result"], step["events"], step["state"]
        words = [f"#{step['index']:<3}", f"{_action(action):<34}", _result(action, result)]
        notes = [note for note in (_event(event, frame) for event in events) if note]
        if notes:
            words.append("  " + " | ".join(notes))
        lines.append(" ".join(words))
        if action["action"] in ("move", "advance", "check", "reset") and state.get("player"):
            lines.append(" " * 39 + _position(state["player"], frame))
        if action["action"] == "check":
            totals["checks"] += 1
            totals["passed"] += result["status"] == "passed"
        for event in events:
            kind = event["kind"]
            if kind == "player_portal_crossing":
                totals["crossings"] += 1
            elif kind == "player_died":
                totals["deaths"] += 1
            elif kind == "player_fall_damage":
                totals["fall_damage"] += 1
            elif kind == "fireworks_started":
                totals["fireworks"] = True
            elif kind == "checkpoint_reached":
                checkpoints.append(event["checkpoint"])
    final = report["steps"][-1]["state"] if report["steps"] else report["initial"]
    player = final.get("player")
    if player:
        lines.append(
            f"final {_position(player, frame)}  cp {player['checkpoint']}  speed {'yes' if player['speed'] else 'no'}  "
            f"low gravity {'yes' if player['low_gravity'] else 'no'}  switches {final['active_switches']}  "
            f"open fields {final['open_fields']}"
        )
    else:
        lines.append("final: player dead")
    lines.append(
        f"checks {totals['passed']}/{totals['checks']} passed   crossings {totals['crossings']}   "
        f"checkpoints {','.join(map(str, checkpoints)) or 'none'}   deaths {totals['deaths']}   "
        f"fall damage {totals['fall_damage']}   fireworks {'yes' if totals['fireworks'] else 'no'}"
    )
    return "\n".join(lines)


def _action(action: dict) -> str:
    kind = action["action"]
    if kind == "move":
        flags = (" crouch" if action.get("crouch") else "") + (" jump" if action.get("jump") else "")
        return f"move ({action['direction'][0]:g},{action['direction'][1]:g}) x{action['ticks']}{flags}"
    if kind == "advance":
        return f"advance x{action['ticks']}"
    if kind == "aim":
        return f"aim ({', '.join(f'{v:g}' for v in action['target'])})"
    if kind == "portal":
        return f"portal {action['end']}"
    if kind == "check":
        grounded = "" if action.get("grounded", True) else " airborne"
        return f"check{grounded} [{_vec(action['min'])} .. {_vec(action['max'])}]"
    return kind


def _result(action: dict, result: dict) -> str:
    status = result.get("status", "?")
    if status == "rejected":
        return f"REJECTED {result.get('reason')}"
    if status == "fizzled":
        return f"FIZZLED {result.get('reason')}"
    if status == "failed":
        return f"FAILED {result.get('reason')}"
    if status == "passed":
        return "PASSED"
    if status == "submitted":
        portal = result.get("portal", {})
        return f"submitted at {_vec(portal.get('position', []))} normal {_vec(portal.get('normal', []), 0)}"
    if status == "interrupted":
        return f"interrupted after {result.get('ticks')} ticks"
    if status == "simulated":
        return f"simulated {result.get('ticks')} ticks"
    return status


def _event(event: dict, frame: GridFrame) -> str | None:
    kind = event["kind"]
    if kind not in KEPT and not (kind == "jump" and not event.get("accepted", True)):
        return None
    tick = f"t{event.get('tick', '?')}"
    if kind == "jump":
        return f"{tick} jump refused"
    if kind == "player_portal_crossing":
        return f"{tick} crossing v{_vec(event['velocity_before'])} -> v{_vec(event['velocity_after'])}"
    if kind == "player_landed":
        return f"{tick} landed at {event['impact_speed']:.1f} m/s"
    if kind in ("player_fall_damage", "player_hit"):
        return f"{tick} {kind.replace('player_', '').replace('_', ' ')} hp {event['health']:.0f}"
    if kind == "checkpoint_reached":
        return f"{tick} checkpoint {event['checkpoint']}"
    if kind == "item_collected":
        return f"{tick} collected {event['item']}"
    if kind == "portal_opened":
        return f"{tick} portal opened"
    if kind == "actor_died":
        return f"{tick} actor {event.get('actor')} died"
    return f"{tick} {kind.replace('player_', '').replace('_', ' ')}"


def _position(player: dict, frame: GridFrame) -> str:
    x, y, z = player["position"]
    col, row = frame.cell_of_world(x, z)
    return (
        f"-> ({x:.2f}, {y:.2f}, {z:.2f})  {frame.describe_y(y)} cell ({col}, {row})  "
        f"{player['support']}  hp {player['health']:.0f}"
    )


def _vec(values, places: int = 1) -> str:
    return "(" + ", ".join(f"{float(v):.{places}f}" for v in values) + ")"
