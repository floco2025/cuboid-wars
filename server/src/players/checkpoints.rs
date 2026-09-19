use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use super::PlayerMap;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::CHARACTER_CONTACT_OFFSET,
    map::Carriers,
    math::direction_from_yaw_pitch,
    physics::{CharacterSupport, CollisionWorld, grounding_diagnostics, passable_fields},
    protocol::{
        Checkpoint, CheckpointKind, FaceYaw, FieldId, MapLayout, PlayerId, PlayerMarker, Position, SCheckpointReached,
        ServerMessage, ServerTick, SwitchState,
    },
};

// Index into `MapLayout.checkpoints`: one placed rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CheckpointId(pub usize);

// A player's respawn point: a checkpoint number, and the rectangle whose
// entry saved it with the facing then. A respawn lands in any rectangle of
// that number and keeps the facing only in the one entered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerCheckpoint {
    pub number: u32,
    pub entry: Option<CheckpointEntry>,
}

// In the rectangle's carrier frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckpointEntry {
    pub id: CheckpointId,
    pub facing: Vec3,
}

impl PlayerCheckpoint {
    // Where every player begins.
    pub const START: Self = Self { number: 0, entry: None };

    // A checkpoint set by number, for `--checkpoint` and `/checkpoint`.
    #[must_use]
    pub(crate) const fn numbered(number: u32) -> Self {
        Self { number, entry: None }
    }
}

impl Default for PlayerCheckpoint {
    fn default() -> Self {
        Self::START
    }
}

// The checkpoint a number refers to, or why there is none.
pub(crate) fn checkpoint_numbered(checkpoints: &[Checkpoint], number: u32) -> Result<PlayerCheckpoint, String> {
    if checkpoints.iter().any(|checkpoint| checkpoint.number == number) {
        return Ok(PlayerCheckpoint::numbered(number));
    }
    let numbers: BTreeSet<_> = checkpoints.iter().map(|checkpoint| checkpoint.number).collect();
    Err(format!(
        "unknown checkpoint {number}: the map's checkpoints are {}",
        numbers.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
    ))
}

// The furthest checkpoint any logged-in player has saved, dead players
// included; the course position actor spawn zones are gated on.
pub(crate) fn checkpoint_progress(players: &PlayerMap) -> u32 {
    players
        .values()
        .filter(|player| player.connection.logged_in)
        .map(|player| player.session.checkpoint.number)
        .max()
        .unwrap_or(0)
}

pub(crate) fn players_checkpoints_system(
    mut players: ResMut<PlayerMap>,
    map: Res<MapLayout>,
    tick: Res<ServerTick>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    switch_state: Res<SwitchState>,
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
                    &passable_fields(&player.life.held_keys, &switch_state.open_fields),
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
        if let Some(entered_id) = contact.filter(|contact| Some(*contact) != previous) {
            let (_, yaw) = position.expect("checkpoint contact missing player position");
            let checkpoint = &map.checkpoints[entered_id.0];
            entered.push((
                *id,
                PlayerCheckpoint {
                    number: checkpoint.number,
                    entry: Some(CheckpointEntry {
                        id: entered_id,
                        facing: carriers
                            .pose(checkpoint.carrier)
                            .inverse_transform_vector(direction_from_yaw_pitch(yaw.0, 0.0)),
                    }),
                },
            ));
        }
    }
    apply_checkpoint_entries(&mut players, &map.checkpoints, entered, tick.0);
}

