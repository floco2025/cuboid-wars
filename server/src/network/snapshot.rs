use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::{ActorMap, ActorMotionQuery, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    map::WeatherState,
    players::{PlayerMap, PlayerStateQuery},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    celestial::CelestialClockAnchor,
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
    celestial_clock: Res<'w, CelestialClockAnchor>,
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

#[derive(SystemParam)]
pub(super) struct SnapshotSource<'w, 's> {
    tick: Res<'w, ServerTick>,
    players: Res<'w, PlayerMap>,
    actors: Res<'w, ActorMap>,
    pending_spawns: Res<'w, PendingActorSpawns>,
    items: Res<'w, ItemMap>,
    switch_state: Res<'w, SwitchState>,
    conditions: WorldConditions<'w>,
    player_data: PlayerStateQuery<'w, 's>,
    actor_data: ActorStateQuery<'w, 's>,
    actor_motions: ActorMotionQuery<'w, 's>,
    item_positions: Query<'w, 's, &'static Position, With<ItemMarker>>,
    missiles: Res<'w, MissileMap>,
}

impl SnapshotSource<'_, '_> {
    fn capture(&self) -> SSnapshot {
        let (quests, locked_switches) = self
            .conditions
            .quests
            .snapshot_fields(&self.conditions.quest_catalog, &self.players);
        SSnapshot {
            tick: self.tick.0,
            players: snapshot_active_players(&self.players, &self.player_data, &self.conditions.portal_assignments),
            actors: snapshot_actors(
                &self.actors,
                &self.actor_data,
                &self.actor_motions,
                &self.conditions.carriers,
            ),
            actors_peaceful: self.actors.peaceful,
            spawning_actors: snapshot_spawning_actors(&self.pending_spawns),
            items: collect_items(&self.items, &self.item_positions),
            missiles: snapshot_missiles(&self.missiles),
            switch_state: (*self.switch_state).clone(),
            quests,
            shared_checkpoint: self.players.shared_checkpoint.number,
            locked_switches,
            cloud_cover: self.conditions.weather.cloud_cover(),
            raining: self.conditions.weather.is_raining(),
            celestial_clock: *self.conditions.celestial_clock,
            portals: self.conditions.portals.snapshot_portals(),
        }
    }
}

pub(super) fn network_broadcast_snapshot_system(
    network: Res<NetworkConfig>,
    mut cadence: Local<Option<UpdateCadence>>,
    source: SnapshotSource,
) {
    if broadcast_due(&mut cadence, || network.snapshot_cadence(), &source.players) {
        broadcast_to_all(&source.players, ServerMessage::Snapshot(source.capture()));
    }
}

/// Observe a completed server tick without advancing it or changing network cadence.
/// The experiment viewer uses the same state projection as normal clients.
pub fn capture_snapshot(world: &mut World) -> SSnapshot {
    use bevy::ecs::system::RunSystemOnce;
    world
        .run_system_once(capture_snapshot_system)
        .expect("snapshot resources installed")
}

fn capture_snapshot_system(source: SnapshotSource) -> SSnapshot {
    source.capture()
}
