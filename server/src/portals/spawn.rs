use bevy::prelude::*;

use super::{PortalAssignments, PortalMap};
use crate::{
    network::{SharedWorld, broadcast_to_all},
    players::PlayerMap,
};
use common::{
    physics::{PortalSet, portal_placement_overlaps},
    protocol::*,
};

pub(crate) fn handle_portal_shot_message(
    id: PlayerId,
    msg: &CPortalShot,
    players: &mut PlayerMap,
    time: &Time,
    world: &SharedWorld,
    portal_assignments: &PortalAssignments,
    portals: &mut PortalMap,
    portal_set: &mut PortalSet,
) {
    let portal = msg.result.portal();
    let access = portal_assignments.get(&id);
    if access.pair() != Some(portal.pair) || !access.allows(portal.end) {
        return;
    }
    let normal = Vec3::new(portal.nx, portal.ny, portal.nz);
    if !Vec3::from(portal.pos).is_finite()
        || !portal.yaw.is_finite()
        || normal.try_normalize().is_none()
        || portal
            .carrier
            .carried_index()
            .is_some_and(|index| index >= world.carriers.carried_count())
    {
        return;
    }
    let Some(player) = players.get_mut(&id) else {
        return;
    };
    if player.is_dead()
        || player.session.generation != msg.generation
        || !player.try_start_portal_shot(time.elapsed_secs(), world.gameplay_config.projectiles.cooldown_secs)
    {
        return;
    }
    match msg.result {
        PortalShotResult::Fizzled(impact) => {
            broadcast_to_all(
                players,
                ServerMessage::PortalFizzled(SPortalFizzled { shooter: id, impact }),
            );
        }
        PortalShotResult::Placed(portal) => {
            if portal_placement_overlaps(&portal, &portals.snapshot_portals(), &world.carriers) || !portals.set(portal)
            {
                return;
            }
            *portal_set = portals.rebuild_set(&world.collision_world, &world.carriers);
            broadcast_to_all(
                players,
                ServerMessage::PortalOpened(SPortalOpened { shooter: id, portal }),
            );
        }
    }
}
