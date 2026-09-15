"""Grid references shared by the editor's map operations."""

from dataclasses import dataclass


@dataclass(frozen=True)
class ZoneRef:
    list_name: str
    index: int
