use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use common::{
    config::{DeathTrigger, PressureSwitchConfig},
    protocol::{BarrierKindTable, BridgeKindTable, HeldPurpose, MapSettings, PlateState},
};

use crate::map::PressurePlateRuntime;

struct PressureSwitch {
    config: PressureSwitchConfig,
    active: bool,
    toggle: bool,
}

impl PressureSwitch {
    fn update_mode(&mut self, logged_in: usize, occupied: bool) -> bool {
        let toggle = self.config.activation.is_toggle(logged_in);
        let changed = toggle != self.toggle;
        if changed || !toggle {
            self.active = occupied;
        }
        self.toggle = toggle;
        changed
    }
}

// Every barrier and bridge kind's switch, plus the plates held last tick,
// which is what makes a press fresh; only this file moves any of it.
#[derive(Resource)]
pub(crate) struct PressureSwitches {
    kinds: HashMap<HeldPurpose, PressureSwitch>,
    prev_held: HashSet<usize>,
    fireworks_ready: bool,
}

// What one tick's occupancy changed, for the cues and feed lines.
pub(crate) struct PlateEdges {
    // Toggle switches flipped by a fresh press.
    pub flipped: Vec<HeldPurpose>,
    // The plates held on the previous tick.
    pub prev_held: HashSet<usize>,
}

impl FromWorld for PressureSwitches {
    fn from_world(world: &mut World) -> Self {
        Self::new(
            world.resource::<MapSettings>(),
            world.resource::<BarrierKindTable>(),
            world.resource::<BridgeKindTable>(),
        )
    }
}

impl PressureSwitches {
    // The tables own the id assignment; the catalogs carry each kind's switch policy.
    pub fn new(settings: &MapSettings, barriers: &BarrierKindTable, bridges: &BridgeKindTable) -> Self {
        let barriers = settings.barrier_kinds.iter().map(|def| {
            let kind = barriers
                .index_of(&def.id)
                .expect("barrier kind missing from BarrierKindTable");
            (HeldPurpose::Barrier(kind), def.pressure_switch)
        });
        let bridges = settings.bridge_kinds.iter().map(|def| {
            let kind = bridges
                .index_of(&def.id)
                .expect("bridge kind missing from BridgeKindTable");
            (HeldPurpose::Bridge(kind), def.pressure_switch)
        });
        Self {
            kinds: barriers
                .chain(bridges)
                .map(|(purpose, config)| {
                    (
                        purpose,
                        PressureSwitch {
                            config,
                            active: false,
                            toggle: config.activation.is_toggle(0),
                        },
                    )
                })
                .collect(),
            prev_held: HashSet::new(),
            fireworks_ready: false,
        }
    }

    // Advance every switch for this tick's occupancy; `held` becomes the
    // previous tick's set for the next call.
    pub fn update(&mut self, logged_in: usize, held: HashSet<usize>, plates: &[PressurePlateRuntime]) -> PlateEdges {
        let mut flipped = Vec::new();
        for (purpose, switch) in &mut self.kinds {
            let occupied = held.iter().any(|idx| plates[*idx].purpose.held() == Some(*purpose));
            let changed_mode = switch.update_mode(logged_in, occupied);
            if switch.toggle && !changed_mode {
                for _ in held
                    .difference(&self.prev_held)
                    .filter(|idx| plates[**idx].purpose.held() == Some(*purpose))
                {
                    switch.active = !switch.active;
                    flipped.push(*purpose);
                }
            }
        }
        let prev_held = std::mem::replace(&mut self.prev_held, held);
        PlateEdges { flipped, prev_held }
    }

    // A death or logout reset: every toggle switch whose policy `triggered`
    // goes off, and its plates held right now count as already pressed, so
    // the reset wins even for a surviving holder until a fresh press.
    pub fn reset(
        &mut self,
        triggered: impl Fn(DeathTrigger) -> bool,
        logged_in: usize,
        held: &HashSet<usize>,
        plates: &[PressurePlateRuntime],
    ) {
        let mut reset = HashSet::new();
        for (purpose, switch) in &mut self.kinds {
            let occupied = held.iter().any(|idx| plates[*idx].purpose.held() == Some(*purpose));
            switch.update_mode(logged_in, occupied);
            if switch.toggle && triggered(switch.config.reset_on_player_death) {
                switch.active = false;
                reset.insert(*purpose);
            }
        }
        let on_reset_purpose = |idx: &usize| {
            plates[*idx]
                .purpose
                .held()
                .is_some_and(|purpose| reset.contains(&purpose))
        };
        self.prev_held.retain(|idx| !on_reset_purpose(idx));
        self.prev_held.extend(held.iter().copied().filter(on_reset_purpose));
    }

    // Commits this tick's firework threshold; true on its rising edge.
    pub fn fireworks_started(&mut self, ready: bool) -> bool {
        let started = ready && !self.fireworks_ready;
        self.fireworks_ready = ready;
        started
    }

    pub fn state(&self) -> PlateState {
        PlateState::from_held(
            self.kinds
                .iter()
                .filter_map(|(purpose, switch)| switch.active.then_some(*purpose)),
        )
    }
}

pub(crate) fn plate_state_sync_system(switches: Res<PressureSwitches>, mut state: ResMut<PlateState>) {
    // Bridge collider sync reacts to changes, so equal states must not wake it.
    state.set_if_neq(switches.state());
}
