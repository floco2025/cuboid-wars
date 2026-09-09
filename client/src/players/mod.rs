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
pub use death::death_overlay_visibility_system;
pub use effects::{
    local_player_camera_shake_system, local_player_cuboid_shake_system, local_player_portal_blend_system,
};
pub(crate) use movement::{PlayerMovementQuery, apply_player_moves, plan_player_moves};
pub use resources::{CommittedPositionRing, CrossingVerdict, LocalPlayerInfo, MyPlayerId, PlayerInfo, PlayerMap};
pub use spawn::{LocalPlayerMarker, PlayerSpawnContext, eye_position, spawn_player};
pub use transform_sync::players_transform_sync_system;
