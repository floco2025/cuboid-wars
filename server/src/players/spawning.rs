use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{FaceYaw, Health, PlayerId, PlayerMoveIntent, PlayerMovementState, PortalAccess, Position},
};

use super::{CheckpointId, PlayerCheckpoint, PlayerMap, checkpoint_at_position, checkpoint_spawn_position};
use crate::{
    characters::{generate_player_spawn_position, spawn_face_yaw},
    map::MapConfig,
    network::broadcast_player_relocation,
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

impl PlayerSpawn {
    pub(crate) fn without_checkpoint(pos: Position) -> Self {
        Self {
            pos,
            face_yaw: spawn_face_yaw(&pos),
            contact: None,
        }
    }
}

// Every body placement: login, respawn, and the invincible void rescue.
// The caller has already established the body's generation.
#[expect(
    clippy::too_many_arguments,
    reason = "one placement threads the body, its spawn, and the cue"
)]
pub(crate) fn place_player_body(
    commands: &mut Commands,
    players: &mut PlayerMap,
    id: PlayerId,
    entity: Entity,
    spawn: &PlayerSpawn,
    health: Health,
    tick: u32,
    portal_access: PortalAccess,
) {
    let movement = PlayerMovementState::new(spawn.pos, PlayerMoveIntent::Idle, 0.0, spawn.face_yaw);
    commands
        .entity(entity)
        .insert((spawn.pos, FaceYaw(spawn.face_yaw), health));
    if let Some(info) = players.get_mut(&id) {
        info.life.movement = movement;
        info.life.checkpoint_contact = spawn.contact;
    }
    broadcast_player_relocation(players, id, tick, movement, health, portal_access);
}
