use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::switches::{PlateEdges, PressureSwitches};

use crate::{
    config::{FeedConfig, ServerGameplayConfig},
    map::{MapConfig, PressurePlateRuntime},
    network::{FeedAudience, FeedEvent, broadcast_firework_show, broadcast_to_all, emit_feed},
    players::PlayerMap,
    quests::{QuestBoard, QuestCatalog, QuestEvent, record_event},
};
use common::{
    map::{Carriers, MapGeometry},
    protocol::{
        BarrierKindTable, BridgeKindTable, HeldPurpose, PlatePurpose, PlateState, PlayerId, PlayerMarker, Position,
        SPressurePlate, ServerMessage,
    },
};

// Is `pos`, in the plate's carrier frame, inside this plate's inner
// 25%-by-area square AND on the plate's level? Y matches within half a
// storey of the plate's floor, which keeps a player on the floor above from
// triggering a plate one level down.
#[must_use]
pub fn player_on_plate(plate: &PressurePlateRuntime, pos: &Position, geometry: &MapGeometry) -> bool {
    if (pos.y - geometry.level_y(plate.level)).abs() >= geometry.level_height() / 2.0 {
        return false;
    }
    let cell = geometry.cell_size();
    let cell_x = geometry.cell_to_world_x(plate.col);
    let cell_z = geometry.cell_to_world_z(plate.row);
    let min_x = cell_x + cell * 0.25;
    let max_x = cell_x + cell * 0.75;
    let min_z = cell_z + cell * 0.25;
    let max_z = cell_z + cell * 0.75;
    pos.x >= min_x && pos.x <= max_x && pos.z >= min_z && pos.z <= max_z
}

// Barrier and bridge kinds use their configured activation: any occupied plate
// for momentary, each fresh plate press for toggle, and toggle with exactly one
// logged-in player for auto. Entering auto toggle seeds from current occupancy.
// Fireworks require min(plate count, alive player count), with at least one alive
// player, and fire only on the threshold's rising edge.
pub(crate) fn pressure_plates_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut quest_board: ResMut<QuestBoard>,
    quest_catalog: Res<QuestCatalog>,
    barrier_kinds: Res<BarrierKindTable>,
    bridge_kinds: Res<BridgeKindTable>,
    positions: Query<&Position, With<PlayerMarker>>,
    plates_state: Res<PlateState>,
    mut switches: ResMut<PressureSwitches>,
) {
    let mut logged_in: usize = 0;
    let mut alive: usize = 0;
    for (_, info) in players.iter() {
        if info.connection.logged_in {
            logged_in += 1;
            if !info.is_dead() {
                alive += 1;
            }
        }
    }

    let plates = &map_config.pressure_plates;
    let holders = plate_holders(
        &map_config,
        &carriers,
        &players,
        &positions,
        quest_board.locked_plate_purposes(),
    );
    let held: HashSet<usize> = holders.keys().copied().collect();
    let edges = switches.update(logged_in, held.clone(), plates);

    // Edge-triggered cues: at most one press and one release cue per tick,
    // regardless of how many plates flipped — the messages carry no plate
    // identity, so collapsing simultaneous flips is lossless. Persistent state
    // lives in `PlateState` + snapshot; these are pure click/clunk SFX.
    if held.difference(&edges.prev_held).next().is_some() {
        broadcast_to_all(&players, ServerMessage::PressurePlate(SPressurePlate { pressed: true }));
    }
    if edges.prev_held.difference(&held).next().is_some() {
        broadcast_to_all(
            &players,
            ServerMessage::PressurePlate(SPressurePlate { pressed: false }),
        );
    }

    PlateFeed {
        players: &players,
        feed: &server_gameplay_config.feed,
        barrier_kinds: &barrier_kinds,
        bridge_kinds: &bridge_kinds,
        plates,
    }
    .emit(&holders, &edges, &plates_state, &switches.state());

    if run_firework_plates(plates, &held, alive, &mut switches) {
        broadcast_firework_show(&players);
        // `/firework` bypasses this on purpose: only the plates count.
        record_event(
            &mut players,
            &mut quest_board,
            &quest_catalog,
            &server_gameplay_config.feed,
            QuestEvent::FireworksStarted,
        );
    }
}

// The feed's view of the plates: who switched a purpose on, and which
// purposes went off.
struct PlateFeed<'a> {
    players: &'a PlayerMap,
    feed: &'a FeedConfig,
    barrier_kinds: &'a BarrierKindTable,
    bridge_kinds: &'a BridgeKindTable,
    plates: &'a [PressurePlateRuntime],
}

