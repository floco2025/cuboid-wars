mod barriers;
mod characters;
mod lock;
mod portals;
mod projectiles;
mod world;

pub use barriers::passable_barrier_kinds;
pub use characters::{
    AirborneMomentum, BallCharacterHit, CharacterEnvironment, CharacterMovePlan, CharacterMovementResult,
    CharacterStep, CharacterSupport, CharacterVerticalVelocity, HitDirection, KnockbackVelocity, PlayerMovementStep,
    ball_character_hit, ball_overlaps_character, blocking_character_move_plan, character_center,
    character_move_plan_is_blocked, character_overlaps_item, character_paths_intersect, character_shape,
    character_vertical_ranges_overlap, knockback_decay_system, momentum_displacement, overlapping_character,
    player_control_velocity, player_jump_velocity, position_has_floor_support, step_character_movement,
    step_player_movement,
};
pub use lock::acquire_lock;
pub use portals::{
    CharacterPortalHop, PortalFrame, PortalPlacement, PortalSet, ProjectileHop, anchored_portals_refresh_system,
    compute_portal_placement, portal_placement_overlaps, traverse_move_intent, traverse_vector,
};
pub use projectiles::{
    BarrierImpact, ProjectileEvent, ProjectileMotion, ProjectileSpawnInfo, SurfaceBounce, calculate_projectile_spawns,
    earliest_projectile_event, projectile_character_hit, projectile_overlaps_character,
};
pub use world::{
    CollisionWorld, ShapeCastHit, WorldSurfaceHit, moving_floors_advance_system, powered_bridges_sync_system,
};
