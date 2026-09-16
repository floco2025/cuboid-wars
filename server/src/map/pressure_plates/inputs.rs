use std::collections::HashSet;

use bevy::prelude::Resource;
use common::{config::DeathTrigger, protocol::SwitchId};

use crate::map::{
    PressurePlateRuntime,
    switches::{SwitchInput, Switches},
};

#[derive(Resource, Default)]
pub(crate) struct PressurePlateInputs {
    prev_held: HashSet<usize>,
}

pub(super) struct PlateEdges {
    pub flipped: Vec<SwitchId>,
    pub prev_held: HashSet<usize>,
}

impl PressurePlateInputs {
    fn sample(
        &self,
        switches: &Switches,
        alive: usize,
        held: &HashSet<usize>,
        plates: &[PressurePlateRuntime],
    ) -> Vec<SwitchInput> {
        switches
            .hold_rules()
            .enumerate()
            .map(|(index, rule)| {
                let id = SwitchId(index as u16);
                let count = plates.iter().filter(|plate| plate.switch == id).count();
                let held_count = held.iter().filter(|index| plates[**index].switch == id).count();
                let presses = held
                    .difference(&self.prev_held)
                    .filter(|index| plates[**index].switch == id)
                    .count();
                SwitchInput {
                    occupied: rule.is_held(count, held_count, alive),
                    presses,
                }
            })
            .collect()
    }

    pub(super) fn update(
        &mut self,
        switches: &mut Switches,
        logged_in: usize,
        alive: usize,
        held: HashSet<usize>,
        plates: &[PressurePlateRuntime],
        tick: u32,
    ) -> PlateEdges {
        let inputs = self.sample(switches, alive, &held, plates);
        let flipped = switches.update(logged_in, &inputs, tick);
        let prev_held = std::mem::replace(&mut self.prev_held, held);
        PlateEdges { flipped, prev_held }
    }

    pub(super) fn reset(
        &mut self,
        switches: &mut Switches,
        triggered: impl Fn(DeathTrigger) -> bool,
        logged_in: usize,
        alive: usize,
        held: &HashSet<usize>,
        plates: &[PressurePlateRuntime],
        tick: u32,
    ) {
        let inputs = self.sample(switches, alive, held, plates);
        let reset = switches.reset(triggered, logged_in, &inputs, tick);
        // Held plates must release before a reset switch accepts another press.
        let on_reset_switch = |index: &usize| reset.contains(&plates[*index].switch);
        self.prev_held.retain(|index| !on_reset_switch(index));
        self.prev_held.extend(held.iter().copied().filter(on_reset_switch));
    }
}