// `entered` holds this tick's entries, each with its rectangle.
pub(super) fn apply_checkpoint_entries(
    players: &mut PlayerMap,
    checkpoints: &[Checkpoint],
    mut entered: Vec<(PlayerId, PlayerCheckpoint)>,
    tick: u32,
) {
    let previous: Vec<_> = players
        .iter()
        .filter(|(_, player)| player.connection.logged_in)
        .map(|(id, player)| (*id, player.session.checkpoint.number))
        .collect();
    entered.sort_by_key(|(player, saved)| (saved.number, saved.entry.map(|entry| entry.id), player.0));
    let mut shared_entries = BTreeMap::new();
    let mut any_entries = BTreeSet::new();
    let mut shared_entrants: BTreeMap<u32, Vec<PlayerId>> = BTreeMap::new();
    for (id, saved) in entered {
        let Some(player) = players.get_mut(&id).filter(|player| player.connection.logged_in) else {
            continue;
        };
        let entry = saved.entry.expect("checkpoint entry missing its rectangle");
        match checkpoints[entry.id.0].kind {
            // Progress only moves forward: a lower-numbered checkpoint is passed, not saved.
            CheckpointKind::Individual => {
                if player.session.checkpoint.number < saved.number {
                    player.session.checkpoint = saved;
                }
            }
            CheckpointKind::GroupAny => {
                any_entries.insert(saved.number);
                shared_entries.entry(saved.number).or_insert(saved);
                shared_entrants.entry(saved.number).or_default().push(id);
            }
            CheckpointKind::GroupAll => {
                player.session.checkpoint_visits.insert(saved.number, entry);
                shared_entries.entry(saved.number).or_insert(saved);
                shared_entrants.entry(saved.number).or_default().push(id);
            }
        }
    }
    let active = players.shared_checkpoint.number;
    let mut activated = None;
    // Course order across every carrier, whatever the compiled order: the
    // lowest shared number of the tick activates, the others follow. The
    // group's progress only moves forward too: re-entering the active shared
    // checkpoint or a lower one changes nothing, so individual saves, the
    // saved facing, and partial visits all stand.
    let shared_numbers: BTreeSet<u32> = checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.kind != CheckpointKind::Individual && checkpoint.number > active)
        .map(|checkpoint| checkpoint.number)
        .collect();
    for number in shared_numbers {
        let saved = if any_entries.contains(&number) {
            shared_entries.get(&number).copied()
        } else if checkpoints
            .iter()
            .any(|checkpoint| checkpoint.number == number && checkpoint.kind == CheckpointKind::GroupAll)
        {
            let visitors: Vec<_> = players
                .iter()
                .filter(|(_, player)| player.connection.logged_in)
                .collect();
            if !visitors
                .iter()
                .all(|(_, player)| player.session.checkpoint_visits.contains_key(&number))
            {
                continue;
            }
            visitors
                .into_iter()
                .min_by_key(|(player, _)| player.0)
                .map(|(_, player)| {
                    shared_entries.get(&number).copied().unwrap_or(PlayerCheckpoint {
                        number,
                        entry: Some(player.session.checkpoint_visits[&number]),
                    })
                })
        } else {
            None
        };
        let Some(saved) = saved else {
            continue;
        };
        players.shared_checkpoint = saved;
        for (_, player) in players.iter_mut().filter(|(_, player)| player.connection.logged_in) {
            if player.session.checkpoint.number < number {
                player.session.checkpoint = saved;
            }
            player.session.checkpoint_visits.clear();
        }
        activated = Some(number);
        break;
    }
    // One activation per tick: whoever entered a shared checkpoint further
    // along now enters it again next tick instead of standing there unnoticed.
    if let Some(activated) = activated {
        for entrant in shared_entrants
            .iter()
            .filter(|(number, _)| **number > activated)
            .flat_map(|(_, entrants)| entrants)
        {
            if let Some(player) = players.get_mut(entrant) {
                player.life.checkpoint_contact = None;
            }
        }
    }
    for (id, previous) in previous {
        let player = players.get(&id).expect("checkpoint recipient missing");
        let saved = player.session.checkpoint;
        if saved.number != previous {
            let _ = player
                .connection
                .channel
                .send(ServerMessage::CheckpointReached(SCheckpointReached {
                    checkpoint: saved.number,
                    tick,
                }));
        }
    }
}

pub(crate) fn checkpoint_at_position(
    checkpoints: &[Checkpoint],
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
    passable: &[FieldId],
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
