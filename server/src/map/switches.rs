use std::collections::HashSet;

use bevy::prelude::*;
use common::{
    config::{DeathTrigger, SwitchConfig, SwitchHold},
    constants::FIREWORK_SHOW_SECS,
    map::CarrierRun,
    protocol::{
        Carrier, CarrierId, FieldId, MapLayout, MapSettings, SwitchId, SwitchState, SwitchTable, sequence_is_newer,
    },
};

use crate::{
    config::ServerGameplayConfig,
    map::{FireworksConfig, MapFireworks},
    schedule::ticks_from_secs,
};

// One switch: its policy, whether it is on, and everything it drives.
struct Switch {
    config: SwitchConfig,
    active: bool,
    toggle: bool,
    // Whether its inputs met the hold rule last tick; an `everyone` toggle
    // flips on that rule's rising edge.
    occupied: bool,
    // Each field it drives, with the field's initial state.
    fields: Vec<(FieldId, bool)>,
    carriers: Vec<(CarrierId, Carrier, CarrierRun)>,
}

impl Switch {
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
        for (_, carrier, run) in &mut self.carriers {
            *run = run.set_active(active != carrier.initially_on, tick, carrier);
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

#[derive(Resource)]
pub(crate) struct Switches {
    switches: Vec<Switch>,
    // Fields no switch drives that start off, and so stay off.
    unswitched_open_fields: Vec<FieldId>,
    fireworks: Option<FireworkTarget>,
}

#[derive(Default, Clone, Copy)]
pub(crate) struct SwitchInput {
    pub occupied: bool,
    pub presses: usize,
}

impl FromWorld for Switches {
    fn from_world(world: &mut World) -> Self {
        Self::new(
            world.resource::<MapSettings>(),
            world.resource::<SwitchTable>(),
            world.resource::<MapLayout>(),
            world.resource::<MapFireworks>().0.as_ref(),
            world.resource::<ServerGameplayConfig>().network.server_hz,
        )
    }
}

impl Switches {
    pub fn new(
        settings: &MapSettings,
        switch_table: &SwitchTable,
        layout: &MapLayout,
        fireworks: Option<&FireworksConfig>,
        server_hz: u32,
    ) -> Self {
        let mut switches: Vec<Switch> = settings
            .switches
            .iter()
            .map(|def| Switch {
                config: def.policy,
                active: false,
                toggle: def.policy.activation.is_toggle(0),
                occupied: false,
                fields: Vec::new(),
                carriers: Vec::new(),
            })
            .collect();
        let mut unswitched_open_fields = Vec::new();
        for (index, field) in settings.fields.iter().enumerate() {
            let id = FieldId(index as u16);
            let switch = field.switch.as_deref().map(|switch| {
                switch_table
                    .index_of(switch)
                    .expect("field switch missing from SwitchTable")
            });
            match switch {
                Some(switch) => switches[usize::from(switch.0)].fields.push((id, field.initially_on)),
                None if !field.initially_on => unswitched_open_fields.push(id),
                None => {}
            }
        }
        for (index, carrier) in layout.carriers.iter().enumerate() {
            if let Some(switch) = carrier.switch {
                switches[usize::from(switch.0)].carriers.push((
                    CarrierId::from_carried_index(index),
                    *carrier,
                    CarrierRun::initial(carrier),
                ));
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
            unswitched_open_fields,
            fireworks,
        }
    }

    pub fn hold_rules(&self) -> impl Iterator<Item = SwitchHold> + '_ {
        self.switches.iter().map(|switch| switch.config.held)
    }

    pub fn update(&mut self, logged_in: usize, inputs: &[SwitchInput], tick: u32) -> Vec<SwitchId> {
        assert_eq!(inputs.len(), self.switches.len());
        let mut flipped = Vec::new();
        for (index, (switch, input)) in self.switches.iter_mut().zip(inputs).enumerate() {
            let changed_mode = switch.update_mode(logged_in, input.occupied, tick);
            if switch.toggle && !changed_mode {
                let presses = match switch.config.held {
                    SwitchHold::Any => input.presses,
                    SwitchHold::Everyone => usize::from(input.occupied && !switch.occupied),
                };
                for _ in 0..presses {
                    switch.set_active(!switch.active, tick);
                    flipped.push(SwitchId(index as u16));
                }
            }
            switch.occupied = input.occupied;
        }
        flipped
    }

    // Preserve unreset inputs' occupancy so the next update can still see
    // an Everyone threshold crossed by a death or logout.
    pub fn reset(
        &mut self,
        triggered: impl Fn(DeathTrigger) -> bool,
        logged_in: usize,
        inputs: &[SwitchInput],
        tick: u32,
    ) -> HashSet<SwitchId> {
        assert_eq!(inputs.len(), self.switches.len());
        let mut reset = HashSet::new();
        for (index, (switch, input)) in self.switches.iter_mut().zip(inputs).enumerate() {
            let changed_mode = switch.update_mode(logged_in, input.occupied, tick);
            let reset_now = switch.toggle && triggered(switch.config.reset_on_player_death);
            if reset_now {
                switch.set_active(false, tick);
                reset.insert(SwitchId(index as u16));
            }
            if reset_now || changed_mode {
                switch.occupied = input.occupied;
            }
        }
        reset
    }

    // Whether a show starts this tick: the fireworks switch is active and
    // the previous show plus the cooldown have passed. The stamp is never
    // cleared, so a re-press during a cooldown waits it out.
    pub fn fireworks_due(&mut self, tick: u32, locked: &[SwitchId]) -> bool {
        let Some(fireworks) = &mut self.fireworks else {
            return false;
        };
        if locked.contains(&fireworks.switch) || !self.switches[usize::from(fireworks.switch.0)].active {
            return false;
        }
        if fireworks.next_show_at.is_some_and(|at| sequence_is_newer(at, tick)) {
            return false;
        }
        fireworks.next_show_at = Some(tick.wrapping_add(fireworks.interval_ticks));
        true
    }

    pub fn state(&self) -> SwitchState {
        let mut state = SwitchState {
            open_fields: self.unswitched_open_fields.clone(),
            ..Default::default()
        };
        for (index, switch) in self.switches.iter().enumerate() {
            if switch.active {
                state.active_switches.push(SwitchId(index as u16));
            }
            // An active switch flips each field from its initial state, so a
            // field is off while the two agree.
            state.open_fields.extend(
                switch
                    .fields
                    .iter()
                    .filter(|(_, initially_on)| switch.active == *initially_on)
                    .map(|(id, _)| *id),
            );
            state
                .carrier_runs
                .extend(switch.carriers.iter().map(|(id, _, run)| (*id, *run)));
        }
        state.sort();
        state
    }
}

pub(crate) fn switch_state_sync_system(switches: Res<Switches>, mut state: ResMut<SwitchState>) {
    // Navigation's bridge sync reacts to changes, so equal states must not wake it.
    state.set_if_neq(switches.state());
}
