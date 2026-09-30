use bevy_math::FloatExt;

use crate::config::FallDamageConfig;

// Below this damage a landing is soft and both fall-damage systems skip it.
// The lerp produces near-zero damage just past `safe_distance` from float
// and tick noise; without this gate every tiny step off a curb would
// wiggle the player's camera and scratch an actor.
pub const FALL_DAMAGE_EMIT_THRESHOLD: f32 = 1.0;

// Express impact energy as a normal-gravity drop so map distance thresholds retain their meaning.
#[must_use]
pub fn fall_distance_for_speed(impact_speed: f32, normal_gravity: f32) -> f32 {
    impact_speed * impact_speed / (2.0 * normal_gravity)
}

// Lerp damage between `safe_distance` (0 dmg) and `lethal_distance`
// (full health), clamping the falloff beyond the lethal endpoint.
#[must_use]
pub fn fall_damage_for_distance(distance: f32, fall: &FallDamageConfig, max_health: f32) -> f32 {
    f32::inverse_lerp(fall.safe_distance, fall.lethal_distance, distance).clamp(0.0, 1.0) * max_health
}

// The damage a landing deals, or `None` for a soft landing.
#[must_use]
pub fn landing_damage(impact_speed: f32, normal_gravity: f32, fall: &FallDamageConfig, max_health: f32) -> Option<f32> {
    let damage = fall_damage_for_distance(fall_distance_for_speed(impact_speed, normal_gravity), fall, max_health);
    (damage >= FALL_DAMAGE_EMIT_THRESHOLD).then_some(damage)
}
