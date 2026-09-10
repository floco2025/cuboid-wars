use common::protocol::{CPlayerMovementEvent, PlayerId, PlayerMovementEvent, Position};

use super::PlayerMap;

#[derive(Default)]
pub(crate) struct PlayerMovementEvents {
    pub landings: Vec<Landing>,
    pub crushed: Option<Position>,
    pub fell_out_of_world: bool,
    pub erase_equipment: bool,
}

pub(crate) struct Landing {
    pub pos: Position,
    pub impact_speed: f32,
}

pub(crate) fn handle_player_movement_event(id: PlayerId, message: CPlayerMovementEvent, players: &mut PlayerMap) {
    let Some(info) = players.get_mut(&id) else { return };
    if info.is_dead() || info.session.generation != message.generation {
        return;
    }
    let outcomes = &mut info.life.outcomes;
    match message.event {
        PlayerMovementEvent::Landed { pos, impact_speed }
            if finite(pos) && impact_speed.is_finite() && impact_speed >= 0.0 =>
        {
            outcomes.landings.push(Landing { pos, impact_speed });
        }
        PlayerMovementEvent::Crushed { pos } if finite(pos) => outcomes.crushed = Some(pos),
        PlayerMovementEvent::FellOutOfWorld => outcomes.fell_out_of_world = true,
        PlayerMovementEvent::EraseEquipment => outcomes.erase_equipment = true,
        _ => {}
    }
}

fn finite(pos: Position) -> bool {
    pos.x.is_finite() && pos.y.is_finite() && pos.z.is_finite()
}

#[cfg(test)]
mod tests {
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
        let event = CPlayerMovementEvent {
            generation: PlayerGeneration(0),
            event: PlayerMovementEvent::Landed {
                pos: Position::default(),
                impact_speed: 8.0,
            },
        };
        handle_player_movement_event(id, event.clone(), &mut players);
        assert_eq!(
            players.get(&id).expect("player missing").life.outcomes.landings.len(),
            1
        );
        players.get_mut(&id).expect("player missing").advance_body();
        handle_player_movement_event(id, event, &mut players);
        let info = players.get(&id).expect("player missing");
        assert!(info.life.outcomes.landings.is_empty());
        assert_eq!(info.session.last_move_seq, 1000);
        players.get_mut(&id).expect("player missing").begin_respawn(1.0);
        handle_player_movement_event(
            id,
            CPlayerMovementEvent {
                generation: PlayerGeneration(1),
                event: PlayerMovementEvent::FellOutOfWorld,
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
            PlayerMovementEvent::Crushed {
                pos: Position::default(),
            },
            PlayerMovementEvent::EraseEquipment,
            PlayerMovementEvent::EraseEquipment,
        ] {
            handle_player_movement_event(
                id,
                CPlayerMovementEvent {
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
            PlayerMovementEvent::Crushed { pos },
            PlayerMovementEvent::Landed { pos, impact_speed: 8.0 },
            PlayerMovementEvent::Landed {
                pos: Position::default(),
                impact_speed: f32::NAN,
            },
            PlayerMovementEvent::Landed {
                pos: Position::default(),
                impact_speed: -1.0,
            },
        ] {
            handle_player_movement_event(
                id,
                CPlayerMovementEvent {
                    generation: PlayerGeneration(0),
                    event: outcome,
                },
                &mut players,
            );
        }
        let outcomes = &players.get(&id).expect("player missing").life.outcomes;
        assert!(outcomes.landings.is_empty() && !outcomes.fell_out_of_world && outcomes.crushed.is_none());
    }
}