impl PlateFeed<'_> {
    fn kind_name(&self, purpose: HeldPurpose) -> String {
        match purpose {
            HeldPurpose::Barrier(kind) => self
                .barrier_kinds
                .id(kind)
                .expect("barrier kind missing from BarrierKindTable")
                .to_owned(),
            HeldPurpose::Bridge(kind) => self
                .bridge_kinds
                .id(kind)
                .expect("bridge kind missing from BridgeKindTable")
                .to_owned(),
        }
    }

    fn emit(&self, holders: &HashMap<usize, PlayerId>, edges: &PlateEdges, before: &PlateState, after: &PlateState) {
        let held: HashSet<usize> = holders.keys().copied().collect();
        let held_per_purpose = held_count_per_purpose(&held, self.plates);
        let prev_held_per_purpose = held_count_per_purpose(&edges.prev_held, self.plates);
        let next: Vec<_> = after.held().collect();

        for purpose in next.iter().copied().filter(|purpose| !before.contains(*purpose)) {
            let Some(presser) = presser_of_purpose(purpose, holders, &edges.prev_held, self.plates) else {
                continue;
            };
            let name = self.players.display_name(&presser);
            emit_feed(
                self.players,
                self.feed,
                FeedAudience::Everyone,
                FeedEvent::plate_held(purpose, name, self.kind_name(purpose)),
            );
        }
        for purpose in before.held().filter(|purpose| !next.contains(purpose)) {
            let held_now = held_per_purpose.get(&purpose).copied().unwrap_or(0);
            let held_before = prev_held_per_purpose.get(&purpose).copied().unwrap_or(0);
            if !edges.flipped.contains(&purpose) && held_now >= held_before {
                continue;
            }
            emit_feed(
                self.players,
                self.feed,
                FeedAudience::Everyone,
                FeedEvent::plate_released(purpose, self.kind_name(purpose)),
            );
        }
    }
}

// Whether this tick's firework plates start a show: the threshold's rising edge.
fn run_firework_plates(
    plates: &[PressurePlateRuntime],
    held: &HashSet<usize>,
    alive: usize,
    switches: &mut PressureSwitches,
) -> bool {
    let firework_plates = plates
        .iter()
        .filter(|plate| plate.purpose == PlatePurpose::Firework)
        .count();
    let held_fireworks = held
        .iter()
        .filter(|idx| plates[**idx].purpose == PlatePurpose::Firework)
        .count();
    switches.fireworks_started(firework_plates_ready(firework_plates, held_fireworks, alive))
}

pub(crate) fn pressure_switch_reset_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    positions: Query<&Position, With<PlayerMarker>>,
    quest_board: Res<QuestBoard>,
    mut switches: ResMut<PressureSwitches>,
) {
    let resets = players.take_resets();
    if resets.is_empty() {
        return;
    }
    let logged_in = players.values().filter(|info| info.connection.logged_in).count();
    let plates = &map_config.pressure_plates;
    let holders = plate_holders(
        &map_config,
        &carriers,
        &players,
        &positions,
        quest_board.locked_plate_purposes(),
    );
    let held: HashSet<_> = holders.keys().copied().collect();
    switches.reset(
        |trigger| {
            resets
                .iter()
                .any(|counts| trigger.applies(counts.logged_in, counts.alive))
        },
        logged_in,
        &held,
        plates,
    );
}

fn plate_holders(
    map_config: &MapConfig,
    carriers: &Carriers,
    players: &PlayerMap,
    positions: &Query<&Position, With<PlayerMarker>>,
    locked: &[PlatePurpose],
) -> HashMap<usize, PlayerId> {
    let mut holders = HashMap::new();
    for (idx, plate) in map_config.pressure_plates.iter().enumerate() {
        if !plate_active(plate, locked) {
            continue;
        }
        let geometry = &map_config.grid(plate.carrier).geometry;
        let pose = carriers.pose(plate.carrier);
        let holder = players.iter().find(|(_, info)| {
            info.connection.logged_in
                && info
                    .entity()
                    .and_then(|entity| positions.get(entity).ok())
                    .is_some_and(|pos| player_on_plate(plate, &pose.inverse_transform_position(pos), geometry))
        });
        if let Some((id, _)) = holder {
            holders.insert(idx, *id);
        }
    }
    holders
}

// Everyone alive is on a firework plate — or every plate is held when the
// players outnumber them.
pub(super) fn firework_plates_ready(plates: usize, held: usize, alive: usize) -> bool {
    plates > 0 && alive > 0 && held >= plates.min(alive)
}

// Plates that solve a still-locked quest don't exist for the players yet.
fn plate_active(plate: &PressurePlateRuntime, locked: &[PlatePurpose]) -> bool {
    !locked.contains(&plate.purpose)
}

fn held_count_per_purpose(held: &HashSet<usize>, plates: &[PressurePlateRuntime]) -> HashMap<HeldPurpose, usize> {
    let mut counts = HashMap::new();
    for purpose in held.iter().filter_map(|idx| plates[*idx].purpose.held()) {
        *counts.entry(purpose).or_insert(0) += 1;
    }
    counts
}

// Who gets credit for flipping a purpose on: the holder of one of its
// plates that was not held last tick, else any current holder. `None` when
// nobody is on a plate of that purpose.
pub(super) fn presser_of_purpose(
    purpose: HeldPurpose,
    holders: &HashMap<usize, PlayerId>,
    prev_held: &HashSet<usize>,
    plates: &[PressurePlateRuntime],
) -> Option<PlayerId> {
    let mut standing = None;
    for (idx, id) in holders {
        if plates[*idx].purpose.held() != Some(purpose) {
            continue;
        }
        if !prev_held.contains(idx) {
            return Some(*id);
        }
        standing = Some(*id);
    }
    standing
}
