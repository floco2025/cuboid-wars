use std::collections::HashSet;

use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{PlayerId, PlayerMarker, Position, SEraserEntered, ServerMessage},
};

use super::PlayerMap;
use crate::network::{ServerToClient, broadcast_to_all};

#[derive(Resource, Default)]
pub struct EraserContacts {
    pub swept: HashSet<(PlayerId, usize)>,
    occupied: HashSet<(PlayerId, usize)>,
}

pub fn erase_power_ups_system(
    mut contacts: ResMut<EraserContacts>,
    mut players: ResMut<PlayerMap>,
    positions: Query<&Position, With<PlayerMarker>>,
    collision_world: Res<CollisionWorld>,
    gameplay: Res<GameplayConfig>,
) {
    let mut statuses = Vec::new();
    let mut occupied = HashSet::new();
    for (id, info) in players.iter_mut() {
        let Some(entity) = info.entity() else {
            continue;
        };
        let touching: Vec<_> = positions
            .get(entity)
            .map(|pos| {
                collision_world
                    .character_eraser_contacts(pos, pos, gameplay.player.physics(), None)
                    .collect()
            })
            .unwrap_or_default();
        let swept = contacts
            .swept
            .iter()
            .filter_map(|(player, field)| (player == id).then_some(*field));
        let mut touched = false;
        let mut entered = false;
        for field in touching.iter().copied().chain(swept) {
            touched = true;
            entered |= !contacts.occupied.contains(&(*id, field));
        }
        occupied.extend(touching.into_iter().map(|field| (*id, field)));
        if entered && info.connection.logged_in {
            let _ = info
                .connection
                .channel
                .send(ServerToClient::Send(ServerMessage::EraserEntered(SEraserEntered)));
        }
        if touched && info.erase_power_ups() {
            statuses.push(info.status(*id));
        }
    }
    contacts.swept.clear();
    contacts.occupied = occupied;
    for status in statuses {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PowerUpState;
    use crate::{config::ServerGameplayConfig, players::PlayerInfo};
    use common::protocol::{BarrierKindId, BarrierKindTable, CarrierId, Eraser, MapLayout, PowerUpKind};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn test_app() -> (App, Entity, UnboundedReceiver<ServerToClient>) {
        let mut app = App::new();
        let layout = MapLayout {
            erasers: [0.0, 4.0]
                .map(|z| Eraser {
                    x1: -2.0,
                    x2: 2.0,
                    z1: z,
                    z2: z,
                    y: 0.0,
                    height: 4.0,
                    width: 0.1,
                    level: 0,
                    carrier: CarrierId::WORLD,
                })
                .to_vec(),
            ..Default::default()
        };
        let entity = app
            .world_mut()
            .spawn((
                PlayerMarker,
                Position {
                    x: 0.0,
                    y: 0.0,
                    z: -3.0,
                },
            ))
            .id();
        let (tx, rx) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, tx);
        info.connection.logged_in = true;
        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), info);
        app.insert_resource(players)
            .insert_resource(
                ServerGameplayConfig::load_default()
                    .expect("gameplay config rejected")
                    .gameplay_config(),
            )
            .insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()))
            .init_resource::<EraserContacts>()
            .add_systems(Update, erase_power_ups_system);
        (app, entity, rx)
    }

    fn entry_cues(rx: &mut UnboundedReceiver<ServerToClient>) -> usize {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|message| matches!(message, ServerToClient::Send(ServerMessage::EraserEntered(_))))
            .count()
    }

    #[test]
    fn empty_inventory_plays_on_entry_and_reentry_but_not_while_standing() {
        let (mut app, entity, mut rx) = test_app();
        for (z, expected) in [(-3.0, 0), (0.0, 1), (0.0, 0), (4.0, 1), (4.0, 0), (8.0, 0), (4.0, 1)] {
            app.world_mut().get_mut::<Position>(entity).expect("position missing").z = z;
            app.update();
            assert_eq!(entry_cues(&mut rx), expected, "at z={z}");
        }
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&PlayerId(1))
            .expect("player missing")
            .life
            .missiles = 2;
        app.update();
        assert_eq!(entry_cues(&mut rx), 0);
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
    fn fast_pass_plays_even_when_both_tick_endpoints_are_outside() {
        let (mut app, entity, mut rx) = test_app();
        app.update();
        app.world_mut()
            .resource_mut::<EraserContacts>()
            .swept
            .insert((PlayerId(1), 0));
        app.world_mut().get_mut::<Position>(entity).expect("position missing").z = 2.0;
        app.update();
        assert_eq!(entry_cues(&mut rx), 1);
        app.update();
        assert_eq!(entry_cues(&mut rx), 0);
    }

    #[test]
    fn standing_in_a_field_erases_fresh_power_ups_but_keeps_keys_and_ammo() {
        let (mut app, entity, mut rx) = test_app();
        app.world_mut().get_mut::<Position>(entity).expect("position missing").z = 0.0;
        for iteration in 0..2 {
            {
                let mut players = app.world_mut().resource_mut::<PlayerMap>();
                let info = players.get_mut(&PlayerId(1)).expect("player missing");
                info.add_key(BarrierKindId(0));
                info.life.missiles = 2;
                info.life.power_ups.fill(PowerUpState::Permanent);
            }
            app.update();
            let players = app.world().resource::<PlayerMap>();
            let info = players.get(&PlayerId(1)).expect("player missing");
            assert!(PowerUpKind::ALL.into_iter().all(|kind| !info.has(kind)));
            assert_eq!(info.life.held_keys, [BarrierKindId(0)]);
            assert_eq!(info.life.missiles, 2);
            assert_eq!(entry_cues(&mut rx), usize::from(iteration == 0));
        }
    }
}
