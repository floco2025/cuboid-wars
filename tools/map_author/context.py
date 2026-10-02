"""One map as the tools see it: its layout, catalogs, physics, and frame."""

from __future__ import annotations

from dataclasses import dataclass

from map_editor.catalogs import MapCatalogs, load_map_settings, map_layout_path, map_settings_path
from map_editor.io import read_map
from map_editor.jump_settings import JumpSettings

from .frame import GridFrame, PortalFootprint


@dataclass(frozen=True)
class MapContext:
    name: str
    data: dict
    catalogs: MapCatalogs
    settings: JumpSettings
    frame: GridFrame
    footprint: PortalFootprint

    @classmethod
    def load(cls, name: str, data: dict | None = None) -> MapContext:
        if data is None:
            data = read_map(map_layout_path(name))
        settings = JumpSettings.from_settings(load_map_settings(name), str(map_settings_path(name)))
        frame = GridFrame.for_map(name, data)
        footprint = PortalFootprint.for_map(name, frame)
        return cls(name, data, MapCatalogs.load(name).for_layout(data), settings, frame, footprint)

    @property
    def textures(self) -> dict[str, bool]:
        return self.catalogs.texture_catalog

    @property
    def level_count(self) -> int:
        return len(self.data["levels"])
