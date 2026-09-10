use crate::{network::broadcast_to_others, players::PlayerMap};
use common::{config::ProjectilesConfig, protocol::*};

pub(crate) fn handle_projectile_shot_message(
    id: PlayerId,
    shot: CProjectileShot,
    players: &PlayerMap,
    config: &ProjectilesConfig,
) {
    if !shot.origin.is_finite()
        || !shot.face_yaw.is_finite()
        || !shot.face_pitch.is_finite()
        || config.multi_shot.shot_offsets(shot.pattern).is_none()
    {
        return;
    }
    broadcast_to_others(players, id, ServerMessage::ProjectileShot(SProjectileShot { id, shot }));
}
