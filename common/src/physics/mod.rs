mod barriers;
mod characters;
mod lock;
mod projectiles;
mod world;

pub use barriers::{OpenBarrierKinds, passable_barrier_kinds};
pub use characters::{
    BallCharacterHit, CharacterEnvironment, CharacterMovePlan, CharacterMovementResult, CharacterStep,
    CharacterSupport, CharacterVerticalVelocity, HitDirection, KnockbackVelocity, ball_character_hit,
    ball_overlaps_character, blocking_character_move_plan, character_center, character_move_plan_is_blocked,
    character_overlaps_item, character_paths_intersect, character_shape, overlapping_character,
    player_control_velocity, player_jump_velocity, position_has_floor_support, step_character_movement,
};
pub use lock::acquire_lock;
pub use projectiles::{
    BarrierImpact, ProjectileMotion, ProjectileSpawnInfo, WorldBounces, calculate_projectile_spawns,
    projectile_character_hit, projectile_overlaps_character,
};
pub use world::{CollisionWorld, ShapeCastHit, WorldSurfaceHit};
