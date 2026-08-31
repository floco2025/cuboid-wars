use bevy::prelude::*;
use std::collections::HashSet;

use super::super::context::ServerMessageContext;
use crate::portals::{PortalInfo, spawn_portal};
use common::{physics::PortalSet, protocol::*};

// Snapshot diff keyed by (owner, end): spawn missing, replace moved (a
// re-shot end), despawn absent (owner disconnected). Any change rebuilds the
// shared `PortalSet` the cosmetic projectile sim flies through.
pub(in crate::network) fn sync_portals(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    server_portals: &[Portal],
) {
    let server_keys: HashSet<(PlayerId, PortalEnd)> =
        server_portals.iter().map(|portal| (portal.owner, portal.end)).collect();

    let mut changed = false;
    for portal in server_portals {
        changed |= upsert_portal(commands, context, portal);
    }
    context.portals.retain(|key, info| {
        if server_keys.contains(key) {
            true
        } else {
            commands.entity(info.entity).despawn();
            changed = true;
            false
        }
    });

    if changed {
        *context.portal_set = PortalSet::rebuild(server_portals);
    }
}

// Idempotent spawn-or-move for one portal end; returns whether anything
// changed. Shared with the `SPortalOpened` cue, which may race the snapshot
// in either order.
pub(in crate::network) fn upsert_portal(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    portal: &Portal,
) -> bool {
    let key = (portal.owner, portal.end);
    if let Some(info) = context.portals.get(&key) {
        if info.portal == *portal {
            return false;
        }
        commands.entity(info.entity).despawn();
    }
    let entity = spawn_portal(commands, &context.portal_assets, portal);
    context.portals.insert(
        key,
        PortalInfo {
            entity,
            portal: *portal,
        },
    );
    true
}
