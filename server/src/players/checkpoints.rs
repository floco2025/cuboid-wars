use bevy::prelude::*;
use rand::{RngExt, rng};

use super::PlayerMap;
use crate::{map::MapConfig, network::ServerToClient};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::CHARACTER_CONTACT_OFFSET,
    map::{CarrierPose, Carriers},
    math::direction_from_yaw_pitch,
    physics::{CharacterSupport, CollisionWorld, character_paths_intersect, grounding_diagnostics},
    protocol::{Checkpoint, FaceYaw, PlayerMarker, Position, SCheckpointReached, ServerMessage},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointId(pub usize);

#[derive(Debug, Clone, Copy)]
pub struct PlayerCheckpoint {
    pub id: CheckpointId,
    pub facing: Vec3,
}

pub(crate) fn players_checkpoints_system(
    mut players: ResMut<PlayerMap>,
    map: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    gameplay: Res<GameplayConfig>,
    positions: Query<(&Position, &FaceYaw), With<PlayerMarker>>,
) {
    if map.checkpoints.is_empty() {
        return;
    }
    for (_, player) in players.iter_mut() {
        if !player.connection.logged_in || player.life.fall_state.support() != CharacterSupport::Ground {
            continue;
        }
        let Some((pos, yaw)) = player.entity().and_then(|entity| positions.get(entity).ok()) else {
            continue;
        };
        let ground = grounding_diagnostics(
            &collision_world,
            pos,
            gameplay.player.physics(),
            &player.life.held_keys,
            &[],
        );
        let Some(hit) = ground.hit.filter(|_| ground.supported) else {
            continue;
        };
        let local = carriers.pose(hit.carrier).inverse_transform_position(pos);
        let Some((index, checkpoint)) = map
            .checkpoints
            .iter()
            .enumerate()
            .find(|(_, zone)| zone.carrier == hit.carrier && contains(zone, &local))
        else {
            continue;
        };
        let id = CheckpointId(index);
        if player.session.checkpoint.is_some_and(|saved| saved.id == id) {
            continue;
        }
        player.session.checkpoint = Some(PlayerCheckpoint {
            id,
            facing: carriers
                .pose(checkpoint.carrier)
                .inverse_transform_vector(direction_from_yaw_pitch(yaw.0, 0.0)),
        });
        let _ = player
            .connection
            .channel
            .send(ServerToClient::Send(ServerMessage::CheckpointReached(
                SCheckpointReached,
            )));
    }
}

fn contains(checkpoint: &Checkpoint, local: &Position) -> bool {
    (local.y - checkpoint.y).abs() <= CHARACTER_CONTACT_OFFSET * 5.0
        && local.x >= checkpoint.min_x
        && local.x < checkpoint.max_x
        && local.z >= checkpoint.min_z
        && local.z < checkpoint.max_z
}

pub(crate) fn checkpoint_spawn_position(
    checkpoint: &Checkpoint,
    pose: &CarrierPose,
    collision_world: &CollisionWorld,
    occupied: &[Position],
    physics: CharacterPhysicsConfig,
) -> Option<Position> {
    let radius = physics.movement_collider.radius();
    let min_x = checkpoint.min_x + radius;
    let max_x = checkpoint.max_x - radius;
    let min_z = checkpoint.min_z + radius;
    let max_z = checkpoint.max_z - radius;
    if min_x > max_x || min_z > max_z {
        return None;
    }
    let mut random = rng();
    for attempt in 0..100 {
        let local = Position {
            x: if attempt == 0 {
                (min_x + max_x) / 2.0
            } else {
                random.random_range(min_x..=max_x)
            },
            y: checkpoint.y,
            z: if attempt == 0 {
                (min_z + max_z) / 2.0
            } else {
                random.random_range(min_z..=max_z)
            },
        };
        let pos = pose.transform_position(&local);
        if collision_world.character_overlaps_solid(&pos, physics, &[])
            || occupied
                .iter()
                .any(|other| character_paths_intersect(&pos, &pos, physics, other, other, physics))
        {
            continue;
        }
        let ground = grounding_diagnostics(collision_world, &pos, physics, &[], &[]);
        if ground.supported && ground.hit.is_some_and(|hit| hit.carrier == checkpoint.carrier) {
            return Some(pos);
        }
    }
    None
}
