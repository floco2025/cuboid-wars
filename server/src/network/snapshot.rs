use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::{ActorMap, ActorMotionQuery, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    map::{LightState, WeatherState},
    players::{PlayerMap, PlayerStateQuery},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    config::{NetworkConfig, UpdateCadence},
    map::Carriers,
    protocol::*,
};

use super::broadcast::{
    broadcast_player_moves, broadcast_to_all, collect_actor_moves, collect_items, collect_player_moves,
    snapshot_active_players, snapshot_actors, snapshot_missiles, snapshot_spawning_actors,
};
use crate::{
    missiles::MissileMap,
    portals::{PortalAssignments, PortalMap},
};

// Bundled: Bevy systems take at most 16 parameters and this one is over.
#[derive(SystemParam)]
pub struct WorldConditions<'w> {
    carriers: Res<'w, Carriers>,
    weather: Res<'w, WeatherState>,
    light: Res<'w, LightState>,
    quests: Res<'w, QuestBoard>,
    quest_catalog: Res<'w, QuestCatalog>,
    portals: Res<'w, PortalMap>,
    portal_assignments: Res<'w, PortalAssignments>,
}

// The cadence ticks even on an empty server so it keeps its phase.
fn broadcast_due(
    cadence: &mut Option<UpdateCadence>,
    configured: impl FnOnce() -> UpdateCadence,
    players: &PlayerMap,
) -> bool {
    cadence.get_or_insert_with(configured).ready() && players.has_active_players()
}

pub(super) fn network_broadcast_player_moves_system(
    network: Res<NetworkConfig>,
    mut cadence: Local<Option<UpdateCadence>>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    player_data: PlayerStateQuery,
) {
    if !broadcast_due(&mut cadence, || network.update_cadence(), &players) {
        return;
    }
    let moves = collect_player_moves(&players, &player_data);
    if !moves.is_empty() {
        broadcast_player_moves(&players, tick.0, &moves);
    }
}

pub(super) fn network_broadcast_actor_moves_system(
    network: Res<NetworkConfig>,
    mut cadence: Local<Option<UpdateCadence>>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    actor_data: ActorStateQuery,
    motions: ActorMotionQuery,
    carriers: Res<Carriers>,
) {
    if !broadcast_due(&mut cadence, || network.update_cadence(), &players) {
        return;
    }
    let moves = collect_actor_moves(&actors, &actor_data, &motions, &carriers);
    if !moves.is_empty() {
        broadcast_to_all(&players, ServerMessage::ActorMoves(SActorMoves { tick: tick.0, moves }));
    }
}

pub(super) fn network_broadcast_snapshot_system(
    network: Res<NetworkConfig>,
    mut cadence: Local<Option<UpdateCadence>>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    pending_spawns: Res<PendingActorSpawns>,
    items: Res<ItemMap>,
    plates: Res<PlateState>,
    conditions: WorldConditions,
    player_data: PlayerStateQuery,
    actor_data: ActorStateQuery,
    actor_motions: ActorMotionQuery,
    item_positions: Query<&Position, With<ItemMarker>>,
    missiles: Res<MissileMap>,
) {
    if !broadcast_due(&mut cadence, || network.snapshot_cadence(), &players) {
        return;
    }

    let all_players = snapshot_active_players(&players, &player_data, &conditions.portal_assignments);
    let all_actors = snapshot_actors(&actors, &actor_data, &actor_motions, &conditions.carriers);
    let all_items = collect_items(&items, &item_positions);
    let all_missiles = snapshot_missiles(&missiles);

    let (quests, locked_switches) = conditions.quests.snapshot_fields(&conditions.quest_catalog, &players);
    let msg = ServerMessage::Snapshot(SSnapshot {
        tick: tick.0,
        players: all_players,
        actors: all_actors,
        actors_peaceful: actors.peaceful,
        spawning_actors: snapshot_spawning_actors(&pending_spawns),
        items: all_items,
        missiles: all_missiles,
        plates: (*plates).clone(),
        quests,
        locked_switches,
        rain_intensity: conditions.weather.intensity(),
        lighting: conditions.light.blend(),
        portals: conditions.portals.snapshot_portals(),
    });
    broadcast_to_all(&players, msg);
}
