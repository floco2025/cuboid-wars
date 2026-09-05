use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::{ActorMap, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    map::{LightState, PlateState, WeatherState},
    players::{PlayerMap, PlayerStateQuery},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    constants::SNAPSHOT_SECS,
    physics::CharacterVerticalVelocity,
    protocol::{ActorMarker, ItemMarker, PlayerMarker, *},
};

use super::broadcast::{
    broadcast_to_all, collect_items, collect_player_moves, snapshot_active_players, snapshot_actors, snapshot_missiles,
    snapshot_spawning_actors,
};
use crate::missiles::{MissileMap, MissileVelocity};
use crate::portals::{PortalAssignments, PortalMap};

// Bundled: Bevy systems take at most 16 parameters and this one is over.
#[derive(SystemParam)]
pub struct WorldConditions<'w> {
    weather: Res<'w, WeatherState>,
    light: Res<'w, LightState>,
    quests: Res<'w, QuestBoard>,
    quest_catalog: Res<'w, QuestCatalog>,
    portals: Res<'w, PortalMap>,
    portal_assignments: Res<'w, PortalAssignments>,
}

// Every active player's movement state after this tick's movement, to
// everyone, every tick.
pub(super) fn network_broadcast_player_moves_system(
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    player_data: PlayerStateQuery,
    motions: Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
) {
    let moves = collect_player_moves(&players, &player_data, &motions);
    if moves.is_empty() {
        return;
    }
    broadcast_to_all(
        &players,
        ServerMessage::PlayerMoves(SPlayerMoves { tick: tick.0, moves }),
    );
}

pub(super) fn network_broadcast_snapshot_system(
    time: Res<Time>,
    mut timer: Local<f32>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    pending_spawns: Res<PendingActorSpawns>,
    items: Res<ItemMap>,
    plates: Res<PlateState>,
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

    if !players.has_active_players() {
        return;
    }

    let all_players = snapshot_active_players(&players, &player_data, &motions, &conditions.portal_assignments);
    let all_actors = snapshot_actors(&actors, &actor_data, &actor_motions);
    let all_items = collect_items(&items, &item_positions);
    let all_missiles = snapshot_missiles(&missiles, &missile_data);

    let (quests, locked_plate_purposes) = conditions.quests.snapshot_fields(&conditions.quest_catalog, &players);
    let msg = ServerMessage::Snapshot(SSnapshot {
        tick: tick.0,
        players: all_players,
        actors: all_actors,
        spawning_actors: snapshot_spawning_actors(&pending_spawns),
        items: all_items,
        missiles: all_missiles,
        plates: (*plates).clone(),
        quests,
        locked_plate_purposes,
        rain_intensity: conditions.weather.intensity(),
        lighting: conditions.light.blend(),
        portals: conditions.portals.snapshot_portals(),
    });
    broadcast_to_all(&players, msg);
}
