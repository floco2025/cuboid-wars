use bevy::prelude::*;
use common::{
    config::MapGeometryConfig,
    map::MapGeometry,
    protocol::{CarrierGrid, MapLayout},
};

// Level focus toggle (R key). When enabled, hides walls/floors at other levels
// and ramps that don't connect to the local player's level. Useful for
// inspecting one level without occluders.
#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug, Default)]
pub struct LevelFocusEnabled(pub bool);

#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug, Default)]
pub struct FocusedMapLevel(pub(crate) Option<u8>);

// Map debug-color mode. Not configurable: every client starts `Off` and the
// C key cycles it at runtime. The map mesh is re-spawned whenever it changes.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugColors(pub DebugColorMode);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugColorMode {
    // Real materials (textures from `assets.json`).
    #[default]
    Off,
    // One color per material name (deterministic hash → HSV).
    ByMaterial,
    // One color per record sent in `MapLayout` (random per batch).
    BySegment,
}

impl DebugColorMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::ByMaterial,
            Self::ByMaterial => Self::BySegment,
            Self::BySegment => Self::Off,
        }
    }
}

// The map's authored volume in metres: the root grid's footprint, centred
// on the world origin, and the height of the tallest storey any carrier
// reaches at either end of its motion, from y = 0.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MapDimensions {
    pub width: f32,
    pub depth: f32,
    pub height: f32,
}

impl MapDimensions {
    #[must_use]
    pub fn from_grids(layout: &MapLayout, grids: &[CarrierGrid], sizes: MapGeometryConfig) -> Self {
        let root = grids
            .iter()
            .find(|grid| grid.carrier.is_world())
            .expect("world grid missing from the map bootstrap");
        let geometry = MapGeometry::new(root.cols, root.rows, sizes);
        let storeys = grids
            .iter()
            .map(|grid| {
                layout
                    .carrier_base_level(grid.carrier)
                    .saturating_add(layout.carrier_motion_levels(grid.carrier))
                    .saturating_add(grid.levels)
            })
            .max()
            .unwrap_or(0);
        Self {
            width: geometry.width(),
            depth: geometry.depth(),
            height: sizes.level_y(storeys),
        }
    }
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
