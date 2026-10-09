use crossbeam_channel::{Receiver, unbounded};

use super::*;
use crate::players::{PlayerInfo, PowerUpState, handle_move_outcome};
use common::protocol::{CMoveOutcome, FieldId, MoveOutcome, PlayerGeneration, PlayerId, PowerUpKind};

fn test_app() -> (App, Receiver<ServerMessage>) {
    let mut app = App::new();
    let (tx, rx) = unbounded();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.logged_in = true;
    let mut players = PlayerMap::default();
    players.insert(PlayerId(1), info);
    app.insert_resource(players).add_systems(Update, erase_equipment_system);
    (app, rx)
}

fn player(app: &mut App) -> &mut PlayerInfo {
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .into_inner()
        .get_mut(&PlayerId(1))
        .expect("player missing")
}

fn erase(app: &mut App) {
    handle_move_outcome(
        PlayerId(1),
        CMoveOutcome {
            generation: PlayerGeneration(0),
            event: MoveOutcome::EraseEquipment,
        },
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
}

#[test]
fn each_erase_command_erases_once_and_only_when_there_is_equipment() {
    let (mut app, rx) = test_app();
    player(&mut app).add_key(FieldId(0));
    for _ in 0..2 {
        erase(&mut app);
        app.update();
        assert!(rx.try_recv().is_err(), "keys alone are nothing to erase");
    }

    for with_power_ups in [false, true] {
        let info = player(&mut app);
        info.add_missiles(2, 3);
        if with_power_ups {
            info.life.power_ups.fill(PowerUpState::Permanent);
        }
        erase(&mut app);
        erase(&mut app);
        app.update();
        let messages: Vec<_> = rx.try_iter().collect();
        let cues = messages
            .iter()
            .filter(|message| matches!(message, ServerMessage::EquipmentErased(_)))
            .count();
        let statuses: Vec<_> = messages
            .into_iter()
            .filter_map(|message| match message {
                ServerMessage::PlayerStatus(status) => Some(status),
                _ => None,
            })
            .collect();
        assert_eq!(cues, 1);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].missiles, 0);
        let info = player(&mut app);
        assert!(PowerUpKind::ALL.into_iter().all(|kind| !info.has(kind)));
        assert_eq!(info.life.held_keys, [FieldId(0)]);
        assert_eq!(info.life.missiles, 0);
    }

    player(&mut app).add_missiles(2, 3);
    app.update();
    assert!(rx.try_recv().is_err(), "a handled command does not persist");
    assert_eq!(player(&mut app).life.missiles, 2);
}
