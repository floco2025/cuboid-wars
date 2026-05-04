use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, try_start_player_jump},
    protocol::PlayerMoveIntent,
};

use super::types::LocalPlayerInputQuery;

pub(super) fn update_player_input_face_and_jump(
    move_intent: PlayerMoveIntent,
    face_yaw: f32,
    jump_requested: bool,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    local_player_query: &mut LocalPlayerInputQuery,
) {
    for (pos, mut input, mut face_direction, mut motion) in local_player_query.iter_mut() {
        *input = move_intent;
        face_direction.0 = face_yaw;
        if jump_requested && let Some(collision_world) = collision_world {
            let _ = try_start_player_jump(
                &mut motion.0,
                collision_world,
                gameplay_config.player.physics(),
                pos,
                pos.x,
                pos.z,
            );
        }
    }
}
