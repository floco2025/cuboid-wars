use bevy::prelude::*;

use super::{PortalAssignments, PortalMap};
use crate::{
    network::broadcast_to_all,
    players::{PlayerMap, PlayerStateQuery},
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, PortalPlacementFailure, PortalSet, compute_portal_placement, portal_placement_overlaps},
    protocol::*,
};

pub fn handle_portal_shot_message(
    entity: Entity,
    id: PlayerId,
    msg: &CPortalShot,
    players: &mut PlayerMap,
    time: &Time,
    player_data: &PlayerStateQuery,
    collision_world: &CollisionWorld,
    carriers: &Carriers,
    map_layout: &MapLayout,
    map_settings: &MapSettings,
    plates: &PlateState,
    gameplay_config: &GameplayConfig,
    portal_assignments: &PortalAssignments,
    portals: &mut PortalMap,
    portal_set: &mut PortalSet,
) {
    let access = portal_assignments.get(&id);
    if !access.allows(msg.end) {
        return;
    }
    let Some(pair) = access.pair() else {
        return;
    };
    // Reject non-finite aim before it reaches the surface ray.
    if !(msg.face_yaw.is_finite() && msg.face_pitch.is_finite()) {
        return;
    }
    if !players
        .get_mut(&id)
        .is_some_and(|info| info.try_start_portal_shot(time.elapsed_secs(), gameplay_config.projectiles.cooldown_secs))
    {
        return;
    }
    let Ok((pos, _, _, _)) = player_data.get(entity) else {
        return;
    };
    let origin = Vec3::new(pos.x, pos.y + gameplay_config.player.eye_height(), pos.z);
    let direction = direction_from_yaw_pitch(msg.face_yaw, msg.face_pitch);
    let placement = match compute_portal_placement(
        origin,
        direction,
        msg.face_yaw,
        gameplay_config.portals.range,
        collision_world,
        map_layout,
        carriers,
        &plates.open_barrier_kinds,
        &map_settings.textures,
    ) {
        Ok(placement) => placement,
        Err(PortalPlacementFailure::IncompatibleMaterial(impact)) => {
            broadcast_to_all(
                players,
                ServerMessage::PortalFizzled(SPortalFizzled {
                    shooter: id,
                    impact: impact.portal(pair, msg.end, carriers),
                }),
            );
            return;
        }
        Err(PortalPlacementFailure::InvalidPlacement) => return,
    };
    if portal_placement_overlaps(&placement, pair, msg.end, &portals.snapshot_portals(), carriers) {
        return;
    }
    let portal = placement.portal(pair, msg.end, carriers);
    if !portals.set(portal) {
        return;
    }
    *portal_set = portals.rebuild_set(collision_world, carriers);
    broadcast_to_all(
        players,
        ServerMessage::PortalOpened(SPortalOpened { shooter: id, portal }),
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::ecs::system::SystemState;
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        config::ServerGameplayConfig,
        network::ServerToClient,
        players::{PlayerInfo, PowerUpState},
    };

    #[test]
    fn incompatible_shot_broadcasts_one_fizzle_without_replacing_a_portal() {
        let config = ServerGameplayConfig::load_default().expect("gameplay config is invalid");
        let gameplay = config.gameplay_config();
        let mut settings = config.maps["hotel"].settings.clone();
        settings.textures = [("blocked".to_owned(), TextureSettings { portalable: false })].into();
        let layout = MapLayout {
            walls: vec![Wall {
                x1: -5.0,
                x2: 5.0,
                z1: 0.0,
                z2: 0.0,
                y: 0.0,
                height: 5.0,
                width: 0.3,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            wall_materials: vec![FaceMaterials::uniform("blocked")],
            ..default()
        };
        let collision = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        let carriers = Carriers::default();
        let mut world = World::new();
        let entity = world
            .spawn((
                Position { x: 0.0, y: 0.0, z: 3.0 },
                PlayerMoveIntent::default(),
                FaceYaw(0.0),
                Health(100.0),
                PlayerMarker,
            ))
            .id();
        let mut queries = SystemState::<PlayerStateQuery>::new(&mut world);
        let query = queries.get(&world).expect("player query parameters are invalid");
        let id = PlayerId(1);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut info = PlayerInfo::new(entity, tx);
        info.connection.logged_in = true;
        info.life.power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Permanent;
        let mut players = PlayerMap::default();
        players.insert(id, info);
        let (tx, mut observer) = mpsc::unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.connection.logged_in = true;
        players.insert(PlayerId(2), info);
        let mut assignments = PortalAssignments::new(PortalMode::Both);
        let pair = assignments.assign(id).pair().expect("portal assignment has no pair");
        let existing = Portal {
            pair,
            end: PortalEnd::A,
            pos: Vec3::new(2.0, 1.5, 0.15).into(),
            nx: 0.0,
            ny: 0.0,
            nz: 1.0,
            yaw: 0.0,
            carrier: CarrierId::WORLD,
        };
        let mut portals = PortalMap::default();
        portals.set(existing);
        let mut set = PortalSet::default();
        let mut time = Time::default();
        time.advance_by(Duration::from_secs(1));
        let shot = CPortalShot {
            end: PortalEnd::A,
            face_yaw: std::f32::consts::PI,
            face_pitch: 0.0,
        };
        for _ in 0..2 {
            handle_portal_shot_message(
                entity,
                id,
                &shot,
                &mut players,
                &time,
                &query,
                &collision,
                &carriers,
                &layout,
                &settings,
                &PlateState::default(),
                &gameplay,
                &assignments,
                &mut portals,
                &mut set,
            );
        }
        for receiver in [&mut rx, &mut observer] {
            let ServerToClient::Send(ServerMessage::PortalFizzled(message)) =
                receiver.try_recv().expect("fizzle missing")
            else {
                panic!("incompatible shot sent a different message");
            };
            assert_eq!(message.shooter, id);
            assert_eq!(message.impact.end, PortalEnd::A);
            assert!((message.impact.pos.z - 0.15).abs() < 1e-4);
            assert!(receiver.try_recv().is_err(), "cooldown allowed a second cue");
        }
        assert_eq!(portals.snapshot_portals(), vec![existing]);
        assert!(set.is_empty());
    }
}
