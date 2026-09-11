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
        PlateState, PlayerId, PlayerMarker, Position, SPressurePlate, ServerMessage, ServerTick, SwitchId, SwitchTable,
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

// Each switch uses its configured activation over the plates that name it:
// held for momentary, each fresh plate press for toggle, and toggle with
// exactly one logged-in player for auto. Entering auto toggle seeds from
// current occupancy. `held` decides what holding means: any occupied plate,
// or every living player on one (every plate when players outnumber them).
// The fireworks switch starts a show whenever it is active and the previous
// show plus the cooldown have passed.
pub(crate) fn pressure_plates_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut quest_board: ResMut<QuestBoard>,
    quest_catalog: Res<QuestCatalog>,
    switch_table: Res<SwitchTable>,
    tick: Res<ServerTick>,
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
        quest_board.locked_switches(),
    );
    let held: HashSet<usize> = holders.keys().copied().collect();
    let edges = switches.update(logged_in, alive, held.clone(), plates, tick.0);

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
        switch_table: &switch_table,
        plates,
    }
    .emit(&holders, &edges, &plates_state, &switches.state());

    if switches.fireworks_due(tick.0, quest_board.locked_switches()) {
        broadcast_firework_show(&players);
        // `/firework` bypasses this on purpose: only the switch counts.
        record_event(
            &mut players,
            &mut quest_board,
            &quest_catalog,
            &server_gameplay_config.feed,
            QuestEvent::FireworksStarted,
        );
    }
}

// The feed's view of the plates: who turned a switch on, and which switches
// went off.
struct PlateFeed<'a> {
    players: &'a PlayerMap,
    feed: &'a FeedConfig,
    switch_table: &'a SwitchTable,
    plates: &'a [PressurePlateRuntime],
}

impl PlateFeed<'_> {
    fn switch_name(&self, switch: SwitchId) -> String {
        self.switch_table
            .id(switch)
            .expect("switch missing from SwitchTable")
            .to_owned()
    }

    fn emit(&self, holders: &HashMap<usize, PlayerId>, edges: &PlateEdges, before: &PlateState, after: &PlateState) {
        let held: HashSet<usize> = holders.keys().copied().collect();
        let held_per_switch = held_count_per_switch(&held, self.plates);
        let prev_held_per_switch = held_count_per_switch(&edges.prev_held, self.plates);

        for switch in after
            .active_switches
            .iter()
            .copied()
            .filter(|switch| !before.is_active(*switch))
        {
            let Some(presser) = presser_of_switch(switch, holders, &edges.prev_held, self.plates) else {
                continue;
            };
            let name = self.players.display_name(&presser);
            emit_feed(
                self.players,
                self.feed,
                FeedAudience::Everyone,
                FeedEvent::SwitchOn {
                    name,
                    switch_name: self.switch_name(switch),
                },
            );
        }
        for switch in before
            .active_switches
            .iter()
            .copied()
            .filter(|switch| !after.is_active(*switch))
        {
            let held_now = held_per_switch.get(&switch).copied().unwrap_or(0);
            let held_before = prev_held_per_switch.get(&switch).copied().unwrap_or(0);
            if !edges.flipped.contains(&switch) && held_now >= held_before {
                continue;
            }
            emit_feed(
                self.players,
                self.feed,
                FeedAudience::Everyone,
                FeedEvent::SwitchOff {
                    switch_name: self.switch_name(switch),
                },
            );
        }
    }
}

pub(crate) fn pressure_switch_reset_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    positions: Query<&Position, With<PlayerMarker>>,
    quest_board: Res<QuestBoard>,
    tick: Res<ServerTick>,
    mut switches: ResMut<PressureSwitches>,
) {
    let resets = players.take_resets();
    if resets.is_empty() {
        return;
    }
    let logged_in = players.values().filter(|info| info.connection.logged_in).count();
    let alive = players
        .values()
        .filter(|info| info.connection.logged_in && !info.is_dead())
        .count();
    let plates = &map_config.pressure_plates;
    let holders = plate_holders(
        &map_config,
        &carriers,
        &players,
        &positions,
        quest_board.locked_switches(),
    );
    let held: HashSet<_> = holders.keys().copied().collect();
    switches.reset(
        |trigger| {
            resets
                .iter()
                .any(|counts| trigger.applies(counts.logged_in, counts.alive))
        },
        logged_in,
        alive,
        &held,
        plates,
        tick.0,
    );
}

fn plate_holders(
    map_config: &MapConfig,
    carriers: &Carriers,
    players: &PlayerMap,
    positions: &Query<&Position, With<PlayerMarker>>,
    locked: &[SwitchId],
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

// Plates of a switch that solves a still-locked quest don't exist for the
// players yet.
fn plate_active(plate: &PressurePlateRuntime, locked: &[SwitchId]) -> bool {
    !locked.contains(&plate.switch)
}

fn held_count_per_switch(held: &HashSet<usize>, plates: &[PressurePlateRuntime]) -> HashMap<SwitchId, usize> {
    let mut counts = HashMap::new();
    for switch in held.iter().map(|idx| plates[*idx].switch) {
        *counts.entry(switch).or_insert(0) += 1;
    }
    counts
}

// Who gets credit for turning a switch on: the holder of one of its plates
// that was not held last tick, else any current holder. `None` when nobody
// is on a plate of that switch.
pub(super) fn presser_of_switch(
    switch: SwitchId,
    holders: &HashMap<usize, PlayerId>,
    prev_held: &HashSet<usize>,
    plates: &[PressurePlateRuntime],
) -> Option<PlayerId> {
    let mut standing = None;
    for (idx, id) in holders {
        if plates[*idx].switch != switch {
            continue;
        }
        if !prev_held.contains(idx) {
            return Some(*id);
        }
        standing = Some(*id);
    }
    standing
}
