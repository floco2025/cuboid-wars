use bevy::prelude::*;
use common::protocol::{SEquipmentErased, ServerMessage};

use super::PlayerMap;
use crate::network::{ServerToClient, broadcast_to_all};

pub fn erase_equipment_system(mut players: ResMut<PlayerMap>) {
    let mut statuses = Vec::new();
    for (id, info) in players.iter_mut() {
        if info.is_dead() {
            continue;
        }
        let erase = std::mem::take(&mut info.life.outcomes.erase_equipment);
        let erased = erase && info.erase_equipment();
        if erased && info.connection.logged_in {
            let _ = info
                .connection
                .channel
                .send(ServerToClient::Send(ServerMessage::EquipmentErased(SEquipmentErased)));
        }
        if erased {
            statuses.push(info.status(*id));
        }
    }
    for status in statuses {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::{PlayerInfo, PowerUpState, handle_move_outcome};
    use common::protocol::{BarrierKindId, CMoveOutcome, MoveOutcome, PlayerGeneration, PlayerId, PowerUpKind};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn test_app() -> (App, UnboundedReceiver<ServerToClient>) {
        let mut app = App::new();
        let (tx, rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.connection.logged_in = true;
        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), info);
        app.insert_resource(players).add_systems(Update, erase_equipment_system);
        (app, rx)
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

    fn erasure_cues(rx: &mut UnboundedReceiver<ServerToClient>) -> usize {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|message| matches!(message, ServerToClient::Send(ServerMessage::EquipmentErased(_))))
            .count()
    }

    #[test]
    fn empty_inventory_and_repeated_commands_stay_silent() {
        let (mut app, mut rx) = test_app();
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&PlayerId(1))
            .expect("player missing")
            .add_key(BarrierKindId(0));
        for _ in 0..3 {
            erase(&mut app);
            app.update();
            assert!(rx.try_recv().is_err());
        }
    }

    #[test]
    fn missile_ammo_alone_is_erased_and_broadcast_once() {
        let (mut app, mut rx) = test_app();
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&PlayerId(1))
            .expect("player missing")
            .add_missiles(2, 3);
        erase(&mut app);
        erase(&mut app);
        app.update();
        let messages: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(message, ServerToClient::Send(ServerMessage::EquipmentErased(_))))
                .count(),
            1
        );
        let statuses: Vec<_> = messages
            .into_iter()
            .filter_map(|message| match message {
                ServerToClient::Send(ServerMessage::PlayerStatus(status)) => Some(status),
                _ => None,
            })
            .collect();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].missiles, 0);
        erase(&mut app);
        app.update();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn one_command_erases_a_fast_pass_and_does_not_persist() {
        let (mut app, mut rx) = test_app();
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&PlayerId(1))
            .expect("player missing")
            .life
            .power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Permanent;
        erase(&mut app);
        app.update();
        assert_eq!(erasure_cues(&mut rx), 1);
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&PlayerId(1))
            .expect("player missing")
            .add_missiles(2, 3);
        app.update();
        assert_eq!(erasure_cues(&mut rx), 0);
        assert_eq!(
            app.world()
                .resource::<PlayerMap>()
                .get(&PlayerId(1))
                .expect("player missing")
                .life
                .missiles,
            2
        );
    }

    #[test]
    fn fresh_commands_erase_new_equipment_and_play_feedback_while_preserving_keys() {
        let (mut app, mut rx) = test_app();
        for _ in 0..2 {
            {
                let mut players = app.world_mut().resource_mut::<PlayerMap>();
                let info = players.get_mut(&PlayerId(1)).expect("player missing");
                info.add_key(BarrierKindId(0));
                info.life.missiles = 2;
                info.life.power_ups.fill(PowerUpState::Permanent);
            }
            erase(&mut app);
            app.update();
            let players = app.world().resource::<PlayerMap>();
            let info = players.get(&PlayerId(1)).expect("player missing");
            assert!(PowerUpKind::ALL.into_iter().all(|kind| !info.has(kind)));
            assert_eq!(info.life.held_keys, [BarrierKindId(0)]);
            assert_eq!(info.life.missiles, 0);
            assert_eq!(erasure_cues(&mut rx), 1);
        }
    }
}
