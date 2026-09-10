use bevy::prelude::*;
use common::protocol::{CPortalRecovery, ClientMessage, PlayerId, SPortalCrossed, sequence_is_newer};

use super::super::{
    context::ServerMessageContext,
    players::{reset_local_comparisons, snap_player},
};
use crate::{
    constants::CAMERA_MAX_PITCH,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, PlayerInfo, PortalTransitBlend, eye_position},
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
        context.gameplay_config.player.eye_height(),
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
    eye_height: f32,
    to_server: &ClientToServerChannel,
) {
    if result.id != my_player_id {
        if result.accepted && !sequence_is_newer(player.last_movement_tick, result.tick) {
            player.last_movement_tick = result.tick;
            snap_player(commands, player, result.movement);
        }
        return;
    }
    if local.is_dead {
        return;
    }
    let Some(undo_view) = local.portal_crossings.resolve(result.seq, result.accepted) else {
        return;
    };
    if result.accepted {
        return;
    }
    warn!(
        "{}#{}, portal crossing {} rejected; snapping to server position",
        player.name, result.id.0, result.seq
    );
    snap_player(commands, player, result.movement);
    reset_local_comparisons(local);
    local.stored_yaw -= undo_view.x;
    local.stored_pitch = (local.stored_pitch - undo_view.y).clamp(-CAMERA_MAX_PITCH, CAMERA_MAX_PITCH);
    if let Some(camera) = camera {
        commands
            .entity(camera)
            .remove::<PortalTransitBlend>()
            .insert(Transform {
                translation: eye_position(result.movement.pos, eye_height),
                rotation: Quat::from_euler(EulerRot::YXZ, local.stored_yaw, local.stored_pitch, 0.0),
                ..default()
            });
    }
    let _ = to_server.send(ClientToServer::Send(ClientMessage::PortalRecovery(CPortalRecovery {
        seq: local.move_seq,
    })));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;
    use common::protocol::{Health, Player, PlayerMoveIntent, PlayerMovementState, Position};
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn acceptance_keeps_current_motion_and_rejection_undoes_all_pending_crossings() {
        for accepted in [true, false] {
            let mut world = World::new();
            let current = Position {
                x: 50.0,
                y: 2.0,
                z: 4.0,
            };
            let entity = world.spawn(current).id();
            let camera = world
                .spawn((
                    Transform::default(),
                    PortalTransitBlend {
                        delta: Quat::IDENTITY,
                        timer: Timer::from_seconds(1.0, TimerMode::Once),
                    },
                ))
                .id();
            let mut player = PlayerInfo::from_snapshot(
                entity,
                &Player::new("Player".into(), current, PlayerMoveIntent::Idle, 0.0, 0, Health(100.0)),
                0,
            );
            let movement = PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, -2.0, 0.0);
            let mut local = LocalPlayerInfo {
                move_seq: 12,
                stored_yaw: 1.6,
                stored_pitch: 0.3,
                ..default()
            };
            local.portal_crossings.record(10, movement, Vec2::new(1.0, 0.1));
            local.portal_crossings.record(12, movement, Vec2::new(0.4, 0.1));
            local.committed_positions.record(12, 20, current);
            let (sender, mut receiver) = unbounded_channel();
            let channel = ClientToServerChannel::new(sender);
            let result = SPortalCrossed {
                id: PlayerId(1),
                seq: 10,
                tick: 15,
                accepted,
                movement,
            };
            let mut state = SystemState::<Commands>::new(&mut world);
            apply_portal_crossing_result(
                result,
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                PlayerId(1),
                &mut player,
                &mut local,
                Some(camera),
                1.6,
                &channel,
            );
            state.apply(&mut world);
            if accepted {
                assert_eq!(*world.get::<Position>(entity).expect("position missing"), current);
                assert!(local.portal_crossings.is_pending());
                assert_eq!(local.stored_yaw, 1.6);
                assert!(receiver.try_recv().is_err());
                assert!(local.portal_crossings.resolve(12, true).is_some());
                assert!(!local.portal_crossings.is_pending());
            } else {
                assert_eq!(*world.get::<Position>(entity).expect("position missing"), movement.pos);
                assert!(!local.portal_crossings.is_pending());
                assert!(local.portal_crossings.entrance.is_none());
                assert!(local.committed_positions.get(12).is_none());
                assert_eq!(local.last_comparison_seq, Some(12));
                assert!((local.stored_yaw - 0.2).abs() < 1e-5);
                assert!((local.stored_pitch - 0.1).abs() < 1e-5);
                assert!(world.get::<PortalTransitBlend>(camera).is_none());
                assert!(matches!(
                    receiver.try_recv(),
                    Ok(ClientToServer::Send(ClientMessage::PortalRecovery(CPortalRecovery {
                        seq: 12
                    })))
                ));
            }
            apply_portal_crossing_result(
                result,
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                PlayerId(1),
                &mut player,
                &mut local,
                Some(camera),
                1.6,
                &channel,
            );
            state.apply(&mut world);
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn remote_crossings_cut_interpolation_but_cannot_rewind_a_newer_update() {
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
                1.6,
                &ClientToServerChannel::new(sender),
            );
            state.apply(&mut world);
            assert_eq!(
                *world.get::<Position>(entity).expect("position missing"),
                if latest_tick > 5 { current } else { movement.pos }
            );
            assert_eq!(player.last_movement_tick, latest_tick.max(5));
        }
    }
}
