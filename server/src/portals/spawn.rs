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
    physics::{CollisionWorld, PortalSet, compute_portal_placement, portal_placement_overlaps},
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
    // No valid aperture (miss, doesn't fit, covers a fixture): silent fizzle
    // — the client ran the same shared check and already dry-fired.
    let Some(placement) = compute_portal_placement(
        origin,
        direction,
        msg.face_yaw,
        gameplay_config.portals.range,
        collision_world,
        map_layout,
        carriers,
    ) else {
        return;
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
