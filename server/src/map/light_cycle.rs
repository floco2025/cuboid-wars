use bevy::prelude::*;

use crate::config::{LightingCycleConfig, LightingMode};
use common::protocol::LightingBlend;

// The preset vocabulary the server can name; the looks themselves are
// client-side config.
pub const LIGHT_PRESETS: [&str; 3] = ["bright", "dim", "dark"];

#[must_use]
pub fn light_preset_from_str(name: &str) -> Option<&'static str> {
    LIGHT_PRESETS.iter().find(|preset| **preset == name).copied()
}

// Where the lighting comes from: the cycle clock, or a latched blend (a
// concrete map mode, or an admin override) that pauses the cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LightMode {
    // Seconds into the cycle timeline; wraps at `cycle_len`.
    Auto { cycle_pos: f32 },
    Manual,
}

// Server-driven lighting, decoupled from weather. `current` is the single
// authoritative blend between two named presets that ships in every
// snapshot; clients resolve the names against their configured looks.
#[derive(Resource)]
pub struct LightState {
    schedule: LightingCycleConfig,
    mode: LightMode,
    current: Blend,
}

// Internal blend over the static preset vocabulary.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Blend {
    from: &'static str,
    to: &'static str,
    blend: f32,
}

impl Blend {
    const fn preset(name: &'static str) -> Self {
        Self {
            from: name,
            to: name,
            blend: 0.0,
        }
    }

    // The preset this blend is closest to.
    fn dominant(self) -> &'static str {
        if self.blend < 0.5 { self.from } else { self.to }
    }

    fn describe(self) -> String {
        if self.from == self.to {
            self.from.to_owned()
        } else {
            format!("{}\u{2192}{} {:.2}", self.from, self.to, self.blend)
        }
    }
}

impl LightState {
    #[must_use]
    pub fn new(schedule: LightingCycleConfig, mode: LightingMode) -> Self {
        let (mode, current) = match mode.preset() {
            Some(name) => (LightMode::Manual, Blend::preset(name)),
            None => (LightMode::Auto { cycle_pos: 0.0 }, blend_at(&schedule, 0.0)),
        };
        Self {
            schedule,
            mode,
            current,
        }
    }

    // The wire form of the current blend.
    #[must_use]
    pub fn blend(&self) -> LightingBlend {
        LightingBlend {
            from: self.current.from.to_owned(),
            to: self.current.to.to_owned(),
            blend: self.current.blend,
        }
    }

    #[must_use]
    fn is_auto(&self) -> bool {
        matches!(self.mode, LightMode::Auto { .. })
    }

    // Admin override: hold a named preset, absolute and cycle-independent;
    // any running cycle pauses.
    pub fn hold_preset(&mut self, name: &'static str) {
        self.mode = LightMode::Manual;
        self.current = Blend::preset(name);
    }

    // Admin override: hold an arbitrary blend between two presets.
    pub fn hold_blend(&mut self, from: &'static str, to: &'static str, blend: f32) {
        self.mode = LightMode::Manual;
        self.current = Blend {
            from,
            to,
            blend: blend.clamp(0.0, 1.0),
        };
    }

    // Admin override: hold a position within the cycle's own range — 1.0 is
    // its highest stop, 0.0 its lowest, along the cycle's descending chain
    // of fades (each fade takes an equal share of the range).
    pub fn hold_cycle_fraction(&mut self, fraction: f32) {
        let fraction = fraction.clamp(0.0, 1.0);
        let stops = stops(&self.schedule);
        let fades = (stops.len() - 1) as f32;
        let along = (1.0 - fraction) * fades;
        let index = (along.floor() as usize).min(stops.len() - 2);
        self.mode = LightMode::Manual;
        self.current = Blend {
            from: stops[index].0,
            to: stops[index + 1].0,
            blend: along - index as f32,
        };
    }

    // Admin override: hand control back to the cycle. Re-enters at the
    // point matching the held blend when it lies on the cycle's path;
    // otherwise at the hold of the nearest preset that is a stop, falling
    // back to the top stop — deterministic, and the cycle self-corrects
    // within one revolution.
    pub fn resume_auto(&mut self) -> Result<(), &'static str> {
        match self.mode {
            LightMode::Manual => {
                self.mode = LightMode::Auto {
                    cycle_pos: pos_for_blend(&self.schedule, self.current),
                };
                Ok(())
            }
            LightMode::Auto { .. } => Err("light cycle already running"),
        }
    }

    #[must_use]
    pub fn status(&self) -> String {
        let source = match self.mode {
            LightMode::Auto { .. } => "auto",
            LightMode::Manual => "held",
        };
        format!("light: {} ({source})", self.current.describe())
    }
}

