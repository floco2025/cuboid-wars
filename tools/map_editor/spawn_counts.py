"""Actor population settings shared by editor validation and controls."""

MAX_ACTOR_COUNT = 2**32 - 1


def actor_count_error(count):
    if not isinstance(count, list):
        return "Count must be a list, for example [3] or [2, 3, 4]."
    if not count:
        return "Count needs at least one entry."
    if any(type(value) is not int or not 0 <= value <= MAX_ACTOR_COUNT for value in count):
        return f"Count entries must be whole numbers from 0 to {MAX_ACTOR_COUNT}."
    if any(left > right for left, right in zip(count, count[1:])):
        return "Counts must stay the same or increase as players join."
    return None


def actor_count_key(count):
    if actor_count_error(count):
        return (2, type(count).__name__, repr(count))
    return (0, tuple(count))


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
