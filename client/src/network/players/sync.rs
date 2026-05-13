use bevy::prelude::*;
use std::collections::HashSet;

use super::handlers::player_movement_velocity;
use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, RenderSettings},
    network::{RoundTripTime, ServerReconciliation},
    players::{LocalPlayerInfo, PlayerInfo, PlayerMap, spawn_player},
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{FaceDirection, Player, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

#[expect(
    clippy::too_many_arguments,
    reason = "snapshot reconciliation needs the asset stack at this entry point"
)]
pub fn sync_players(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    local_player_info: &mut LocalPlayerInfo,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    server_players: &[(PlayerId, Player)],
) {
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

    // Spawn newly-appeared players. Skip the local player if it's already in
    // the map (e.g., we kept its entity through death — see respawn handling
    // further down).
    for (id, player) in server_players {
        if players.contains_key(id) {
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

    // Handle players no longer in the snapshot:
    //   * remote players → despawn entity, drop PlayerInfo (logoff / death of
    //     another player both look identical from this side).
    //   * the local player → keep entity (camera/mouse-look need it), hide
    //     visibility, keep PlayerInfo (preserves score), insert
    //     LocalPlayerDead. The next snapshot that re-includes our id will
    //     teleport us to the respawn position.
    let mut local_just_died = false;
    players.retain(|id, player| {
        if update_ids.contains(id) {
            return true;
        }
        if *id == my_player_id {
            commands.entity(player.entity).insert(Visibility::Hidden);
            local_just_died = true;
            true
        } else {
            commands.entity(player.entity).despawn();
            false
        }
    });
    if local_just_died {
        local_player_info.is_dead = true;
    }

    // Handle local-player respawn: if we were dead and our id reappeared in
    // this snapshot, hard-teleport our existing entity to the new spawn
    // position, restore visibility, and clear the death state.
    if local_player_info.is_dead
        && let Some((_, server_player)) = server_players.iter().find(|(id, _)| *id == my_player_id)
        && let Some(info) = players.get(&my_player_id)
    {
        commands.entity(info.entity).insert((
            server_player.movement.pos,
            // Reset the previous-tick anchor so render interpolation doesn't
            // smear the respawn teleport across one render frame.
            PreviousTickPosition(server_player.movement.pos),
            FaceDirection(server_player.face_dir),
            CharacterVerticalVelocity(server_player.movement.vertical_velocity),
            server_player.health,
            Visibility::Visible,
        ));
        local_player_info.is_dead = false;
    }

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
        player.health,
        player.face_dir,
        is_local,
    );
    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(player.movement.vertical_velocity));

    if is_local && let Ok(camera_entity) = camera_query.single() {
        let camera_rotation = player.face_dir + std::f32::consts::PI;
        commands.entity(camera_entity).insert(
            Transform::from_xyz(player.movement.pos.x, 2.5, player.movement.pos.z + 3.0)
                .with_rotation(Quat::from_rotation_y(camera_rotation)),
        );
    }

    players.insert(
        id,
        PlayerInfo {
            entity,
            score: player.score,
            name: player.name.clone(),
            speed_power_up: player.speed_power_up,
            multi_shot_power_up: player.multi_shot_power_up,
            phasing_power_up: player.phasing_power_up,
            anti_gravity_power_up: player.anti_gravity_power_up,
            stunned: player.stunned,
            held_keys: Vec::new(),
        },
    );
}

fn update_snapshot_player(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    my_player_id: PlayerId,
    gameplay_config: &GameplayConfig,
    id: PlayerId,
    server_player: &Player,
) {
    if let Some(client_player) = players.get_mut(&id) {
        if let Ok((client_pos, _, _)) = player_data.get(client_player.entity) {
            let server_velocity = player_movement_velocity(
                server_player.movement,
                gameplay_config.player.walk_speed,
                gameplay_config.player.run_speed,
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
                    .insert(CharacterVerticalVelocity(server_player.movement.vertical_velocity));
            }
        }

        client_player.score = server_player.score;
        client_player.speed_power_up = server_player.speed_power_up;
        client_player.multi_shot_power_up = server_player.multi_shot_power_up;
        client_player.phasing_power_up = server_player.phasing_power_up;
        commands.entity(client_player.entity).insert(server_player.health);
    }
}
