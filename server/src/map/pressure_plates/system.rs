use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::inputs::{PlateEdges, PressurePlateInputs};
use crate::map::switches::Switches;

use crate::{
    config::{FeedConfig, ServerGameplayConfig},
    map::{MapConfig, PressurePlateRuntime},
    network::{FeedAudience, FeedEvent, broadcast_firework_show, broadcast_to_all, emit_feed},
    players::PlayerMap,
    quests::{QuestBoard, QuestCatalog, QuestEvent, record_event},
};
use common::{
    map::{Carriers, MapGeometry},
    physics::CollisionWorld,
    protocol::{
        PlayerId, PlayerMarker, Position, SPressurePlate, ServerMessage, ServerTick, SwitchId, SwitchState, SwitchTable,
    },
};

pub(crate) fn pressure_plates_collision_system(
    quest_board: Res<QuestBoard>,
    mut collision_world: ResMut<CollisionWorld>,
) {
    if quest_board.is_changed() {
        collision_world.set_locked_pressure_plates(quest_board.locked_switches());
    }
}

// Is `pos`, in the plate's carrier frame, on this plate's square AND on the
// plate's level? Y matches within half a storey of the plate's floor, which
// keeps a player on the floor above from triggering a plate one level down.
#[must_use]
pub fn player_on_plate(plate: &PressurePlateRuntime, pos: &Position, geometry: &MapGeometry) -> bool {
    if (pos.y - geometry.level_y(plate.level)).abs() >= geometry.level_height() / 2.0 {
        return false;
    }
    let half = plate.side / 2.0;
    (pos.x - geometry.cell_center_x(plate.col)).abs() <= half
        && (pos.z - geometry.cell_center_z(plate.row)).abs() <= half
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
    switch_state: Res<SwitchState>,
    mut switches: ResMut<Switches>,
    mut inputs: ResMut<PressurePlateInputs>,
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
    let edges = inputs.update(&mut switches, logged_in, alive, held, plates, tick.0);
    let changes = PlateChanges::new(&holders, &edges, &switch_state, &switches.state(), plates);

    // At most one click each way per tick: the cue names no switch, so
    // collapsing simultaneous changes loses nothing.
    for (switched_on, changed) in [(true, !changes.on.is_empty()), (false, !changes.off.is_empty())] {
        if changed {
            broadcast_to_all(&players, ServerMessage::PressurePlate(SPressurePlate { switched_on }));
        }
    }

    PlateFeed {
        players: &players,
        feed: &server_gameplay_config.feed,
        switch_table: &switch_table,
    }
    .emit(&changes);

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

// What the plates did this tick: each switch they turned on, with whoever
// gets the credit, and each they turned off. A switch that a reset or a
// change of activation mode turned off is in neither.
struct PlateChanges {
    on: Vec<(SwitchId, PlayerId)>,
    off: Vec<SwitchId>,
}

impl PlateChanges {
    fn new(
        holders: &HashMap<usize, PlayerId>,
        edges: &PlateEdges,
        before: &SwitchState,
        after: &SwitchState,
        plates: &[PressurePlateRuntime],
    ) -> Self {
        let held: HashSet<usize> = holders.keys().copied().collect();
        let held_per_switch = held_count_per_switch(&held, plates);
        let prev_held_per_switch = held_count_per_switch(&edges.prev_held, plates);
        let on = after
            .active_switches
            .iter()
            .copied()
            .filter(|switch| !before.is_active(*switch))
            .filter_map(|switch| {
                presser_of_switch(switch, holders, &edges.prev_held, plates).map(|presser| (switch, presser))
            })
            .collect();
        let off = before
            .active_switches
            .iter()
            .copied()
            .filter(|switch| !after.is_active(*switch))
            .filter(|switch| {
                let held_now = held_per_switch.get(switch).copied().unwrap_or(0);
                let held_before = prev_held_per_switch.get(switch).copied().unwrap_or(0);
                edges.flipped.contains(switch) || held_now < held_before
            })
            .collect();
        Self { on, off }
    }
}

// The feed's view of the plates: who turned a switch on, and which switches
// went off.
struct PlateFeed<'a> {
    players: &'a PlayerMap,
    feed: &'a FeedConfig,
    switch_table: &'a SwitchTable,
}

impl PlateFeed<'_> {
    fn switch_name(&self, switch: SwitchId) -> String {
        self.switch_table
            .id(switch)
            .expect("switch missing from SwitchTable")
            .to_owned()
    }

    fn emit(&self, changes: &PlateChanges) {
        for (switch, presser) in &changes.on {
            emit_feed(
                self.players,
                self.feed,
                FeedAudience::Everyone,
                FeedEvent::SwitchOn {
                    name: self.players.display_name(presser),
                    switch_name: self.switch_name(*switch),
                },
            );
        }
        for switch in &changes.off {
            emit_feed(
                self.players,
                self.feed,
                FeedAudience::Everyone,
                FeedEvent::SwitchOff {
                    switch_name: self.switch_name(*switch),
                },
            );
        }
    }
}

pub(crate) fn switch_reset_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    positions: Query<&Position, With<PlayerMarker>>,
    quest_board: Res<QuestBoard>,
    tick: Res<ServerTick>,
    mut switches: ResMut<Switches>,
    mut inputs: ResMut<PressurePlateInputs>,
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
    inputs.reset(
        &mut switches,
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
