use bevy::prelude::*;

// Level focus toggle (R key). When enabled, hides walls/floors at other levels
// and ramps that don't connect to the local player's level. Useful for
// inspecting one level without occluders.
#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug, Default)]
pub struct LevelFocusEnabled(pub bool);

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
