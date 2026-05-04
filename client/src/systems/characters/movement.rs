use bevy::prelude::*;

use crate::{
    config::AssetSet,
    resources::{ActorMap, PlayerMap},
    systems::{
        BumpFlashMarker,
        actors::{ActorMovementQuery, actor_start_positions, apply_actor_moves, plan_actor_moves},
        players::{PlayerMovementQuery, apply_player_moves, plan_player_moves},
    },
};
use common::{config::GameplayConfig, physics::CollisionWorld};

pub fn characters_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Option<Res<CollisionWorld>>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    mut players_query: PlayerMovementQuery,
    mut actors_query: ActorMovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashMarker>>,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts = actor_start_positions(&actors_query);

    plan_player_moves(
        &mut commands,
        delta,
        collision_world.as_deref(),
        &gameplay_config,
        &players,
        &mut players_query,
        &mut bump_flash_ui,
        &mut planned_moves,
    );
    plan_actor_moves(
        &mut commands,
        delta,
        collision_world.as_deref(),
        &gameplay_config,
        &actors,
        &actor_starts,
        &mut actors_query,
        &mut planned_moves,
    );
    apply_player_moves(
        &mut commands,
        delta,
        &asset_server,
        &asset_set,
        &mut players_query,
        &mut bump_flash_ui,
        &planned_moves,
    );
    apply_actor_moves(&mut actors_query, &planned_moves);
}
