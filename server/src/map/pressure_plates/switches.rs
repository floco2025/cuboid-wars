use std::collections::HashSet;

use bevy::prelude::*;
use common::{
    config::{DeathTrigger, PressureSwitchConfig, SwitchHold},
    constants::FIREWORK_SHOW_SECS,
    map::CarrierRun,
    protocol::{
        BarrierKindId, BarrierKindTable, BridgeKindId, BridgeKindTable, CarrierId, MapLayout, MapSettings, PlateState,
        SwitchId, SwitchTable, sequence_is_newer,
    },
};

use crate::{
    config::{FireworksConfig, ServerGameplayConfig},
    map::{MapFireworks, PressurePlateRuntime},
    schedule::ticks_from_secs,
};

// One switch: its policy, whether it is on, and everything it drives.
struct PressureSwitch {
    config: PressureSwitchConfig,
    active: bool,
    toggle: bool,
    // Whether its plates met the hold rule last tick; an `everyone` toggle
    // flips on that rule's rising edge.
    occupied: bool,
    barriers: Vec<BarrierKindId>,
    bridges: Vec<BridgeKindId>,
    carriers: Vec<(CarrierId, CarrierRun)>,
}

impl PressureSwitch {
    fn update_mode(&mut self, logged_in: usize, occupied: bool, tick: u32) -> bool {
        let toggle = self.config.activation.is_toggle(logged_in);
        let changed = toggle != self.toggle;
        if changed || !toggle {
            self.set_active(occupied, tick);
        }
        self.toggle = toggle;
        changed
    }

    // The one place a switch turns on or off, so its carriers' runs always
    // flip with it and never re-stamp without a change.
    fn set_active(&mut self, active: bool, tick: u32) {
        if active == self.active {
            return;
        }
        self.active = active;
        for (_, run) in &mut self.carriers {
            *run = run.set_running(active, tick);
        }
    }
}

// The fireworks switch's cadence: while it is active a show starts whenever
// the previous show and the cooldown have passed.
struct FireworkTarget {
    switch: SwitchId,
    interval_ticks: u32,
    next_show_at: Option<u32>,
}

// Every switch of the map, plus the plates held last tick, which is what
// makes a press fresh; only this file moves any of it.
#[derive(Resource)]
pub(crate) struct PressureSwitches {
    switches: Vec<PressureSwitch>,
    fireworks: Option<FireworkTarget>,
    prev_held: HashSet<usize>,
}

// What one tick's occupancy changed, for the cues and feed lines.
pub(crate) struct PlateEdges {
    // Toggle switches flipped by a fresh press.
    pub flipped: Vec<SwitchId>,
    // The plates held on the previous tick.
    pub prev_held: HashSet<usize>,
}

impl FromWorld for PressureSwitches {
    fn from_world(world: &mut World) -> Self {
        Self::new(
            world.resource::<MapSettings>(),
            world.resource::<BarrierKindTable>(),
            world.resource::<BridgeKindTable>(),
            world.resource::<SwitchTable>(),
            world.resource::<MapLayout>(),
            world.resource::<MapFireworks>().0.as_ref(),
            world.resource::<ServerGameplayConfig>().network.server_hz,
        )
    }
}

impl PressureSwitches {
    // The switch table owns the id assignment; the settings carry each
    // switch's policy and each kind's switch, the layout each carrier's.
    pub fn new(
        settings: &MapSettings,
        barriers: &BarrierKindTable,
        bridges: &BridgeKindTable,
        switch_table: &SwitchTable,
        layout: &MapLayout,
        fireworks: Option<&FireworksConfig>,
        server_hz: u32,
    ) -> Self {
        let mut switches: Vec<PressureSwitch> = settings
            .switches
            .iter()
            .map(|def| PressureSwitch {
                config: def.policy,
                active: false,
                toggle: def.policy.activation.is_toggle(0),
                occupied: false,
                barriers: Vec::new(),
                bridges: Vec::new(),
                carriers: Vec::new(),
            })
            .collect();
        for (def, switch) in settings
            .barrier_kinds
            .iter()
            .zip(settings.barrier_switches(switch_table))
        {
            if let Some(switch) = switch {
                let kind = barriers
                    .index_of(&def.id)
                    .expect("barrier kind missing from BarrierKindTable");
                switches[usize::from(switch.0)].barriers.push(kind);
            }
        }
        for (def, switch) in settings.bridge_kinds.iter().zip(settings.bridge_switches(switch_table)) {
            if let Some(switch) = switch {
                let kind = bridges
                    .index_of(&def.id)
                    .expect("bridge kind missing from BridgeKindTable");
                switches[usize::from(switch.0)].bridges.push(kind);
            }
        }
        for (index, carrier) in layout.carriers.iter().enumerate() {
            if let Some(switch) = carrier.switch {
                switches[usize::from(switch.0)]
                    .carriers
                    .push((CarrierId::from_carried_index(index), CarrierRun::STOPPED));
            }
        }
        let fireworks = fireworks.map(|fireworks| FireworkTarget {
            switch: switch_table
                .index_of(&fireworks.switch)
                .expect("fireworks switch missing from SwitchTable"),
            interval_ticks: ticks_from_secs(FIREWORK_SHOW_SECS + fireworks.cooldown_secs, server_hz),
            next_show_at: None,
        });
        Self {
            switches,
            fireworks,
            prev_held: HashSet::new(),
        }
    }

