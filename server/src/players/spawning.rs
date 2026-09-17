use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{Checkpoint, FaceYaw, Health, PlayerId, PlayerMoveIntent, PlayerMovementState, PortalAccess, Position},
};

use super::{CheckpointId, PlayerCheckpoint, PlayerMap, checkpoint_at_position};
use crate::{
    characters::{generate_checkpoint_spawn_position, spawn_face_yaw},
    map::MapConfig,
    network::broadcast_player_relocation,
};

pub(crate) struct PlayerSpawn {
    pub pos: Position,
    pub face_yaw: f32,
    pub contact: Option<CheckpointId>,
}

// Movement and placements update the retained state before ECS commands apply.
pub(crate) fn occupied_player_positions(players: &PlayerMap, carriers: &Carriers, except: PlayerId) -> Vec<Position> {
    players
        .iter()
        .filter(|(id, player)| **id != except && player.connection.logged_in && !player.is_dead())
        .map(|(_, player)| {
            let movement = &player.life.movement;
            carriers.pose(movement.carrier).transform_position(&movement.pos)
        })
        .collect()
}

// A clear spot at the saved checkpoint, `None` while every spot is blocked.
// The saved facing holds only in the rectangle it was saved in; a spot in
// another rectangle of the number faces the origin like a first spawn.
pub(crate) fn player_spawn_destination(
    map: &MapConfig,
    checkpoints: &[Checkpoint],
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied: &[Position],
    physics: CharacterPhysicsConfig,
    saved: PlayerCheckpoint,
) -> Option<PlayerSpawn> {
    let pos = generate_checkpoint_spawn_position(
        map,
        carriers,
        checkpoints,
        saved.number,
        collision_world,
        occupied,
        physics,
    )?;
    let contact = checkpoint_at_position(checkpoints, carriers, collision_world, &pos, physics, &[]);
    let face_yaw = match saved.entry {
        Some(entry) if Some(entry.id) == contact => {
            let facing = carriers
                .pose(checkpoints[entry.id.0].carrier)
                .transform_vector(entry.facing);
            facing.x.atan2(facing.z)
        }
        _ => spawn_face_yaw(&pos),
    };
    Some(PlayerSpawn { pos, face_yaw, contact })
}

// The start, for a body that must appear now; the origin, with a warning,
// when the start is blocked too.
pub(crate) fn start_destination(
    map: &MapConfig,
    checkpoints: &[Checkpoint],
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied: &[Position],
    physics: CharacterPhysicsConfig,
) -> PlayerSpawn {
    player_spawn_destination(
        map,
        checkpoints,
        carriers,
        collision_world,
        occupied,
        physics,
        PlayerCheckpoint::START,
    )
    .unwrap_or_else(|| {
        warn!("no clear spot at the start, spawning at the origin");
        let pos = Position::default();
        PlayerSpawn {
            pos,
            face_yaw: spawn_face_yaw(&pos),
            contact: None,
        }
    })
}

// Every body placement: login, respawn, the invincible void rescue, and
// `/return`. The caller has already established the body's generation and health.
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
    commands.entity(entity).insert((spawn.pos, FaceYaw(spawn.face_yaw)));
    if let Some(info) = players.get_mut(&id) {
        info.life.movement = movement;
        info.life.checkpoint_contact = spawn.contact;
    }
    broadcast_player_relocation(players, id, tick, movement, health, portal_access);
}
