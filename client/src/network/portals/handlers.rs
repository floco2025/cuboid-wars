use bevy::prelude::*;

use super::{super::context::ServerMessageContext, sync::upsert_portal};
use crate::audio::play_spatial_sound;
use common::{physics::PortalSet, protocol::*};

// A portal end was placed or moved. Latency cue ahead of the snapshot —
// idempotent against a racing snapshot having spawned it already.
pub(in crate::network) fn handle_portal_opened_message(
    message: SPortalOpened,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if upsert_portal(commands, context, &message.portal) {
        if let Some(collision_world) = context.collision_world.as_deref() {
            *context.portal_set = PortalSet::rebuild(&context.portals.wire_portals(), collision_world);
        }
        play_spatial_sound(
            commands,
            &context.asset_server,
            context.asset_set.player_sound("plate_press"),
            &context.client_settings.audio,
            Vec3::from(message.portal.pos),
        );
    }
}
