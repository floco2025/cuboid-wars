use bevy::prelude::*;
use common::{
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{
        CMove, ClientMessage, FaceYaw, PlayerInput, PlayerMoveIntent, PlayerMovementState, Position, ServerTick,
    },
};

use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};

pub fn capture_player_input_system(
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    mut local: ResMut<LocalPlayerInfo>,
    query: Query<(&PlayerMoveIntent, &FaceYaw), With<LocalPlayerMarker>>,
) {
    local.pending_input = None;
    if local.is_dead {
        return;
    }
    let Ok((intent, yaw)) = query.single() else {
        return;
    };
    let hops = players.get(&my_player_id.0).map_or(0, |info| info.hops);
    local.pending_input = Some((
        PlayerInput {
            move_intent: *intent,
            face_yaw: yaw.0,
        },
        hops,
    ));
}

pub fn commit_player_input_system(
    to_server: Res<ClientToServerChannel>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    tick: Res<ServerTick>,
    mut local: ResMut<LocalPlayerInfo>,
    query: Query<
        (
            &Position,
            &PlayerMoveIntent,
            &FaceYaw,
            &CharacterVerticalVelocity,
            Option<&AirborneMomentum>,
            Option<&KnockbackVelocity>,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    let Some((input, hops)) = local.pending_input.take() else {
        return;
    };
    if local.is_dead {
        return;
    }
    let Ok((pos, intent, yaw, vertical, airborne, knockback)) = query.single() else {
        return;
    };
    let info = players.get(&my_player_id.0);
    let result_hops = info.map_or(0, |info| info.hops);
    local.move_seq = local.move_seq.wrapping_add(1);
    let seq = local.move_seq;
    let movement = PlayerMovementState::new(*pos, *intent, vertical.0, yaw.0).with_momentum(
        airborne.map_or(Vec3::ZERO, |m| m.0),
        knockback.map_or(Vec3::ZERO, |m| m.0),
    );
    local.committed_positions.record(seq, tick.0, *pos);
    let _ = to_server.send(ClientToServer::Send(ClientMessage::Move(CMove {
        seq,
        input,
        hops,
        movement,
        result_hops,
    })));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PlayerInfo;
    use common::protocol::{Health, Player, PlayerId};

    #[test]
    fn report_pairs_pre_step_input_with_post_step_position_and_portal_motion() {
        let mut app = App::new();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let id = PlayerId(1);
        let input_intent = PlayerMoveIntent::Running { direction: 3.0 };
        let entity = app
            .world_mut()
            .spawn((
                LocalPlayerMarker,
                Position::default(),
                input_intent,
                FaceYaw(3.0),
                CharacterVerticalVelocity(0.0),
                AirborneMomentum::default(),
                KnockbackVelocity::default(),
            ))
            .id();
        let mut players = PlayerMap::default();
        let player = Player::new(
            "Player".into(),
            Position::default(),
            input_intent,
            3.0,
            0,
            Health(100.0),
        );
        let mut info = PlayerInfo::from_snapshot(entity, &player, 9);
        info.hops = 2;
        players.insert(id, info);
        app.insert_resource(players)
            .insert_resource(MyPlayerId(id))
            .insert_resource(ServerTick(10))
            .init_resource::<LocalPlayerInfo>()
            .insert_resource(ClientToServerChannel::new(sender));
        let capture = app.world_mut().register_system(capture_player_input_system);
        let commit = app.world_mut().register_system(commit_player_input_system);
        app.world_mut().run_system(capture).expect("input capture failed");
        let result = Position {
            x: 10.0,
            y: 3.0,
            z: 1.0,
        };
        let output_intent = PlayerMoveIntent::Running { direction: 0.0 };
        app.world_mut().entity_mut(entity).insert((
            result,
            output_intent,
            FaceYaw(0.0),
            CharacterVerticalVelocity(7.0),
            AirborneMomentum(Vec3::X * 4.0),
            KnockbackVelocity(Vec3::Z * 2.0),
        ));
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&id)
            .expect("player missing")
            .hops = 3;
        app.world_mut().run_system(commit).expect("movement commit failed");
        let ClientToServer::Send(ClientMessage::Move(report)) = receiver.try_recv().expect("report missing") else {
            panic!("unexpected message");
        };
        assert_eq!(report.input.move_intent, input_intent);
        assert_eq!(report.input.face_yaw, 3.0);
        assert_eq!(report.hops, 2);
        assert_eq!(report.result_hops, 3);
        assert_eq!(report.movement.pos, result);
        assert_eq!(report.movement.move_intent, output_intent);
        assert_eq!(report.movement.vertical_velocity, 7.0);
        assert_eq!(report.movement.airborne_momentum, [4.0, 0.0, 0.0]);
        assert_eq!(report.movement.knockback, [0.0, 0.0, 2.0]);
        let local = app.world().resource::<LocalPlayerInfo>();
        let record = local
            .committed_positions
            .get(report.seq)
            .expect("reported position missing");
        assert_eq!(record.pos, result);
        assert_eq!(record.tick, 10);
        app.world_mut().run_system(commit).expect("empty commit failed");
        assert!(receiver.try_recv().is_err());
    }
}
