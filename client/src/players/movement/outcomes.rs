use bevy::prelude::*;
use common::{
    config::{GameplayConfig, NetworkConfig, UpdateCadence},
    constants::CHARACTER_FALL_DEATH_Y,
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{CMoveOutcome, CarrierId, ClientMessage, MoveOutcome, Position},
};

use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

// What the motor decided this tick, for the systems that report it.
#[derive(Component)]
pub struct LocalMovementStep {
    pub(crate) start: Position,
    pub(crate) crushed: bool,
    pub(crate) impact_speed: f32,
    pub(crate) carrier: CarrierId,
    pub(crate) support: CharacterSupport,
}

pub(crate) fn report_move_outcomes_system(
    to_server: Res<ClientToServerChannel>,
    mut local: ResMut<LocalPlayerInfo>,
    collision: Res<CollisionWorld>,
    carriers: Res<Carriers>,
    gameplay: Res<GameplayConfig>,
    network: Res<NetworkConfig>,
    mut eraser_cadence: Local<Option<UpdateCadence>>,
    query: Query<(&Position, &LocalMovementStep), With<LocalPlayerMarker>>,
) {
    if local.is_dead {
        return;
    }
    let Ok((pos, step)) = query.single() else {
        return;
    };
    let reports = &mut local.reports;
    let generation = reports.generation;
    let crossing = reports.crossing_entrance.as_ref();
    let sweep_end = crossing.copied().unwrap_or(*pos);
    let physics = gameplay.player.physics();
    let touching = collision
        .character_eraser_contacts(pos, pos, physics, Some(&carriers))
        .next()
        .is_some();
    let swept = collision
        .character_eraser_contacts(&step.start, &sweep_end, physics, Some(&carriers))
        .next()
        .is_some();
    let send = |outcome| {
        to_server.send(ClientToServer::Send(ClientMessage::MoveOutcome(CMoveOutcome {
            generation,
            event: outcome,
        })))
    };
    // A pickup update can arrive after contact, so the client inventory cannot
    // gate erasure: standing in a field keeps reporting, at the movement
    // cadence, and entry restarts that cadence so it is reported at once.
    let contact = touching || swept;
    let erase_due = contact && eraser_cadence.get_or_insert_with(|| network.update_cadence()).ready();
    if !contact {
        *eraser_cadence = None;
    }
    if erase_due {
        send(MoveOutcome::EraseEquipment);
    }
    if crossing.is_none() && step.impact_speed > 0.0 {
        send(MoveOutcome::Landed {
            pos: *pos,
            impact_speed: step.impact_speed,
        });
    }
    if crossing.is_none() && step.crushed {
        send(MoveOutcome::Crushed { pos: *pos });
    }
    if pos.y < CHARACTER_FALL_DEATH_Y && !reports.void_reported {
        reports.void_reported = true;
        send(MoveOutcome::FellOutOfWorld);
    }
}

#[cfg(test)]
#[path = "tests/outcomes.rs"]
mod tests;
