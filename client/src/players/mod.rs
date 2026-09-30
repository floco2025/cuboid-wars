mod animation;
#[cfg(test)]
#[path = "tests/animation.rs"]
mod animation_tests;
mod components;
mod crouch;
mod death;
mod effects;
mod footsteps;
mod movement;
mod resources;
mod spawn;
mod transform_sync;

pub use animation::PlayerAnimationMotion;
pub(crate) use animation::{PlayerAnimationPlayback, player_animation_update_system};
pub use components::{BumpFeedbackState, CameraShake, CuboidShake, PortalTransitBlend};
pub(crate) use components::{PlayerSample, RemotePlayerMotion};
pub use crouch::CrouchBlend;
pub(crate) use crouch::crouch_blend_system;
pub use death::death_overlay_visibility_system;
pub use effects::{
    local_player_camera_shake_system, local_player_cuboid_shake_system, local_player_portal_blend_system,
};
pub(crate) use footsteps::footsteps_plugin;
pub use movement::{
    JumpRequested, LocalMovementReports, LocalMovementStep, OwnerBody, OwnerTickOutcome, OwnerWorld, PlayerMove,
    collect_move_outcomes, ladder_facing, owner_tick, plan_player_move,
};
pub(crate) use movement::{
    interpolate_remote_players_system, local_player_feedback_system, local_player_movement_system,
};
pub use resources::{LocalPlayerInfo, MyPlayerId, PlayerInfo, PlayerMap};
pub use spawn::PlayerMotionBundle;
pub use spawn::{LocalPlayerMarker, PlayerSpawnContext, eye_position, spawn_player};
pub use transform_sync::players_transform_sync_system;
