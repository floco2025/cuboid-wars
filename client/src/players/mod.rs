mod animation;
#[cfg(test)]
mod animation_tests;
mod components;
mod death;
mod effects;
mod movement;
mod resources;
mod spawn;
mod transform_sync;

pub use animation::PlayerAnimationMotion;
pub(crate) use animation::player_animation_update_system;
pub use components::{BumpFeedbackState, CameraShake, CuboidShake, PortalTransitBlend};
pub(crate) use components::{PlayerSample, RemotePlayerMotion};
pub use death::death_overlay_visibility_system;
pub use effects::{
    local_player_camera_shake_system, local_player_cuboid_shake_system, local_player_portal_blend_system,
};
pub use movement::{LocalMovementReports, report_player_movement_system};
pub(crate) use movement::{
    LocalMovementStep, PlayerMovementQuery, apply_player_moves, interpolate_remote_players_system, plan_player_moves,
    report_move_outcomes_system,
};
pub use resources::{LocalPlayerInfo, MyPlayerId, PlayerInfo, PlayerMap};
pub use spawn::{LocalPlayerMarker, PlayerSpawnContext, eye_position, spawn_player};
pub use transform_sync::players_transform_sync_system;
