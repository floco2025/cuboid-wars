mod blast;
mod characters;
mod fields;
mod portals;
mod watchdog;
mod world;

pub use blast::{blast_falloff_at_distance, blast_hit};
pub use characters::{
    CharacterEnvironment, CharacterMovePlan, CharacterMovementResult, CharacterStart, CharacterStep, CharacterSupport,
    CharacterVerticalVelocity, GroundingDiagnostics, KnockbackVelocity, LadderMode, PlayerFlightPortals,
    PlayerFlightState, PlayerFlightTick, PlayerWish, accelerate_player, character_axis_separation,
    character_hitbox_center, character_hitbox_shape, character_move_plans_intersect, character_movement_center,
    character_movement_shape, character_passive_motion, character_paths_intersect, character_positions_intersect,
    fall_distance_for_speed, flight_funnel_prediction, grounding_diagnostics, knockback_decay_system, landing_damage,
    player_control_velocity, player_move_speed, player_wish_velocity, position_has_floor_support,
    step_character_movement, step_character_movement_from, step_player_flight,
};
pub use fields::passable_fields;
pub use portals::{
    CharacterHopBody, CharacterPortalHop, FunnelPrediction, FunnelStep, PortalFrame, PortalPlacement,
    PortalPlacementFailure, PortalSet, ProjectileHop, StraddledGate, compute_portal_placement, in_character_aperture,
    portal_placement_overlaps, portal_placement_yaw, traverse_move_intent, traverse_point, traverse_rotation,
    traverse_vector, traverse_yaw,
};
pub use watchdog::ProgressWatchdog;
pub use world::{
    CollisionMesh, CollisionSource, CollisionWorld, LadderVolume, ShapeCastHit, Solid, SolidFace, WorldSurfaceHit,
    carriers_advance_system,
};
