use bevy::prelude::*;
use std::f32::consts::PI;

use super::{super::context::ServerMessageContext, snap_player};
use crate::{
    network::SampleTiming,
    players::{
        LocalPlayerInfo, PlayerInfo, PlayerMap, PlayerSpawnContext, PortalTransitBlend, RemotePlayerMotion,
        spawn_player,
    },
    ui::BannerMessage,
};
use common::{
    map::Carriers,
    protocol::{Player, PlayerGeneration, PlayerId, SPlayerRelocated, sequence_is_newer},
};

pub(in crate::network) fn sync_players(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    tick: u32,
    server_players: &[(PlayerId, Player)],
) {
    for (id, player) in server_players {
        sync_player(commands, context, my_player_id, tick, *id, player);
    }
    for (id, generation, entity) in absent_bodies(&context.players, tick, server_players) {
        context.players.retire_body(id, generation);
        if id == my_player_id {
            commands.entity(entity).insert(Visibility::Hidden);
            context.local_player_info.is_dead = true;
            context.local_player_info.reports.clear_crossings();
        } else {
            commands.entity(entity).despawn();
            context.players.remove(&id);
        }
    }
}

// Bodies the snapshot no longer lists, except those placed after the tick it describes.
fn absent_bodies(
    players: &PlayerMap,
    tick: u32,
    server_players: &[(PlayerId, Player)],
) -> Vec<(PlayerId, PlayerGeneration, Entity)> {
    players
        .iter()
        .filter(|(id, info)| {
            !server_players.iter().any(|(present, _)| present == *id) && !sequence_is_newer(info.spawn_tick, tick)
        })
        .map(|(id, info)| (*id, info.generation, info.entity))
        .collect()
}

pub(in crate::network) fn handle_player_relocated_message(
    message: SPlayerRelocated,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if context
        .players
        .get(&message.id)
        .is_some_and(|info| info.generation == message.player.generation)
    {
        return;
    }
    sync_player(
        commands,
        context,
        context.my_player_id.0,
        message.tick,
        message.id,
        &message.player,
    );
}

fn sync_player(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    tick: u32,
    id: PlayerId,
    player: &Player,
) {
    if !context.players.accepts_generation(id, player.generation) {
        return;
    }
    let is_local = id == my_player_id;
    let was_dead = is_local && context.local_player_info.is_dead;
    let sample_timing = SampleTiming::new(&context.client_settings.interpolation, &context.network);
    let body_changed = match context.players.get_mut(&id) {
        Some(info) => place_player_body(
            commands,
            info,
            &mut context.local_player_info,
            is_local,
            tick,
            player,
            &context.carriers,
            sample_timing,
        ),
        None => {
            let entity = spawn_player(
                commands,
                PlayerSpawnContext {
                    carriers: &context.carriers,
                    asset_server: &context.assets.asset_server,
                    meshes: &mut context.meshes,
                    materials: &mut context.materials,
                    images: &mut context.images,
                    asset_set: &context.assets.asset_set,
                    client_settings: &context.client_settings,
                    gameplay_config: &context.gameplay_config,
                    max_health: context.assets.max_health.player,
                    sample_timing,
                },
                id,
                player,
                is_local,
            );
            context
                .players
                .insert(id, PlayerInfo::from_snapshot(entity, player, tick));
            if is_local {
                begin_local_body(&mut context.local_player_info, player);
            }
            true
        }
    };
    if is_local {
        *context.portal_access = player.portal_access;
        if body_changed {
            if let Ok(camera) = context.cameras.single() {
                commands.entity(camera).remove::<PortalTransitBlend>();
            }
            if was_dead && let Some(reminder) = context.quest_log.reminder() {
                context.banner.push(BannerMessage::QuestAnnouncement(reminder));
            }
        }
    }
    if let Some(info) = context.players.get_mut(&id) {
        info.apply_snapshot(player);
        commands.entity(info.entity).insert(player.health);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "placing a body touches the map entry, the local state, and the buffer timing"
)]
fn place_player_body(
    commands: &mut Commands,
    info: &mut PlayerInfo,
    local: &mut LocalPlayerInfo,
    is_local: bool,
    tick: u32,
    player: &Player,
    carriers: &Carriers,
    sample_timing: SampleTiming,
) -> bool {
    if !player.generation.is_newer_than(info.generation) {
        return false;
    }
    info.generation = player.generation;
    info.last_movement_tick = tick;
    info.spawn_tick = tick;
    snap_player(commands, info, &player.movement, carriers);
    commands.entity(info.entity).insert(Visibility::Visible);
    if is_local {
        begin_local_body(local, player);
    } else {
        commands
            .entity(info.entity)
            .insert(RemotePlayerMotion::new(player.movement, sample_timing));
    }
    true
}

fn begin_local_body(local: &mut LocalPlayerInfo, player: &Player) {
    local.stored_yaw = player.movement.face_yaw + PI;
    local.stored_pitch = 0.0;
    local.reports.begin_body(player.generation);
    local.is_dead = false;
}

#[cfg(test)]
#[path = "tests/sync.rs"]
mod tests;
