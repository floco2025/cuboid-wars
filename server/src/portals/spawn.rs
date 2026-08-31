use bevy::prelude::*;

use super::PortalMap;
use crate::{
    network::broadcast_to_all,
    players::{PlayerMap, PlayerStateQuery},
};
use common::{
    config::GameplayConfig,
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, PortalPlacement, PortalSet, compute_portal_placement},
    protocol::*,
};

pub fn handle_portal_shot_message(
    entity: Entity,
    id: PlayerId,
    msg: &CPortalShot,
    players: &PlayerMap,
    player_data: &PlayerStateQuery,
    collision_world: &CollisionWorld,
    map_layout: &MapLayout,
    gameplay_config: &GameplayConfig,
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
    // No valid aperture (miss, doesn't fit, covers a fixture): silent fizzle
    // — the client ran the same shared check and already dry-fired.
    let Some(placement) = compute_portal_placement(
        origin,
        direction,
        msg.face_yaw,
        gameplay_config.portals.range,
        collision_world,
        map_layout,
    ) else {
        return;
    };
    let portal = portal_from_placement(&placement, id, msg.end, msg.face_yaw);
    portals.set(portal);
    *portal_set = portals.rebuild_set(collision_world);
    broadcast_to_all(players, ServerMessage::PortalOpened(SPortalOpened { portal }));
}

// Pure geometry: the aperture sits at the hit point with the surface's
// outward normal; the shooter's yaw rides along to orient the frame where
// world-up degenerates (near-vertical normals).
fn portal_from_placement(placement: &PortalPlacement, owner: PlayerId, end: PortalEnd, face_yaw: f32) -> Portal {
    Portal {
        owner,
        end,
        pos: placement.pos.into(),
        nx: placement.normal.x,
        ny: placement.normal.y,
        nz: placement.normal.z,
        yaw: face_yaw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_carries_the_placement_geometry_and_shooter_yaw() {
        let placement = PortalPlacement {
            pos: Vec3::new(1.0, 2.0, 3.0),
            normal: Vec3::NEG_X,
        };
        let portal = portal_from_placement(&placement, PlayerId(7), PortalEnd::B, 1.25);
        assert_eq!(Vec3::from(portal.pos), placement.pos);
        assert_eq!(Vec3::new(portal.nx, portal.ny, portal.nz), placement.normal);
        assert_eq!(portal.owner, PlayerId(7));
        assert_eq!(portal.end, PortalEnd::B);
        assert_eq!(portal.yaw, 1.25);
    }
}
