use bevy::prelude::*;
use common::protocol::{CPortalRecovery, ClientMessage, PlayerId, SPortalCrossed, sequence_is_newer};

use crate::{
    network::{ClientToServer, ClientToServerChannel, context::ServerMessageContext, players::snap_player},
    players::{CrossingResolution, LocalPlayerInfo, PlayerInfo},
    portals::undo_portal_view,
};

pub(in crate::network) fn handle_portal_crossed_message(
    result: SPortalCrossed,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let Some(player) = context.players.get_mut(&result.id) else {
        return;
    };
    apply_portal_crossing_result(
        result,
        commands,
        context.my_player_id.0,
        player,
        &mut context.local_player_info,
        context.cameras.single().ok(),
        &context.to_server,
    );
}

fn apply_portal_crossing_result(
    result: SPortalCrossed,
    commands: &mut Commands,
    my_player_id: PlayerId,
    player: &mut PlayerInfo,
    local: &mut LocalPlayerInfo,
    camera: Option<Entity>,
    to_server: &ClientToServerChannel,
) {
    if result.id != my_player_id {
        // Placed whenever it arrives: the body kept solid portal backing, so
        // smoothing from a newer update cannot take it through the wall, and
        // the next update corrects a tick-old exit forward.
        if result.accepted {
            if sequence_is_newer(result.tick, player.last_movement_tick) {
                player.last_movement_tick = result.tick;
            }
            snap_player(commands, player, &result.movement);
        }
        return;
    }
    if result.accepted {
        local.reports.resolve_crossing(result.seq, true);
        return;
    }
    // Answered first, whatever state the crossing is in: the server holds
    // every report until the recovery arrives, and only a death would
    // otherwise release it.
    to_server.send(ClientToServer::Send(ClientMessage::PortalRecovery(CPortalRecovery {
        seq: local.reports.seq(),
    })));
    let CrossingResolution::Rejected { undo_view } = local.reports.resolve_crossing(result.seq, false) else {
        return;
    };
    if local.is_dead {
        return;
    }
    warn!(
        "{}#{}, portal crossing {} rejected; snapping to server position",
        player.name, result.id.0, result.seq
    );
    snap_player(commands, player, &result.movement);
    local.reports.invalidate();
    undo_portal_view(commands, camera, local, undo_view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PortalTransitBlend;
    use bevy::ecs::system::SystemState;
    use common::protocol::{Health, Player, PlayerMoveIntent, PlayerMovementState, Position};
    use tokio::sync::mpsc::unbounded_channel;

    struct Fixture {
        world: World,
        entity: Entity,
        camera: Entity,
        player: PlayerInfo,
        local: LocalPlayerInfo,
    }

    const CURRENT: Position = Position {
        x: 50.0,
        y: 2.0,
        z: 4.0,
    };

    fn fixture() -> Fixture {
        let mut world = World::new();
        let entity = world.spawn(CURRENT).id();
        let camera = world
            .spawn((
                Transform::default(),
                PortalTransitBlend {
                    delta: Quat::IDENTITY,
                    timer: Timer::from_seconds(1.0, TimerMode::Once),
                },
            ))
            .id();
        let player = PlayerInfo::from_snapshot(
            entity,
            &Player::new("Player".into(), CURRENT, PlayerMoveIntent::Idle, 0.0, 0, Health(100.0)),
            0,
        );
        let mut local = LocalPlayerInfo {
            stored_yaw: 1.6,
            stored_pitch: 0.3,
            ..default()
        };
        let movement = PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, -2.0, 0.0);
        local.reports.begin_crossing(movement, Vec2::new(1.0, 0.1));
        local.reports.set_seq(12);
        local.reports.record(12, 18, CURRENT);
        Fixture {
            world,
            entity,
            camera,
            player,
            local,
        }
    }

    fn crossed(seq: u32, accepted: bool) -> SPortalCrossed {
        SPortalCrossed {
            id: PlayerId(1),
            seq,
            tick: 15,
            accepted,
            movement: PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, -2.0, 0.0),
        }
    }

    // The report system's part, by hand: the crossing goes out under `seq`.
    fn send_crossing(local: &mut LocalPlayerInfo, seq: u32) {
        assert!(local.reports.take_crossing(seq).is_some());
    }

    fn apply(fixture: &mut Fixture, result: SPortalCrossed) -> Option<ClientToServer> {
        let (sender, mut receiver) = unbounded_channel();
        let channel = ClientToServerChannel::new(sender);
        let mut state = SystemState::<Commands>::new(&mut fixture.world);
        apply_portal_crossing_result(
            result,
            &mut state.get_mut(&mut fixture.world).expect("commands unavailable"),
            PlayerId(1),
            &mut fixture.player,
            &mut fixture.local,
            Some(fixture.camera),
            &channel,
        );
        state.apply(&mut fixture.world);
        receiver.try_recv().ok()
    }

    #[test]
    fn acceptance_keeps_the_local_prediction_and_sends_nothing() {
        let mut fixture = fixture();
        send_crossing(&mut fixture.local, 10);
        fixture
            .local
            .reports
            .begin_crossing(crossed(12, true).movement, Vec2::new(0.4, 0.1));
        send_crossing(&mut fixture.local, 12);
        assert!(apply(&mut fixture, crossed(10, true)).is_none());
        assert_eq!(
            *fixture.world.get::<Position>(fixture.entity).expect("position missing"),
            CURRENT
        );
        assert!(fixture.local.reports.crossing_pending());
        assert_eq!(fixture.local.stored_yaw, 1.6);
        assert!(apply(&mut fixture, crossed(12, true)).is_none());
        assert!(!fixture.local.reports.crossing_pending());
    }

    #[test]
    fn rejection_snaps_undoes_every_pending_crossing_and_answers_with_a_recovery() {
        let mut fixture = fixture();
        send_crossing(&mut fixture.local, 10);
        fixture
            .local
            .reports
            .begin_crossing(crossed(12, true).movement, Vec2::new(0.4, 0.1));
        send_crossing(&mut fixture.local, 12);
        let result = crossed(10, false);
        assert!(matches!(
            apply(&mut fixture, result),
            Some(ClientToServer::Send(ClientMessage::PortalRecovery(CPortalRecovery {
                seq: 12
            })))
        ));
        assert_eq!(
            *fixture.world.get::<Position>(fixture.entity).expect("position missing"),
            result.movement.pos
        );
        assert!(!fixture.local.reports.crossing_pending());
        assert!(fixture.local.reports.echo_tick(12).is_none());
        assert!((fixture.local.stored_yaw - 0.2).abs() < 1e-5);
        assert!((fixture.local.stored_pitch - 0.1).abs() < 1e-5);
        assert!(fixture.world.get::<PortalTransitBlend>(fixture.camera).is_none());
    }

    #[test]
    fn a_rejection_the_client_cannot_match_or_apply_is_still_answered_and_moves_nothing() {
        for dead in [false, true] {
            let mut fixture = fixture();
            send_crossing(&mut fixture.local, 10);
            fixture.local.is_dead = dead;
            let seq = if dead { 10 } else { 7 };
            assert!(matches!(
                apply(&mut fixture, crossed(seq, false)),
                Some(ClientToServer::Send(ClientMessage::PortalRecovery(CPortalRecovery {
                    seq: 12
                })))
            ));
            assert_eq!(
                *fixture.world.get::<Position>(fixture.entity).expect("position missing"),
                CURRENT
            );
            assert_eq!(fixture.local.stored_yaw, 1.6);
            assert!(fixture.world.get::<PortalTransitBlend>(fixture.camera).is_some());
        }
    }

    #[test]
    fn remote_crossings_always_place_the_body_and_never_rewind_the_tick() {
        for latest_tick in [4, 5, 6] {
            let mut world = World::new();
            let current = Position {
                x: 20.0,
                y: 0.0,
                z: 0.0,
            };
            let entity = world.spawn(current).id();
            let mut player = PlayerInfo::from_snapshot(
                entity,
                &Player::new("Player".into(), current, PlayerMoveIntent::Idle, 0.0, 0, Health(100.0)),
                latest_tick,
            );
            let movement =
                PlayerMovementState::new(Position { x: 10.0, ..default() }, PlayerMoveIntent::Idle, 3.0, 1.0);
            let result = SPortalCrossed {
                id: PlayerId(2),
                seq: 10,
                tick: 5,
                accepted: true,
                movement,
            };
            let (sender, _) = unbounded_channel();
            let mut state = SystemState::<Commands>::new(&mut world);
            apply_portal_crossing_result(
                result,
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                PlayerId(1),
                &mut player,
                &mut LocalPlayerInfo::default(),
                None,
                &ClientToServerChannel::new(sender),
            );
            state.apply(&mut world);
            assert_eq!(*world.get::<Position>(entity).expect("position missing"), movement.pos);
            assert_eq!(player.last_movement_tick, latest_tick.max(5));
        }
    }
}
