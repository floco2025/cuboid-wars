mod barriers;
mod characters;
mod lock;
mod portals;
mod projectiles;
mod world;

pub use barriers::passable_barrier_kinds;
pub use characters::{
    AirborneMomentum, BallCharacterHit, CARRIER_RIDE_TOLERANCE, CharacterEnvironment, CharacterMovePlan,
    CharacterMovementResult, CharacterStep, CharacterSupport, CharacterVerticalVelocity, GroundingDiagnostics,
    HitDirection, KnockbackVelocity, LadderMode, PlayerMovementStep, ball_character_hit, ball_overlaps_character,
    blocking_character_move_plan, character_hitbox_center, character_hitbox_shape, character_move_plan_is_blocked,
    character_movement_center, character_movement_pose, character_movement_shape, character_overlaps_item,
    character_paths_intersect, character_surface_distance, grounding_diagnostics, knockback_decay_system,
    momentum_displacement, overlapping_character, player_control_velocity, player_jump_velocity,
    position_has_floor_support, step_character_movement, step_player_movement,
};
pub use lock::acquire_lock;
pub use portals::{
    CharacterPortalHop, PortalFrame, PortalPlacement, PortalPlacementFailure, PortalSet, ProjectileHop,
    carried_portals_refresh_system, compute_portal_placement, portal_placement_overlaps, traverse_move_intent,
    traverse_vector,
};
pub use projectiles::{
    FieldImpact, PROJECTILE_EVENT_LIMIT, ProjectileEvent, ProjectileMotion, ProjectileSpawnInfo, SurfaceBounce,
    calculate_projectile_spawns, earliest_projectile_event, projectile_character_hit, projectile_overlaps_character,
};
pub use world::{
    CollisionWorld, FieldKind, LadderVolume, ShapeCastHit, WorldSurfaceHit, carriers_advance_system,
    powered_bridges_sync_system,
};
