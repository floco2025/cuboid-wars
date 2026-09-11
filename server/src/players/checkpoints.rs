use std::collections::BTreeMap;

use bevy::prelude::*;
use rand::RngExt;

use super::PlayerMap;
use crate::characters::sample_clear_position;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::CHARACTER_CONTACT_OFFSET,
    map::{CarrierPose, Carriers},
    math::direction_from_yaw_pitch,
    physics::{CharacterSupport, CollisionWorld, grounding_diagnostics},
    protocol::{
        BarrierId, Checkpoint, CheckpointKind, FaceYaw, MapLayout, PlayerId, PlayerMarker, Position,
        SCheckpointReached, ServerMessage,
    },
};

// Index into `MapLayout.checkpoints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CheckpointId(pub usize);

pub(crate) fn checkpoints_exist(layout: Res<MapLayout>) -> bool {
    !layout.checkpoints.is_empty()
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerCheckpoint {
    pub id: CheckpointId,
    pub facing: Vec3,
}

pub(crate) fn players_checkpoints_system(
    mut players: ResMut<PlayerMap>,
    map: Res<MapLayout>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    gameplay: Res<GameplayConfig>,
    positions: Query<(&Position, &FaceYaw), With<PlayerMarker>>,
) {
    let mut entered = Vec::new();
    for (id, player) in players.iter_mut().filter(|(_, player)| player.connection.logged_in) {
        let position = player.entity().and_then(|entity| positions.get(entity).ok());
        let contact = position
            .filter(|_| player.life.movement.support == CharacterSupport::Ground)
            .and_then(|(pos, _)| {
                checkpoint_at_position(
                    &map.checkpoints,
                    &carriers,
                    &collision_world,
                    pos,
                    gameplay.player.physics(),
                    &collision_world.passable_barriers(&player.life.held_keys, &[]),
                )
            });
        // A fresh body has no movement support yet; keep its seeded contact until it leaves the zone.
        let contact = contact.or_else(|| {
            player.life.checkpoint_contact.filter(|id| {
                let checkpoint = &map.checkpoints[id.0];
                position.is_some_and(|(pos, _)| {
                    contains(
                        checkpoint,
                        &carriers.pose(checkpoint.carrier).inverse_transform_position(pos),
                    )
                })
            })
        });
        let previous = std::mem::replace(&mut player.life.checkpoint_contact, contact);
        if let Some(checkpoint) = contact.filter(|contact| Some(*contact) != previous) {
            let (_, yaw) = position.expect("checkpoint contact missing player position");
            entered.push((
                *id,
                PlayerCheckpoint {
                    id: checkpoint,
                    facing: carriers
                        .pose(map.checkpoints[checkpoint.0].carrier)
                        .inverse_transform_vector(direction_from_yaw_pitch(yaw.0, 0.0)),
                },
            ));
        }
    }
    apply_checkpoint_entries(&mut players, &map.checkpoints, entered);
}

pub(super) fn apply_checkpoint_entries(
    players: &mut PlayerMap,
    checkpoints: &[Checkpoint],
    mut entered: Vec<(PlayerId, PlayerCheckpoint)>,
) {
    let previous: Vec<_> = players
        .iter()
        .filter(|(_, player)| player.connection.logged_in)
        .map(|(id, player)| (*id, player.session.checkpoint.map(|checkpoint| checkpoint.id)))
        .collect();
    entered.sort_by_key(|(player, checkpoint)| (checkpoint.id, player.0));
    let mut shared_entries = BTreeMap::new();
    let mut shared_entrants: BTreeMap<CheckpointId, Vec<PlayerId>> = BTreeMap::new();
    for (id, saved) in entered {
        let Some(player) = players.get_mut(&id).filter(|player| player.connection.logged_in) else {
            continue;
        };
        match checkpoints[saved.id.0].kind {
            CheckpointKind::Individual => player.session.checkpoint = Some(saved),
            CheckpointKind::GroupAny => {
                shared_entries.entry(saved.id).or_insert(saved);
                shared_entrants.entry(saved.id).or_default().push(id);
            }
            CheckpointKind::GroupAll => {
                player.session.checkpoint_visits.insert(saved.id, saved.facing);
                shared_entries.entry(saved.id).or_insert(saved);
                shared_entrants.entry(saved.id).or_default().push(id);
            }
        }
    }
    let active = players.shared_checkpoint.map(|checkpoint| checkpoint.id);
    let mut activated = None;
    for (index, checkpoint) in checkpoints.iter().enumerate() {
        let id = CheckpointId(index);
        // Re-entering the active shared checkpoint changes nothing: individual
        // saves, the saved facing, and partial visits all stand.
        if active == Some(id) {
            continue;
        }
        let saved = match checkpoint.kind {
            CheckpointKind::Individual => continue,
            CheckpointKind::GroupAny => shared_entries.get(&id).copied(),
            CheckpointKind::GroupAll => {
                let visitors: Vec<_> = players
                    .iter()
                    .filter(|(_, player)| player.connection.logged_in)
                    .collect();
                if !visitors
                    .iter()
                    .all(|(_, player)| player.session.checkpoint_visits.contains_key(&id))
                {
                    continue;
                }
                visitors
                    .into_iter()
                    .min_by_key(|(player, _)| player.0)
                    .map(|(_, player)| {
                        shared_entries.get(&id).copied().unwrap_or(PlayerCheckpoint {
                            id,
                            facing: player.session.checkpoint_visits[&id],
                        })
                    })
            }
        };
        let Some(saved) = saved else {
            continue;
        };
        players.shared_checkpoint = Some(saved);
        for (_, player) in players.iter_mut().filter(|(_, player)| player.connection.logged_in) {
            player.session.checkpoint = Some(saved);
            player.session.checkpoint_visits.clear();
        }
        activated = Some(id);
        break;
    }
    // One activation per tick: whoever entered another shared checkpoint now
    // enters it again next tick instead of standing there unnoticed. A
    // re-entry of the checkpoint that was active is not an entry to repeat,
    // or it would win the next tick and roll everyone back.
    if let Some(activated) = activated {
        for entrant in shared_entrants
            .iter()
            .filter(|(id, _)| **id != activated && Some(**id) != active)
            .flat_map(|(_, entrants)| entrants)
        {
            if let Some(player) = players.get_mut(entrant) {
                player.life.checkpoint_contact = None;
            }
        }
    }
    for (id, previous) in previous {
        let player = players.get(&id).expect("checkpoint recipient missing");
        if player.session.checkpoint.map(|checkpoint| checkpoint.id) != previous {
            let _ = player
                .connection
                .channel
                .send(ServerMessage::CheckpointReached(SCheckpointReached));
        }
    }
}

pub(crate) fn checkpoint_at_position(
    checkpoints: &[Checkpoint],
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
    passable: &[BarrierId],
) -> Option<CheckpointId> {
    let ground = grounding_diagnostics(collision_world, pos, physics, passable, &[]);
    let hit = ground.hit.filter(|_| ground.supported)?;
    let local = carriers.pose(hit.carrier).inverse_transform_position(pos);
    checkpoints
        .iter()
        .position(|checkpoint| checkpoint.carrier == hit.carrier && contains(checkpoint, &local))
        .map(CheckpointId)
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
    // The center first, then anywhere in the rectangle; a spot is clear of
    // every solid and stands on the checkpoint's own carrier.
    sample_clear_position(
        occupied,
        physics,
        |attempt, rng| {
            let local = Position {
                x: if attempt == 0 {
                    (min_x + max_x) / 2.0
                } else {
                    rng.random_range(min_x..=max_x)
                },
                y: checkpoint.y,
                z: if attempt == 0 {
                    (min_z + max_z) / 2.0
                } else {
                    rng.random_range(min_z..=max_z)
                },
            };
            Some(pose.transform_position(&local))
        },
        |pos| {
            if collision_world.character_overlaps_solid(pos, physics, &[]) {
                return false;
            }
            let ground = grounding_diagnostics(collision_world, pos, physics, &[], &[]);
            ground.supported && ground.hit.is_some_and(|hit| hit.carrier == checkpoint.carrier)
        },
    )
}
