"""Kind catalogs spliced into a settings.json as text, so a catalog edit
rewrites only its own value and the rest of the file stays byte for byte."""

from __future__ import annotations

import json

# prettier --print-width 80 --object-wrap collapse, the format of every
# checked-in settings.json.
PRINT_WIDTH = 80
INDENT = "  "


def _scalar(value) -> str:
    return json.dumps(value, ensure_ascii=False)


def _object_text(entry: dict, indent: str, trailing: str) -> str:
    collapsed = (
        "{ " + ", ".join(f"{_scalar(key)}: {_scalar(value)}" for key, value in entry.items()) + " }" if entry else "{}"
    )
    if len(indent + collapsed + trailing) <= PRINT_WIDTH:
        return collapsed
    inner = indent + INDENT
    return (
        "{\n"
        + ",\n".join(f"{inner}{_scalar(key)}: {_scalar(value)}" for key, value in entry.items())
        + "\n"
        + indent
        + "}"
    )


def format_kinds(entries: list[dict], prefix: str, indent: str, trailing: str) -> str:
    """`entries` as prettier prints them after `prefix` (the line up to the
    value) and before `trailing` (the comma after it, if any)."""
    if not entries:
        return "[]"
    always_break = len(entries) > 1 and all(isinstance(entry, dict) and len(entry) > 1 for entry in entries)
    if not always_break:
        flat = "[" + ", ".join(_object_text(entry, "", "") for entry in entries) + "]"
        if len(prefix + flat + trailing) <= PRINT_WIDTH:
            return flat
    inner = indent + INDENT
    last = len(entries) - 1
    lines = [inner + _object_text(entry, inner, "," if index < last else "") for index, entry in enumerate(entries)]
    return "[\n" + ",\n".join(lines) + "\n" + indent + "]"


def _skip_whitespace(text: str, index: int) -> int:
    while index < len(text) and text[index] in " \t\r\n":
        index += 1
    return index


def _skip_string(text: str, index: int) -> int:
    index += 1
    while index < len(text):
        if text[index] == "\\":
            index += 2
        elif text[index] == '"':
            return index + 1
        else:
            index += 1
    raise ValueError("unterminated string")


def _skip_value(text: str, index: int) -> int:
    if text[index] == '"':
        return _skip_string(text, index)
    if text[index] in "{[":
        depth = 0
        while index < len(text):
            char = text[index]
            if char == '"':
                index = _skip_string(text, index)
                continue
            if char in "{[":
                depth += 1
            elif char in "}]":
                depth -= 1
                if depth == 0:
                    return index + 1
            index += 1
        raise ValueError("unterminated value")
    end = index
    while end < len(text) and text[end] not in ",}] \t\r\n":
        end += 1
    return end


def top_level_spans(text: str) -> list[tuple[str, int, int, int]]:
    """`(key, key_start, value_start, value_end)` for each key of the root
    object, in file order."""
    index = _skip_whitespace(text, 0)
    if index >= len(text) or text[index] != "{":
        raise ValueError("settings must be a JSON object")
    index += 1
    spans = []
    while True:
        index = _skip_whitespace(text, index)
        if index >= len(text):
            raise ValueError("unterminated object")
        if text[index] == "}":
            return spans
        if text[index] == ",":
            index += 1
            continue
        if text[index] != '"':
            raise ValueError(f"expected a key at offset {index}")
        key_start = index
        key_end = _skip_string(text, index)
        index = _skip_whitespace(text, key_end)
        if index >= len(text) or text[index] != ":":
            raise ValueError(f"expected ':' at offset {index}")
        value_start = _skip_whitespace(text, index + 1)
        value_end = _skip_value(text, value_start)
        spans.append((json.loads(text[key_start:key_end]), key_start, value_start, value_end))
        index = value_end


def _line_indent(text: str, key_start: int) -> str:
    line_start = text.rfind("\n", 0, key_start) + 1
    leading = text[line_start:key_start]
    return leading if leading.strip() == "" else INDENT


def _trailing(text: str, value_end: int) -> str:
    index = _skip_whitespace(text, value_end)
    return "," if index < len(text) and text[index] == "," else ""


def splice_catalog(text: str, key: str, entries: list[dict]) -> str:
    """`text` with the root's `key` replaced by `entries`; a missing key is
    added after `barrier_kinds`, or last."""
    spans = top_level_spans(text)
    for name, key_start, value_start, value_end in spans:
        if name == key:
            indent = _line_indent(text, key_start)
            prefix = indent + text[key_start:value_start]
            value = format_kinds(entries, prefix, indent, _trailing(text, value_end))
            return text[:value_start] + value + text[value_end:]
    anchor = next((span for span in spans if span[0] == "barrier_kinds"), spans[-1] if spans else None)
    if anchor is None:
        open_brace = text.index("{")
        close_brace = text.index("}", open_brace)
        prefix = f"{INDENT}{_scalar(key)}: "
        value = format_kinds(entries, prefix, INDENT, "")
        return text[: open_brace + 1] + "\n" + prefix + value + "\n" + text[close_brace:]
    _, key_start, _, value_end = anchor
    indent = _line_indent(text, key_start)
    prefix = f"{indent}{_scalar(key)}: "
    value = format_kinds(entries, prefix, indent, _trailing(text, value_end))
    return text[:value_end] + ",\n" + prefix + value + text[value_end:]


def splice_catalogs(text: str, catalogs: dict[str, list[dict]]) -> str:
    for key, entries in catalogs.items():
        text = splice_catalog(text, key, entries)
    return text
