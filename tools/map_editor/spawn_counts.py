"""Actor population settings shared by editor validation and controls."""

from .core import call


def actor_count_error(count):
    return call("actor_count_error", count)


def actor_count_summary(count):
    if actor_count_error(count):
        return str(count)
    return str(count[0]) if len(count) == 1 else f"{count[0]}–{count[-1]} by players"


def actor_count_preview(count):
    error = actor_count_error(count)
    if error:
        return error
    if len(count) == 1:
        return f"{count[0]} actors, regardless of player count."
    rows = [
        f"{index}{'+' if index == len(count) else ''} {'player' if index == 1 else 'players'} → {value} actors"
        for index, value in enumerate(count, 1)
    ]
    return "; ".join(rows) + f". Empty server: {count[0]} actors. Dead players still count."
