use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::{ActorMap, ActorStateQuery, PendingActorSpawns},
    items::ItemMap,
    map::{LightState, OpenBarrierKinds, WeatherState},
    players::{PlayerMap, PlayerStateQuery},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    constants::SNAPSHOT_SECS,
    physics::CharacterVerticalVelocity,
    protocol::{ActorMarker, ItemMarker, PlayerMarker, *},
};

use super::broadcast::{
    broadcast_to_all, collect_items, snapshot_active_players, snapshot_actors, snapshot_missiles,
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

pub(super) fn network_broadcast_snapshot_system(
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

    if !players.has_active_players() {
        return;
    }

    *seq = seq.wrapping_add(1);

    let all_players = snapshot_active_players(&players, &player_data, &motions, &conditions.portal_assignments);
    let all_actors = snapshot_actors(&actors, &actor_data, &actor_motions);
    let all_items = collect_items(&items, &item_positions);
    let all_missiles = snapshot_missiles(&missiles, &missile_data);

    let (quests, locked_plate_purposes) = conditions.quests.snapshot_fields(&conditions.quest_catalog, &players);
    let msg = ServerMessage::Snapshot(SSnapshot {
        seq: *seq,
        players: all_players,
        actors: all_actors,
        spawning_actors: snapshot_spawning_actors(&pending_spawns),
        items: all_items,
        missiles: all_missiles,
        open_barrier_kinds: open_barrier_kinds.0.clone(),
        quests,
        locked_plate_purposes,
        rain_intensity: conditions.weather.intensity(),
        lighting: conditions.light.blend(),
        portals: conditions.portals.snapshot_portals(),
    });
    broadcast_to_all(&players, msg);
}
