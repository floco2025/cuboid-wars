mod characters;
mod items;
mod projectiles;
mod world;

pub use characters::{
    CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity, blocking_character_move_plan,
    character_move_plan_is_blocked, character_move_plans_intersect, character_paths_intersect, overlapping_character,
    overlaps_other_character, position_has_floor_support, step_character_movement, try_start_player_jump,
};
pub use items::character_overlaps_item;
pub use projectiles::{
    HitDirection, ProjectileCharacterHit, ProjectileMarker, ProjectileMotion, ProjectileSpawnInfo,
    calculate_projectile_spawns, projectile_character_hit, projectile_hits_character,
};
pub use world::CollisionWorld;

use crate::protocol::BarrierKindId;

// Merge per-player `held_keys` with the globally `open_kinds` (currently held
// open by pressure plates) into the slice that `step_character_movement`
// treats as "barriers I can pass through". One source of truth used by both
// server-authoritative movement and client-side prediction — keeps the two
// sides in agreement about what's passable.
#[must_use]
pub fn passable_barrier_kinds(held_keys: &[BarrierKindId], open_kinds: &[BarrierKindId]) -> Vec<BarrierKindId> {
    if open_kinds.is_empty() {
        return held_keys.to_vec();
    }
    let mut combined: Vec<BarrierKindId> = held_keys.to_vec();
    for k in open_kinds {
        if !combined.contains(k) {
            combined.push(*k);
        }
    }
    combined
}
