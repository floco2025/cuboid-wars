use common::{config::CharacterPhysicsConfig, map::Carriers, physics::CollisionWorld, protocol::Position};

use super::{CheckpointId, PlayerCheckpoint, checkpoint_at_position, checkpoint_spawn_position};
use crate::{
    characters::{generate_player_spawn_position, spawn_face_yaw},
    map::MapConfig,
};

pub(crate) struct PlayerSpawn {
    pub pos: Position,
    pub face_yaw: f32,
    pub contact: Option<CheckpointId>,
}

pub(crate) fn player_spawn_destination(
    map: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied: &[Position],
    physics: CharacterPhysicsConfig,
    saved: Option<PlayerCheckpoint>,
) -> Option<PlayerSpawn> {
    let (pos, face_yaw) = if let Some(saved) = saved {
        let checkpoint = &map.checkpoints[saved.id.0];
        let pose = carriers.pose(checkpoint.carrier);
        let pos = checkpoint_spawn_position(checkpoint, &pose, collision_world, occupied, physics)?;
        let facing = pose.transform_vector(saved.facing);
        (pos, facing.x.atan2(facing.z))
    } else {
        let pos = generate_player_spawn_position(map, carriers, collision_world, occupied, physics);
        (pos, spawn_face_yaw(&pos))
    };
    let contact = checkpoint_at_position(&map.checkpoints, carriers, collision_world, &pos, physics, &[]);
    Some(PlayerSpawn { pos, face_yaw, contact })
}
