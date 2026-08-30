use bevy::{ecs::system::SystemParam, prelude::*};
use common::{config::GameplayConfig, protocol::*};

use super::{
    actors::{sync_actors, sync_spawning_actors},
    items::sync_items,
    missiles::sync_missiles,
    players::{PlayerSnapshotAssets, PlayerSnapshotState, sync_players},
};
use crate::{
    actors::{ActorGhostMap, ActorMap},
    barriers::{BarrierAssets, LockedPlatePurposes, OpenBarrierKinds},
    cameras::MainCameraMarker,
    characters::MaxHealth,
    config::{AssetSet, ClientSettings},
    items::{ItemAssets, ItemMap},
    map::skybox::LightingState,
    missiles::{MissileAssets, MissileMap},
    network::{LastSnapshotSeq, RoundTripTime},
    players::{LocalPlayerInfo, PlayerMap},
    ui::{HudBanner, QuestLog},
    vfx::RainIntensity,
};

#[derive(SystemParam)]
pub(super) struct SnapshotAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    graphs: ResMut<'w, Assets<AnimationGraph>>,
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
    gameplay_config: Res<'w, GameplayConfig>,
    max_health: Res<'w, MaxHealth>,
    item_assets: Res<'w, ItemAssets>,
    barrier_assets: Res<'w, BarrierAssets>,
    missile_assets: Res<'w, MissileAssets>,
}

#[derive(SystemParam)]
pub(super) struct SnapshotWorldState<'w, 's> {
    players: ResMut<'w, PlayerMap>,
    actors: ResMut<'w, ActorMap>,
    items: ResMut<'w, ItemMap>,
    last_snapshot_seq: ResMut<'w, LastSnapshotSeq>,
    local_player_info: ResMut<'w, LocalPlayerInfo>,
    quest_log: ResMut<'w, QuestLog>,
    banner: ResMut<'w, HudBanner>,
    actor_ghosts: ResMut<'w, ActorGhostMap>,
    missiles: ResMut<'w, MissileMap>,
    missile_data: Query<'w, 's, &'static Position, With<MissileMarker>>,
    open_barrier_kinds: ResMut<'w, OpenBarrierKinds>,
    locked_plate_purposes: ResMut<'w, LockedPlatePurposes>,
    rain_intensity: ResMut<'w, RainIntensity>,
    lighting: ResMut<'w, LightingState>,
}

#[derive(SystemParam)]
pub(super) struct SnapshotMessageContext<'w, 's> {
    assets: SnapshotAssets<'w>,
    world: SnapshotWorldState<'w, 's>,
    player_data: Query<'w, 's, (&'static Position, &'static PlayerMoveIntent, &'static FaceYaw), With<PlayerMarker>>,
    actor_data: Query<'w, 's, (&'static Position, &'static ActorMoveIntent, &'static FaceYaw), With<ActorMarker>>,
    cameras: Query<'w, 's, Entity, (With<Camera3d>, With<MainCameraMarker>)>,
}

pub(super) fn handle_snapshot_message(
    message: &SSnapshot,
    commands: &mut Commands,
    my_player_id: PlayerId,
    rtt: &RoundTripTime,
    context: &mut SnapshotMessageContext,
) {
    let assets = &mut context.assets;
    let world = &mut context.world;
    if !world.last_snapshot_seq.should_accept(message.seq) {
        warn!(
            "Ignoring outdated SSnapshot (seq: {}, last: {})",
            message.seq,
            world
                .last_snapshot_seq
                .last_raw()
                .map_or_else(|| "none".to_string(), |seq| seq.to_string())
        );
        return;
    }

    world.last_snapshot_seq.record(message.seq);

    let mut player_assets = PlayerSnapshotAssets {
        meshes: &mut assets.meshes,
        materials: &mut assets.materials,
        images: &mut assets.images,
        graphs: &mut assets.graphs,
        asset_server: &assets.asset_server,
        asset_set: &assets.asset_set,
        client_settings: &assets.client_settings,
        gameplay_config: &assets.gameplay_config,
        max_health: &assets.max_health,
    };
    // Avoid marking an untouched quest log as changed on every snapshot.
    if !message.quests.is_empty() {
        world.quest_log.apply_group_status(&message.quests);
    }

    let mut player_state = PlayerSnapshotState {
        players: &mut world.players,
        rtt,
        local_player_info: &mut world.local_player_info,
        quest_log: &world.quest_log,
        banner: &mut world.banner,
        my_player_id,
    };
    sync_players(
        commands,
        &mut player_assets,
        &mut player_state,
        &context.player_data,
        &context.cameras,
        &message.players,
    );
    sync_actors(
        commands,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.graphs,
        &mut world.actors,
        rtt,
        &context.actor_data,
        &assets.asset_server,
        &assets.asset_set,
        &assets.client_settings,
        &assets.gameplay_config,
        &assets.max_health,
        &message.actors,
    );
    sync_spawning_actors(
        commands,
        &mut world.actor_ghosts,
        &assets.asset_server,
        &assets.asset_set,
        &assets.gameplay_config,
        &message.spawning_actors,
    );
    sync_items(
        commands,
        &assets.item_assets,
        &assets.barrier_assets,
        &assets.missile_assets,
        &mut world.items,
        &message.items,
    );
    sync_missiles(
        commands,
        &assets.missile_assets,
        &mut world.missiles,
        rtt,
        &world.missile_data,
        &message.missiles,
    );

    // The server sorts these vectors, so equality is stable across snapshots.
    if message.open_barrier_kinds != world.open_barrier_kinds.0 {
        world.open_barrier_kinds.0.clone_from(&message.open_barrier_kinds);
    }
    if world.locked_plate_purposes.0 != message.locked_plate_purposes {
        world.locked_plate_purposes.0.clone_from(&message.locked_plate_purposes);
    }

    world.rain_intensity.target = message.rain_intensity;
    world.lighting.target.clone_from(&message.lighting);
    world.lighting.synced = true;
}
