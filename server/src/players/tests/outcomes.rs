use super::*;
use crate::players::PlayerInfo;
use bevy::prelude::Entity;
use common::protocol::PlayerGeneration;
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn outcomes_survive_later_movement_but_not_body_replacement() {
    let id = PlayerId(1);
    let (tx, _) = unbounded_channel();
    let mut players = PlayerMap::default();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.session.last_move_seq = 1000;
    players.insert(id, info);
    let event = CMoveOutcome {
        generation: PlayerGeneration(0),
        event: MoveOutcome::Landed {
            pos: Position::default(),
            impact_speed: 8.0,
        },
    };
    handle_move_outcome(id, event.clone(), &mut players);
    assert_eq!(
        players.get(&id).expect("player missing").life.outcomes.landings.len(),
        1
    );
    players.get_mut(&id).expect("player missing").advance_body();
    handle_move_outcome(id, event, &mut players);
    let info = players.get(&id).expect("player missing");
    assert!(info.life.outcomes.landings.is_empty());
    assert_eq!(info.session.last_move_seq, 1000);
    players.get_mut(&id).expect("player missing").begin_respawn(1.0);
    handle_move_outcome(
        id,
        CMoveOutcome {
            generation: PlayerGeneration(1),
            event: MoveOutcome::FellOutOfWorld,
        },
        &mut players,
    );
    assert!(
        !players
            .get(&id)
            .expect("player missing")
            .life
            .outcomes
            .fell_out_of_world
    );
}

#[test]
fn a_brief_crush_and_eraser_pass_are_not_lost_between_ticks() {
    let id = PlayerId(1);
    let (tx, _) = unbounded_channel();
    let mut players = PlayerMap::default();
    players.insert(id, PlayerInfo::new(Entity::PLACEHOLDER, tx));
    for outcome in [
        MoveOutcome::Crushed {
            pos: Position::default(),
        },
        MoveOutcome::EraseEquipment,
        MoveOutcome::EraseEquipment,
    ] {
        handle_move_outcome(
            id,
            CMoveOutcome {
                generation: PlayerGeneration(0),
                event: outcome,
            },
            &mut players,
        );
    }
    let outcomes = &players.get(&id).expect("player missing").life.outcomes;
    assert!(outcomes.crushed.is_some());
    assert!(outcomes.erase_equipment);
}

#[test]
fn malformed_outcomes_do_not_reach_gameplay_rules() {
    let id = PlayerId(1);
    let (tx, _) = unbounded_channel();
    let mut players = PlayerMap::default();
    players.insert(id, PlayerInfo::new(Entity::PLACEHOLDER, tx));
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
        handle_move_outcome(
            id,
            CMoveOutcome {
                generation: PlayerGeneration(0),
                event: outcome,
            },
            &mut players,
        );
    }
    let outcomes = &players.get(&id).expect("player missing").life.outcomes;
    assert!(outcomes.landings.is_empty() && !outcomes.fell_out_of_world && outcomes.crushed.is_none());
}
