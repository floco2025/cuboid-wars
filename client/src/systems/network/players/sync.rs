use bevy::prelude::*;
use std::collections::HashSet;

use super::player_movement_velocity;
use crate::{
    config::{AssetSet, RenderSettings},
    markers::MainCameraMarker,
    resources::{PlayerInfo, PlayerMap, RoundTripTime},
    spawning::spawn_player,
    systems::ServerReconciliation,
};
use common::{
    config::GameplayConfig,
    markers::PlayerMarker,
    physics::CharacterVerticalMotion,
    protocol::{CharacterMoveIntent, FaceDirection, Player, PlayerId, Position},
};

pub fn sync_players(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &CharacterMoveIntent, &FaceDirection), With<PlayerMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    server_players: &[(PlayerId, Player)],
) {
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

    for (id, player) in server_players {
        if players.0.contains_key(id) {
            continue;
        }

        spawn_snapshot_player(
            commands,
            meshes,
            materials,
            images,
            graphs,
            players,
            camera_query,
            my_player_id,
            asset_server,
            asset_set,
            render_settings,
            gameplay_config,
            *id,
            player,
        );
    }

    players.0.retain(|id, player| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(player.entity).despawn();
            false
        }
    });

    for (id, server_player) in server_players {
        update_snapshot_player(
            commands,
            players,
            rtt,
            player_data,
            my_player_id,
            gameplay_config,
            *id,
            server_player,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy spawn path needs asset resources at the call site"
)]
fn spawn_snapshot_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    id: PlayerId,
    player: &Player,
) {
    let is_local = id == my_player_id;
    debug!("spawning player {:?} from Update (is_local: {})", id, is_local);
    let entity = spawn_player(
        commands,
        asset_server,
        meshes,
        materials,
        images,
        graphs,
        asset_set,
        render_settings,
        gameplay_config,
        id.0,
        &player.name,
        &player.movement.pos,
        player.movement.move_intent,
        player.face_dir,
        is_local,
    );
    commands
        .entity(entity)
        .insert(CharacterVerticalMotion(player.movement.vertical_velocity));

    if is_local && let Ok(camera_entity) = camera_query.single() {
        let camera_rotation = player.face_dir + std::f32::consts::PI;
        commands.entity(camera_entity).insert(
            Transform::from_xyz(player.movement.pos.x, 2.5, player.movement.pos.z + 3.0)
                .with_rotation(Quat::from_rotation_y(camera_rotation)),
        );
    }

    players.0.insert(
        id,
        PlayerInfo {
            entity,
            hits: player.hits,
            name: player.name.clone(),
            speed_power_up: player.speed_power_up,
            multi_shot_power_up: player.multi_shot_power_up,
            phasing_power_up: player.phasing_power_up,
            stunned: player.stunned,
        },
    );
}

fn update_snapshot_player(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &CharacterMoveIntent, &FaceDirection), With<PlayerMarker>>,
    my_player_id: PlayerId,
    gameplay_config: &GameplayConfig,
    id: PlayerId,
    server_player: &Player,
) {
    if let Some(client_player) = players.0.get_mut(&id) {
        if let Ok((client_pos, _, _)) = player_data.get(client_player.entity) {
            let server_velocity = player_movement_velocity(
                server_player.movement,
                gameplay_config.characters.player.speed,
                server_player.speed_power_up,
            );

            if id != my_player_id {
                commands
                    .entity(client_player.entity)
                    .insert(server_player.movement.move_intent);
            }
            commands.entity(client_player.entity).insert(ServerReconciliation {
                client_pos: *client_pos,
                server_pos: server_player.movement.pos,
                server_velocity,
                timer: 0.0,
                rtt: rtt.rtt.as_secs_f32(),
            });
            if id != my_player_id {
                commands
                    .entity(client_player.entity)
                    .insert(CharacterVerticalMotion(server_player.movement.vertical_velocity));
            }
        }

        client_player.hits = server_player.hits;
        client_player.speed_power_up = server_player.speed_power_up;
        client_player.multi_shot_power_up = server_player.multi_shot_power_up;
        client_player.phasing_power_up = server_player.phasing_power_up;
    }
}
