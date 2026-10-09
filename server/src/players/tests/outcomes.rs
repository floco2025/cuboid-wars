use bevy::prelude::Entity;
use crossbeam_channel::unbounded;

use super::*;
use crate::players::PlayerInfo;
use common::protocol::PlayerGeneration;

const ID: PlayerId = PlayerId(1);

fn players() -> PlayerMap {
    let (tx, _) = unbounded();
    let mut players = PlayerMap::default();
    players.insert(ID, PlayerInfo::new(Entity::PLACEHOLDER, tx));
    players
}

fn report(players: &mut PlayerMap, generation: u32, event: MoveOutcome) {
    handle_move_outcome(
        ID,
        CMoveOutcome {
            generation: PlayerGeneration(generation),
            event,
        },
        players,
    );
}

fn outcomes(players: &PlayerMap) -> &PendingOutcomes {
    &players.get(&ID).expect("player missing").life.outcomes
}

#[test]
fn outcomes_survive_later_movement_but_not_body_replacement() {
    let mut players = players();
    players.get_mut(&ID).expect("player missing").session.last_move_seq = 1000;
    let landed = || MoveOutcome::Landed {
        pos: Position::default(),
        impact_speed: 8.0,
    };
    report(&mut players, 0, landed());
    assert_eq!(outcomes(&players).landings.len(), 1);
    players.get_mut(&ID).expect("player missing").advance_body();
    report(&mut players, 0, landed());
    assert!(outcomes(&players).landings.is_empty());
    assert_eq!(players.get(&ID).expect("player missing").session.last_move_seq, 1000);
    players.get_mut(&ID).expect("player missing").begin_respawn(1.0);
    report(&mut players, 1, MoveOutcome::FellOutOfWorld);
    assert!(!outcomes(&players).fell_out_of_world);
}

#[test]
fn a_brief_crush_and_eraser_pass_are_not_lost_between_ticks() {
    let mut players = players();
    for outcome in [
        MoveOutcome::Crushed {
            pos: Position::default(),
        },
        MoveOutcome::EraseEquipment,
        MoveOutcome::EraseEquipment,
    ] {
        report(&mut players, 0, outcome);
    }
    assert!(outcomes(&players).crushed.is_some());
    assert!(outcomes(&players).erase_equipment);
}

#[test]
fn malformed_outcomes_do_not_reach_gameplay_rules() {
    let mut players = players();
    let pos = Position {
        y: f32::INFINITY,
        ..Position::default()
    };
    for outcome in [
        MoveOutcome::Crushed { pos },
        MoveOutcome::Landed { pos, impact_speed: 8.0 },
        MoveOutcome::Landed {
            pos: Position::default(),
            impact_speed: f32::NAN,
        },
        MoveOutcome::Landed {
            pos: Position::default(),
            impact_speed: -1.0,
        },
    ] {
        report(&mut players, 0, outcome);
    }
    let outcomes = outcomes(&players);
    assert!(outcomes.landings.is_empty() && !outcomes.fell_out_of_world && outcomes.crushed.is_none());
}