    // Advance every switch for this tick's occupancy; `held` becomes the
    // previous tick's set for the next call.
    pub fn update(
        &mut self,
        logged_in: usize,
        alive: usize,
        held: HashSet<usize>,
        plates: &[PressurePlateRuntime],
        tick: u32,
    ) -> PlateEdges {
        let mut flipped = Vec::new();
        let prev_held = &self.prev_held;
        for (index, switch) in self.switches.iter_mut().enumerate() {
            let id = SwitchId(index as u16);
            let occupied = occupied(switch.config.held, id, &held, plates, alive);
            let changed_mode = switch.update_mode(logged_in, occupied, tick);
            if switch.toggle && !changed_mode {
                let presses = match switch.config.held {
                    SwitchHold::Any => held
                        .difference(prev_held)
                        .filter(|idx| plates[**idx].switch == id)
                        .count(),
                    SwitchHold::Everyone => usize::from(occupied && !switch.occupied),
                };
                for _ in 0..presses {
                    switch.set_active(!switch.active, tick);
                    flipped.push(id);
                }
            }
            switch.occupied = occupied;
        }
        let prev_held = std::mem::replace(&mut self.prev_held, held);
        PlateEdges { flipped, prev_held }
    }

    // A death or logout reset: every toggle switch whose policy `triggered`
    // goes off, and its plates held right now count as already pressed, so
    // the reset wins even for a surviving holder until a fresh press. Every
    // other switch keeps its last observed occupancy: the death may have
    // lowered an `everyone` threshold to what the survivors hold, and the
    // next update must still see that rising edge.
    pub fn reset(
        &mut self,
        triggered: impl Fn(DeathTrigger) -> bool,
        logged_in: usize,
        alive: usize,
        held: &HashSet<usize>,
        plates: &[PressurePlateRuntime],
        tick: u32,
    ) {
        let mut reset = HashSet::new();
        for (index, switch) in self.switches.iter_mut().enumerate() {
            let id = SwitchId(index as u16);
            let occupied = occupied(switch.config.held, id, held, plates, alive);
            let changed_mode = switch.update_mode(logged_in, occupied, tick);
            let reset_now = switch.toggle && triggered(switch.config.reset_on_player_death);
            if reset_now {
                switch.set_active(false, tick);
                reset.insert(id);
            }
            if reset_now || changed_mode {
                switch.occupied = occupied;
            }
        }
        let on_reset_switch = |idx: &usize| reset.contains(&plates[*idx].switch);
        self.prev_held.retain(|idx| !on_reset_switch(idx));
        self.prev_held.extend(held.iter().copied().filter(on_reset_switch));
    }

    // Whether a show starts this tick: the fireworks switch is active and
    // the previous show plus the cooldown have passed. The stamp is never
    // cleared, so a re-press during a cooldown waits it out.
    pub fn fireworks_due(&mut self, tick: u32) -> bool {
        let Some(fireworks) = &mut self.fireworks else {
            return false;
        };
        if !self.switches[usize::from(fireworks.switch.0)].active {
            return false;
        }
        if fireworks.next_show_at.is_some_and(|at| sequence_is_newer(at, tick)) {
            return false;
        }
        fireworks.next_show_at = Some(tick.wrapping_add(fireworks.interval_ticks));
        true
    }

    pub fn state(&self) -> PlateState {
        let mut state = PlateState::default();
        for (index, switch) in self.switches.iter().enumerate() {
            if switch.active {
                state.active_switches.push(SwitchId(index as u16));
                state.open_barrier_kinds.extend(&switch.barriers);
                state.powered_bridge_kinds.extend(&switch.bridges);
            }
            state.carrier_runs.extend(switch.carriers.iter().copied());
        }
        state.sort();
        state
    }
}

// Whether a switch's plates hold it: `held` and `plates` index the same list.
fn occupied(
    hold: SwitchHold,
    switch: SwitchId,
    held: &HashSet<usize>,
    plates: &[PressurePlateRuntime],
    alive: usize,
) -> bool {
    let plate_count = plates.iter().filter(|plate| plate.switch == switch).count();
    let held_count = held.iter().filter(|idx| plates[**idx].switch == switch).count();
    hold.is_held(plate_count, held_count, alive)
}

pub(crate) fn plate_state_sync_system(switches: Res<PressureSwitches>, mut state: ResMut<PlateState>) {
    // Bridge collider sync reacts to changes, so equal states must not wake it.
    state.set_if_neq(switches.state());
}
