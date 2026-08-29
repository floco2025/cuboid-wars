use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::{ActorMap, PendingActorSpawns},
    config::ServerGameplayConfig,
    items::ItemMap,
    map::{LightState, OpenBarrierKinds, WeatherState},
    players::PlayerMap,
    quests::QuestBoard,
};
use common::{
    constants::SNAPSHOT_SECS,
    physics::CharacterVerticalVelocity,
    protocol::{ActorMarker, ItemMarker, PlayerMarker, *},
};

use super::incoming::{ActorStateQuery, PlayerStateQuery};

use super::broadcast::{
    broadcast_to_all, collect_items, snapshot_actors, snapshot_logged_in_players, snapshot_missiles,
    snapshot_spawning_actors,
};
use crate::missiles::{MissileMap, MissileVelocity};

// Bundled: Bevy systems take at most 16 parameters and this one is over.
#[derive(SystemParam)]
pub struct WorldConditions<'w> {
    weather: Res<'w, WeatherState>,
    light: Res<'w, LightState>,
    quests: Res<'w, QuestBoard>,
    config: Res<'w, ServerGameplayConfig>,
}

pub fn network_broadcast_snapshot_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut seq: Local<u32>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    pending_spawns: Res<PendingActorSpawns>,
    items: Res<ItemMap>,
    open_barrier_kinds: Res<OpenBarrierKinds>,
    conditions: WorldConditions,
    player_data: PlayerStateQuery,
    motions: Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    actor_data: ActorStateQuery,
    actor_motions: Query<&CharacterVerticalVelocity, With<ActorMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    missiles: Res<MissileMap>,
    missile_data: Query<(&Position, &MissileVelocity), With<MissileMarker>>,
) {
    *timer += time.delta_secs();
    if *timer < SNAPSHOT_SECS {
        return;
    }
    // Carry the phase remainder rather than zeroing, so the long-run rate
    // holds at SNAPSHOT_HZ instead of drifting slower by the leftover each tick.
    *timer -= SNAPSHOT_SECS;

    if players.all_logged_out() {
        return;
    }

    *seq = seq.wrapping_add(1);

    let all_players = snapshot_logged_in_players(&players, &player_data, &motions);
    let all_actors = snapshot_actors(&actors, &actor_data, &actor_motions);
    let all_items = collect_items(&items, &item_positions);
    let all_missiles = snapshot_missiles(&missiles, &missile_data);

    let msg = ServerMessage::Snapshot(SSnapshot {
        seq: *seq,
        players: all_players,
        actors: all_actors,
        spawning_actors: snapshot_spawning_actors(&pending_spawns),
        items: all_items,
        missiles: all_missiles,
        open_barrier_kinds: open_barrier_kinds.0.clone(),
        quests: conditions.quests.snapshot(&conditions.config.quests, &players),
        firework_plates_active: conditions.quests.fireworks_active(&conditions.config.quests),
        rain_intensity: conditions.weather.intensity(),
        lighting: conditions.light.blend(),
    });
    broadcast_to_all(&players, msg);
}
