use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use common::{
    config::PressureSwitchConfig,
    protocol::{BarrierKindId, BridgeKindId, HeldPurpose, KindDef, MapSettings, PlateState},
};

use super::PressurePlateRuntime;

pub(super) struct PressureSwitch {
    pub config: PressureSwitchConfig,
    pub active: bool,
    pub toggle: bool,
}

impl PressureSwitch {
    pub fn update_mode(&mut self, logged_in: usize, occupied: bool) -> bool {
        let toggle = self.config.activation.is_toggle(logged_in);
        let changed = toggle != self.toggle;
        if changed || !toggle {
            self.active = occupied;
        }
        self.toggle = toggle;
        changed
    }
}

#[derive(Resource)]
pub(super) struct PressureSwitches {
    pub kinds: HashMap<HeldPurpose, PressureSwitch>,
    pub prev_held: HashSet<usize>,
    pub fireworks_ready: bool,
}

impl FromWorld for PressureSwitches {
    fn from_world(world: &mut World) -> Self {
        let settings = world.resource::<MapSettings>();
        Self::new(&settings.barrier_kinds, &settings.bridge_kinds)
    }
}

impl PressureSwitches {
    pub fn new(barriers: &[KindDef], bridges: &[KindDef]) -> Self {
        let barriers = barriers
            .iter()
            .enumerate()
            .map(|(idx, def)| (HeldPurpose::Barrier(BarrierKindId(idx as u16)), def.pressure_switch));
        let bridges = bridges
            .iter()
            .enumerate()
            .map(|(idx, def)| (HeldPurpose::Bridge(BridgeKindId(idx as u16)), def.pressure_switch));
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

    pub fn update(
        &mut self,
        logged_in: usize,
        held: &HashSet<usize>,
        plates: &[PressurePlateRuntime],
    ) -> Vec<HeldPurpose> {
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
        flipped
    }

    pub fn state(&self) -> PlateState {
        PlateState::from_held(
            self.kinds
                .iter()
                .filter_map(|(purpose, switch)| switch.active.then_some(*purpose)),
        )
    }
}

pub(super) fn plate_state_sync_system(switches: Res<PressureSwitches>, mut state: ResMut<PlateState>) {
    // Bridge collider sync reacts to changes, so equal states must not wake it.
    state.set_if_neq(switches.state());
}
