mod camera;
mod components;
mod death;
mod effects;
mod movement;
mod resources;
mod spawn;
mod transform_sync;

pub use camera::{
    local_player_camera_sync_system, local_player_rearview_sync_system, local_player_rearview_viewport_system,
    local_player_visibility_sync_system,
};
pub use components::{BumpFeedbackState, CameraShake, CuboidShake};
pub use death::death_overlay_visibility_system;
pub use effects::{local_player_camera_shake_system, local_player_cuboid_shake_system};
pub(crate) use movement::{PlayerMovementQuery, apply_player_moves, plan_player_moves};
pub use resources::{LocalPlayerInfo, MyPlayerId, PlayerInfo, PlayerMap};
pub use spawn::{LocalPlayerMarker, spawn_player};
pub use transform_sync::players_transform_sync_system;
