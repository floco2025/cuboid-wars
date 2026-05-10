use bevy::prelude::*;

use crate::config::DebugColorMode;

// Level focus toggle (R key). When enabled, hides walls/floors at other levels
// and ramps that don't connect to the local player's level. Useful for
// inspecting one level without occluders.
#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug, Default)]
pub struct LevelFocusEnabled(pub bool);

// Debug colors mode. Cycled at runtime via the C key. The map mesh is
// re-spawned whenever this changes.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugColors(pub DebugColorMode);
