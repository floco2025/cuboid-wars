use bevy::prelude::*;

use super::PortalMap;
use crate::{
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    players::{PlayerMap, PlayerStateQuery},
};
use common::{
    config::GameplayConfig,
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, PortalSet, WorldSurfaceHit},
    protocol::*,
};

pub fn handle_portal_shot_message(
    entity: Entity,
    id: PlayerId,
    msg: &CPortalShot,
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    portals: &mut PortalMap,
    portal_set: &mut PortalSet,
) {
    // Reject non-finite aim before it reaches the surface ray.
    if !(msg.face_yaw.is_finite() && msg.face_pitch.is_finite()) {
        return;
    }
    let Ok((pos, _, _, _)) = player_data.get(entity) else {
        return;
    };
    let origin = Vec3::new(pos.x, pos.y + gameplay_config.player.eye_height(), pos.z);
    let direction = direction_from_yaw_pitch(msg.face_yaw, msg.face_pitch);
    // No surface within range: the shot silently fizzles.
    let Some(hit) = collision_world.world_surface_along_ray(origin, direction, server_gameplay_config.portals.range)
    else {
        return;
    };
    let portal = portal_from_hit(&hit, id, msg.end, msg.face_yaw);
    portals.set(portal);
    *portal_set = portals.rebuild_set();
    broadcast_to_all(players, ServerMessage::PortalOpened(SPortalOpened { portal }));
}

// Pure geometry: the aperture sits at the hit point with the surface's
// outward normal; the shooter's yaw rides along to orient the frame where
// world-up degenerates (near-vertical normals).
fn portal_from_hit(hit: &WorldSurfaceHit, owner: PlayerId, end: PortalEnd, face_yaw: f32) -> Portal {
    Portal {
        owner,
        end,
        pos: hit.point.into(),
        nx: hit.normal.x,
        ny: hit.normal.y,
        nz: hit.normal.z,
        yaw: face_yaw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_carries_the_hit_geometry_and_shooter_yaw() {
        let hit = WorldSurfaceHit {
            point: Vec3::new(1.0, 2.0, 3.0),
            normal: Vec3::NEG_X,
        };
        let portal = portal_from_hit(&hit, PlayerId(7), PortalEnd::B, 1.25);
        assert_eq!(Vec3::from(portal.pos), hit.point);
        assert_eq!(Vec3::new(portal.nx, portal.ny, portal.nz), hit.normal);
        assert_eq!(portal.owner, PlayerId(7));
        assert_eq!(portal.end, PortalEnd::B);
        assert_eq!(portal.yaw, 1.25);
    }
}