// The stops the cycle visits, brightest first: (preset, hold_secs) for each
// present hold.
fn stops(schedule: &LightingCycleConfig) -> Vec<(&'static str, f32)> {
    [
        ("bright", schedule.bright_secs),
        ("dim", schedule.dim_secs),
        ("dark", schedule.dark_secs),
    ]
    .into_iter()
    .filter_map(|(name, hold)| hold.map(|hold| (name, hold)))
    .collect()
}

// Fade length between two adjacent stops; config validation guarantees the
// matching key is set.
fn fade_secs(schedule: &LightingCycleConfig, high: &str, low: &str) -> f32 {
    let fade = match (high, low) {
        ("bright", "dim") => schedule.bright_dim_secs,
        ("dim", "dark") => schedule.dim_dark_secs,
        _ => schedule.bright_dark_secs,
    };
    fade.expect("fade missing for adjacent lighting_cycle stops")
}

// The cycle timeline as (duration, from, to) segments; holds are segments
// with equal endpoints. Down through the stops, then back up, holding
// intermediate stops on both legs; the wrap point is the top hold.
fn segments(schedule: &LightingCycleConfig) -> Vec<(f32, &'static str, &'static str)> {
    let stops = stops(schedule);
    let mut segments = vec![(stops[0].1, stops[0].0, stops[0].0)];
    for pair in stops.windows(2) {
        let ((high, _), (low, low_hold)) = (pair[0], pair[1]);
        segments.push((fade_secs(schedule, high, low), high, low));
        segments.push((low_hold, low, low));
    }
    for (i, pair) in stops.windows(2).enumerate().rev() {
        let ((high, high_hold), (low, _)) = (pair[0], pair[1]);
        segments.push((fade_secs(schedule, high, low), low, high));
        if i != 0 {
            segments.push((high_hold, high, high));
        }
    }
    segments
}

fn cycle_len(schedule: &LightingCycleConfig) -> f32 {
    segments(schedule).iter().map(|(duration, ..)| duration).sum()
}

fn blend_at(schedule: &LightingCycleConfig, pos: f32) -> Blend {
    let mut start = 0.0;
    for (duration, from, to) in segments(schedule) {
        if pos < start + duration {
            return Blend {
                from,
                to,
                blend: if from == to { 0.0 } else { (pos - start) / duration },
            };
        }
        start += duration;
    }
    Blend::preset(stops(schedule)[0].0)
}

// Inverse of `blend_at` for resuming (see `resume_auto` for the policy on
// blends that don't lie on the path).
fn pos_for_blend(schedule: &LightingCycleConfig, blend: Blend) -> f32 {
    let mut start = 0.0;
    for (duration, from, to) in segments(schedule) {
        if from == to && from == blend.from && blend.from == blend.to {
            return start;
        }
        if from != to && from == blend.from && to == blend.to {
            return start + blend.blend * duration;
        }
        // The same physical blend expressed in the opposite direction.
        if from != to && from == blend.to && to == blend.from {
            return start + (1.0 - blend.blend) * duration;
        }
        start += duration;
    }
    // Off the path: enter at the dominant preset's hold when it is a stop,
    // else at the top of the cycle.
    let dominant = blend.dominant();
    let mut start = 0.0;
    for (duration, from, to) in segments(schedule) {
        if from == to && from == dominant {
            return start;
        }
        start += duration;
    }
    0.0
}

pub fn light_cycle_system(time: Res<Time>, mut light: ResMut<LightState>) {
    tick_light(&mut light, time.delta_secs());
}

pub fn light_cycle_is_running(light: Res<LightState>) -> bool {
    light.is_auto()
}

fn tick_light(state: &mut LightState, delta: f32) {
    if let LightMode::Auto { cycle_pos } = &mut state.mode {
        *cycle_pos = (*cycle_pos + delta) % cycle_len(&state.schedule);
        state.current = blend_at(&state.schedule, *cycle_pos);
    }
}

#[cfg(test)]
#[path = "tests/light_cycle.rs"]
mod tests;
