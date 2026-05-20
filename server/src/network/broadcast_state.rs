use bevy::prelude::*;

use crate::{
    map::OpenBarrierKinds,
    resources::{ActorMap, ItemMap, PlayerMap},
};
use common::{
    constants::SNAPSHOT_SECS,
    physics::CharacterVerticalVelocity,
    protocol::{ActorMarker, ItemMarker, PlayerMarker, *},
};

use super::broadcast::{broadcast_to_all, collect_items, snapshot_actors, snapshot_logged_in_players};

pub fn network_broadcast_snapshot_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut seq: Local<u32>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    items: Res<ItemMap>,
    open_barrier_kinds: Res<OpenBarrierKinds>,
    player_data: Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    motions: Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    actor_data: Query<(&Position, &ActorMoveIntent, &FaceDirection, &Health), With<ActorMarker>>,
    actor_motions: Query<&CharacterVerticalVelocity, With<ActorMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
) {
    *timer += time.delta_secs();
    if *timer < SNAPSHOT_SECS {
        return;
    }
    *timer = 0.0;

    *seq = seq.wrapping_add(1);

    if players.values().all(|info| !info.logged_in) {
        return;
    }

    let all_players = snapshot_logged_in_players(&players, &player_data, &motions);
    let all_actors = snapshot_actors(&actors, &actor_data, &actor_motions);
    let all_items = collect_items(&items, &item_positions);

    let msg = ServerMessage::Snapshot(SSnapshot {
        seq: *seq,
        players: all_players,
        actors: all_actors,
        items: all_items,
        open_barrier_kinds: open_barrier_kinds.0.clone(),
    });
    broadcast_to_all(&players, msg);
}
