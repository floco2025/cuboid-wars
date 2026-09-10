use bevy::prelude::*;
use common::{
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{
        CMove, CPortalCross, ClientMessage, FaceYaw, PlayerMoveIntent, PlayerMovementState, Position, ServerTick,
    },
};

use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

pub fn commit_player_input_system(
    to_server: Res<ClientToServerChannel>,
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
    if local.is_dead {
        return;
    }
    let Ok((pos, intent, yaw, vertical, airborne, knockback)) = query.single() else {
        return;
    };
    local.move_seq = local.move_seq.wrapping_add(1);
    let seq = local.move_seq;
    let movement = PlayerMovementState::new(*pos, *intent, vertical.0, yaw.0).with_momentum(
        airborne.map_or(Vec3::ZERO, |m| m.0),
        knockback.map_or(Vec3::ZERO, |m| m.0),
    );
    local.committed_positions.record(seq, tick.0, *pos);
    let command = if let Some(entrance) = local.portal_crossings.entrance.take() {
        ClientToServer::Send(ClientMessage::PortalCross(CPortalCross {
            seq,
            entrance,
            movement,
        }))
    } else {
        let message = ClientMessage::Move(CMove { seq, movement });
        if local.portal_crossings.is_pending() {
            ClientToServer::SendReliable(message)
        } else {
            ClientToServer::Send(message)
        }
    };
    let _ = to_server.send(command);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{characters::PreviousTickPosition, players::PlayerMap, portals::portal_transit_system, test_fixtures};
    use common::{
        map::Carriers,
        physics::{CollisionWorld, PortalSet},
        protocol::{BarrierKindTable, CarrierId, MapLayout, PlayerId, PlayerMarker, Portal, PortalEnd, PortalPairId},
    };
    use std::f32::consts::PI;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn only_local_players_cross_and_followup_moves_stay_reliable_until_confirmation() {
        let mut app = App::new();
        let (sender, mut receiver) = unbounded_channel();
        let collision = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let portals: Vec<_> = [PortalEnd::A, PortalEnd::B]
            .into_iter()
            .enumerate()
            .map(|(i, end)| Portal {
                pair: PortalPairId(1),
                end,
                pos: Position {
                    x: i as f32 * 10.0,
                    y: 1.6,
                    z: 0.0,
                },
                nx: 0.0,
                ny: 0.0,
                nz: 1.0,
                yaw: 0.0,
                carrier: CarrierId::WORLD,
            })
            .collect();
        let portal_set = PortalSet::rebuild(&portals, &collision, &Carriers::default());
        app.insert_resource(portal_set)
            .insert_resource(test_fixtures::gameplay_config())
            .insert_resource(test_fixtures::map_settings())
            .init_resource::<LocalPlayerInfo>()
            .init_resource::<PlayerMap>()
            .insert_resource(ServerTick(10))
            .insert_resource(ClientToServerChannel::new(sender))
            .add_systems(Update, (portal_transit_system, commit_player_input_system).chain());
        let entrance = Position {
            x: 0.0,
            y: 0.7,
            z: -0.05,
        };
        let spawn = |world: &mut World, id| {
            world
                .spawn((
                    PlayerId(id),
                    PlayerMarker,
                    entrance,
                    PreviousTickPosition(Position { z: 0.15, ..entrance }),
                    PlayerMoveIntent::Running { direction: PI },
                    FaceYaw(PI),
                    CharacterVerticalVelocity(-2.0),
                    AirborneMomentum(Vec3::X * 4.0),
                    KnockbackVelocity(Vec3::Z * 2.0),
                ))
                .id()
        };
        let local = spawn(app.world_mut(), 1);
        app.world_mut().entity_mut(local).insert(LocalPlayerMarker);
        let remote = spawn(app.world_mut(), 2);
        app.update();
        let ClientToServer::Send(ClientMessage::PortalCross(crossing)) = receiver.try_recv().expect("crossing missing")
        else {
            panic!("expected crossing event")
        };
        assert_eq!(crossing.seq, 1);
        assert_eq!(crossing.entrance.pos, entrance);
        assert_eq!(
            crossing.entrance.move_intent,
            PlayerMoveIntent::Running { direction: PI }
        );
        assert!((crossing.movement.pos.x - 10.0).abs() < 1e-5);
        assert!((crossing.movement.airborne_momentum[0] + 4.0).abs() < 1e-4);
        assert!((crossing.movement.knockback[2] + 2.0).abs() < 1e-4);
        assert_eq!(
            *app.world().get::<Position>(remote).expect("remote position missing"),
            entrance
        );
        assert!(receiver.try_recv().is_err());
        app.world_mut()
            .get_mut::<Position>(local)
            .expect("local position missing")
            .z += 1.0;
        app.update();
        assert!(matches!(
            receiver.try_recv(),
            Ok(ClientToServer::SendReliable(ClientMessage::Move(CMove { seq: 2, .. })))
        ));
        app.world_mut()
            .resource_mut::<LocalPlayerInfo>()
            .portal_crossings
            .resolve(1, true);
        app.update();
        assert!(matches!(
            receiver.try_recv(),
            Ok(ClientToServer::Send(ClientMessage::Move(CMove { seq: 3, .. })))
        ));
        let info = app.world().resource::<LocalPlayerInfo>();
        assert_eq!(
            info.committed_positions.get(3).expect("record missing").pos,
            *app.world().get::<Position>(local).expect("position missing")
        );
    }
}
