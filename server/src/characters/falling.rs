use bevy::math::FloatExt;

use crate::config::FallDamageConfig;

// Below this damage a landing is soft and both fall-damage systems skip it.
// The lerp produces near-zero damage just past `safe_distance` from float
// and tick noise; without this gate every tiny step off a curb would
// wiggle the player's camera and scratch an actor.
// Keep this cutoff in sync with tools/map_editor/jump_reach.py.
pub(crate) const FALL_DAMAGE_EMIT_THRESHOLD: f32 = 1.0;

// Express impact energy as a normal-gravity drop so map distance thresholds retain their meaning.
pub(crate) fn fall_distance_for_speed(impact_speed: f32, normal_gravity: f32) -> f32 {
    impact_speed * impact_speed / (2.0 * normal_gravity)
}

// Lerp damage between `safe_distance` (0 dmg) and `lethal_distance`
// (full health), clamping the falloff beyond the lethal endpoint.
// Keep this curve in sync with tools/map_editor/jump_reach.py::FallSettings.damage_fraction.
pub(crate) fn fall_damage_for_distance(distance: f32, fall: &FallDamageConfig, max_health: f32) -> f32 {
    f32::inverse_lerp(fall.safe_distance, fall.lethal_distance, distance).clamp(0.0, 1.0) * max_health
}

#[cfg(test)]
#[path = "tests/falling.rs"]
mod tests;
