use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_DEATH_Y,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{CPlayerMovementEvent, ClientMessage, PlayerMovementEvent, Position},
};

use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

#[derive(Component)]
pub(crate) struct LocalMovementStep {
    pub start: Position,
    pub crushed: bool,
    pub impact_speed: f32,
}

pub(crate) fn report_player_movement_events_system(
    to_server: Res<ClientToServerChannel>,
    mut local: ResMut<LocalPlayerInfo>,
    collision: Res<CollisionWorld>,
    carriers: Res<Carriers>,
    gameplay: Res<GameplayConfig>,
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
        .character_eraser_contacts(pos, pos, physics, None)
        .next()
        .is_some();
    let swept = collision
        .character_eraser_contacts(&step.start, &sweep_end, physics, Some(&carriers))
        .next()
        .is_some();
    let send = |outcome| {
        to_server.send(ClientToServer::Send(ClientMessage::PlayerMovementEvent(
            CPlayerMovementEvent {
                generation,
                event: outcome,
            },
        )))
    };
    // A pickup update can arrive after contact, so the client inventory cannot gate erasure.
    if touching || swept {
        send(PlayerMovementEvent::EraseEquipment);
    }
    if crossing.is_none() && step.impact_speed > 0.0 {
        send(PlayerMovementEvent::Landed {
            pos: *pos,
            impact_speed: step.impact_speed,
        });
    }
    if crossing.is_none() && step.crushed {
        send(PlayerMovementEvent::Crushed { pos: *pos });
    }
    if pos.y < CHARACTER_FALL_DEATH_Y && !reports.void_reported {
        reports.void_reported = true;
        send(PlayerMovementEvent::FellOutOfWorld);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures;
    use common::protocol::{BarrierKindTable, CarrierId, Eraser, Lane, MapLayout, PlayerGeneration};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn app(layout: MapLayout) -> (App, Entity, UnboundedReceiver<ClientToServer>) {
        let mut app = App::new();
        let (tx, rx) = unbounded_channel();
        app.insert_resource(test_fixtures::gameplay_config())
            .insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()))
            .insert_resource(Carriers::from_layout(&layout))
            .init_resource::<LocalPlayerInfo>()
            .insert_resource(ClientToServerChannel::new(tx))
            .add_systems(Update, report_player_movement_events_system);
        let entity = app
            .world_mut()
            .spawn((
                LocalPlayerMarker,
                Position::default(),
                LocalMovementStep {
                    start: Position::default(),
                    crushed: false,
                    impact_speed: 0.0,
                },
            ))
            .id();
        (app, entity, rx)
    }

    fn messages(rx: &mut UnboundedReceiver<ClientToServer>) -> Vec<PlayerMovementEvent> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .map(|command| {
                let ClientToServer::Send(message @ ClientMessage::PlayerMovementEvent(_)) = command else {
                    panic!("unexpected command")
                };
                assert_eq!(message.lane(), Lane::Reliable);
                let ClientMessage::PlayerMovementEvent(event) = message else {
                    unreachable!()
                };
                event.event
            })
            .collect()
    }

    #[test]
    fn landing_reports_the_motor_impact_speed_and_portal_crossings_suppress_impacts() {
        for crossing in [false, true] {
            let (mut app, entity, mut rx) = app(MapLayout::default());
            app.world_mut()
                .get_mut::<LocalMovementStep>(entity)
                .expect("step missing")
                .impact_speed = 20.0;
            if crossing {
                app.world_mut()
                    .resource_mut::<LocalPlayerInfo>()
                    .reports
                    .begin_crossing(Position::default());
            }
            app.update();
            let events = messages(&mut rx);
            if crossing {
                assert!(events.is_empty());
            } else {
                assert!(matches!(
                    events.as_slice(),
                    [PlayerMovementEvent::Landed { impact_speed: 20.0, .. }]
                ));
            }
            app.world_mut()
                .get_mut::<LocalMovementStep>(entity)
                .expect("step missing")
                .impact_speed = 0.0;
            app.update();
            assert!(messages(&mut rx).is_empty());
        }
    }

    #[test]
    fn eraser_sweeps_cover_travel_and_both_portal_ends_without_sweeping_the_gap() {
        for (eraser_x, crossing, expected) in [
            (1.0, false, true),
            (1.0, true, true),
            (5.0, true, false),
            (100.0, true, true),
        ] {
            let layout = MapLayout {
                erasers: vec![Eraser {
                    x1: eraser_x,
                    x2: eraser_x,
                    z1: -5.0,
                    z2: 5.0,
                    y: -1.0,
                    height: 5.0,
                    width: 0.1,
                    level: 0,
                    carrier: CarrierId::WORLD,
                }],
                ..default()
            };
            let (mut app, entity, mut rx) = app(layout);
            if crossing {
                app.world_mut()
                    .resource_mut::<LocalPlayerInfo>()
                    .reports
                    .begin_crossing(Position { x: 2.0, ..default() });
            }
            app.world_mut().get_mut::<Position>(entity).expect("position missing").x =
                if crossing { 100.0 } else { 2.0 };
            app.update();
            assert_eq!(
                messages(&mut rx)
                    .iter()
                    .any(|event| matches!(event, PlayerMovementEvent::EraseEquipment)),
                expected
            );
        }
    }

    #[test]
    fn crushing_reports_while_contact_lasts_and_void_reports_once_per_body() {
        let (mut app, entity, mut rx) = app(MapLayout::default());
        app.world_mut()
            .get_mut::<LocalMovementStep>(entity)
            .expect("step missing")
            .crushed = true;
        app.update();
        assert!(matches!(
            messages(&mut rx).as_slice(),
            [PlayerMovementEvent::Crushed { .. }]
        ));
        app.update();
        assert!(matches!(
            messages(&mut rx).as_slice(),
            [PlayerMovementEvent::Crushed { .. }]
        ));
        app.world_mut()
            .get_mut::<LocalMovementStep>(entity)
            .expect("step missing")
            .crushed = false;
        app.world_mut().get_mut::<Position>(entity).expect("position missing").y = CHARACTER_FALL_DEATH_Y - 1.0;
        app.update();
        assert!(matches!(
            messages(&mut rx).as_slice(),
            [PlayerMovementEvent::FellOutOfWorld]
        ));
        app.update();
        assert!(messages(&mut rx).is_empty());
        app.world_mut()
            .resource_mut::<LocalPlayerInfo>()
            .reports
            .begin_body(PlayerGeneration(1));
        app.update();
        assert!(matches!(
            messages(&mut rx).as_slice(),
            [PlayerMovementEvent::FellOutOfWorld]
        ));
    }
}
