use bevy::prelude::*;
use std::collections::HashSet;

use super::components::ServerReconciliation;
use crate::{
    resources::{RoundTripTime, SentryInfo, SentryMap},
    spawning::spawn_sentry,
};
use common::{markers::SentryMarker, protocol::*};

// ============================================================================
// Sentry Message Handlers
// ============================================================================

/// Handle individual sentry update with reconciliation.
pub fn handle_sentry_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    sentries: &mut ResMut<SentryMap>,
    rtt: &ResMut<RoundTripTime>,
    sentry_query: &Query<&Position, With<SentryMarker>>,
    msg: SSentry,
    asset_server: &Res<AssetServer>,
) {
    if let Some(sentry_info) = sentries.0.get(&msg.id) {
        // Update existing sentry with reconciliation
        if let Ok(client_pos) = sentry_query.get(sentry_info.entity) {
            commands.entity(sentry_info.entity).insert((
                msg.sentry.vel,
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.sentry.pos,
                    server_vel: msg.sentry.vel,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
            ));
        } else {
            // No client position yet, just set server state
            commands.entity(sentry_info.entity).insert((
                msg.sentry.pos,
                msg.sentry.vel,
            ));
        }
    } else {
        // Spawn new sentry
        let entity = spawn_sentry(
            commands,
            meshes,
            materials,
            asset_server,
            graphs,
            msg.id,
            &msg.sentry.pos,
            &msg.sentry.vel,
        );
        sentries.0.insert(msg.id, SentryInfo { entity });
    }
}

/// Handle sentry hitting player - play sound effect.
pub fn handle_sentry_hit_message(commands: &mut Commands, _msg: SSentryHit, asset_server: &AssetServer) {
    // Play sound - this message is only sent to the player who was hit
    commands.spawn((
        AudioPlayer::new(asset_server.load("sounds/sentry_hits_player.wav")),
        PlaybackSettings::DESPAWN,
    ));
}

// ============================================================================
// Sentry Synchronization Helper
// ============================================================================

/// Synchronize sentries from bulk Update message - spawn/despawn/reconcile.
pub fn sync_sentries(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    sentries: &mut ResMut<SentryMap>,
    rtt: &ResMut<RoundTripTime>,
    sentry_query: &Query<&Position, With<SentryMarker>>,
    server_sentries: &[(SentryId, Sentry)],
    asset_server: &Res<AssetServer>,
) {
    let server_sentry_ids: HashSet<SentryId> = server_sentries.iter().map(|(id, _)| *id).collect();

    // Spawn any sentries that appear in the update but are missing locally
    for (sentry_id, server_sentry) in server_sentries {
        if sentries.0.contains_key(sentry_id) {
            continue;
        }
        let entity = spawn_sentry(
            commands,
            meshes,
            materials,
            asset_server,
            graphs,
            *sentry_id,
            &server_sentry.pos,
            &server_sentry.vel,
        );
        sentries.0.insert(*sentry_id, SentryInfo { entity });
    }

    // Despawn sentries no longer present in the authoritative snapshot
    sentries.0.retain(|id, sentry_info| {
        if server_sentry_ids.contains(id) {
            true
        } else {
            commands.entity(sentry_info.entity).despawn();
            false
        }
    });

    // Update existing sentries with server state (position and velocity)
    for (sentry_id, server_sentry) in server_sentries {
        if let Some(client_sentry) = sentries.0.get(sentry_id) {
            // Check if we have a client position to track reconciliation
            if let Ok(client_pos) = sentry_query.get(client_sentry.entity) {
                commands.entity(client_sentry.entity).insert((
                    server_sentry.vel,
                    ServerReconciliation {
                        client_pos: *client_pos,
                        server_pos: server_sentry.pos,
                        server_vel: server_sentry.vel,
                        timer: 0.0,
                        rtt: rtt.rtt.as_secs_f32(),
                    },
                ));
            } else {
                // No client position yet, just set server state
                commands
                    .entity(client_sentry.entity)
                    .insert((server_sentry.pos, server_sentry.vel));
            }
        }
    }
}
