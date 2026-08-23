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

fn tick_light(state: &mut LightState, delta: f32) {
    if let LightMode::Auto { cycle_pos } = &mut state.mode {
        *cycle_pos = (*cycle_pos + delta) % cycle_len(&state.schedule);
        state.current = blend_at(&state.schedule, *cycle_pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Timeline: bright [0,20), fade [20,24), dim [24,30), fade [30,32),
    // dark [32,42), fade [42,44), dim [44,50), fade [50,54), wrap at 54.
    fn cycle() -> LightingCycleConfig {
        LightingCycleConfig {
            bright_secs: Some(20.0),
            dim_secs: Some(6.0),
            dark_secs: Some(10.0),
            bright_dim_secs: Some(4.0),
            dim_dark_secs: Some(2.0),
            bright_dark_secs: None,
        }
    }

    fn bright_dark() -> LightingCycleConfig {
        LightingCycleConfig {
            dim_secs: None,
            bright_dim_secs: None,
            dim_dark_secs: None,
            bright_dark_secs: Some(8.0),
            ..cycle()
        }
    }

    fn blend(from: &'static str, to: &'static str, t: f32) -> Blend {
        Blend { from, to, blend: t }
    }

    fn assert_blend_eq(actual: Blend, expected: Blend) {
        assert_eq!(actual.from, expected.from);
        assert_eq!(actual.to, expected.to);
        assert!(
            (actual.blend - expected.blend).abs() < 1e-3,
            "blend {} != {}",
            actual.blend,
            expected.blend
        );
    }

    #[test]
    fn concrete_modes_hold_their_preset_forever() {
        for (mode, name) in [
            (LightingMode::Bright, "bright"),
            (LightingMode::Dim, "dim"),
            (LightingMode::Dark, "dark"),
        ] {
            let mut state = LightState::new(cycle(), mode);
            for _ in 0..100 {
                tick_light(&mut state, 30.0);
            }
            assert_blend_eq(state.current, Blend::preset(name));
        }
    }

    #[test]
    fn auto_mode_starts_at_the_top_stop() {
        let state = LightState::new(cycle(), LightingMode::Auto);
        assert_blend_eq(state.current, Blend::preset("bright"));

        let dim_dark = LightingCycleConfig {
            bright_secs: None,
            bright_dim_secs: None,
            ..cycle()
        };
        let state = LightState::new(dim_dark, LightingMode::Auto);
        assert_blend_eq(state.current, Blend::preset("dim"));
    }

    #[test]
    fn blend_follows_the_timeline() {
        for (pos, expected) in [
            (0.0, Blend::preset("bright")),
            (19.9, Blend::preset("bright")),
            (22.0, blend("bright", "dim", 0.5)),
            (27.0, Blend::preset("dim")),
            (31.0, blend("dim", "dark", 0.5)),
            (41.9, Blend::preset("dark")),
            (43.0, blend("dark", "dim", 0.5)),
            (47.0, Blend::preset("dim")),
            (52.0, blend("dim", "bright", 0.5)),
        ] {
            assert_blend_eq(blend_at(&cycle(), pos), expected);
        }
    }

    #[test]
    fn dim_less_cycle_blends_bright_and_dark_directly() {
        // bright [0,20), fade [20,28), dark [28,38), fade [38,46), wrap.
        for (pos, expected) in [
            (0.0, Blend::preset("bright")),
            (24.0, blend("bright", "dark", 0.5)),
            (30.0, Blend::preset("dark")),
            (42.0, blend("dark", "bright", 0.5)),
        ] {
            assert_blend_eq(blend_at(&bright_dark(), pos), expected);
        }
        let mut state = LightState::new(bright_dark(), LightingMode::Auto);
        tick_light(&mut state, 46.0);
        assert_blend_eq(state.current, Blend::preset("bright"));
    }

    #[test]
    fn hold_preset_pauses_the_cycle() {
        let mut state = LightState::new(cycle(), LightingMode::Auto);
        state.hold_preset("dark");
        for _ in 0..100 {
            tick_light(&mut state, 30.0);
        }
        assert_blend_eq(state.current, Blend::preset("dark"));
        assert_eq!(state.status(), "light: dark (held)");
    }

    #[test]
    fn hold_blend_clamps_and_describes() {
        let mut state = LightState::new(cycle(), LightingMode::Bright);
        state.hold_blend("bright", "dark", 2.0);
        assert_blend_eq(state.current, blend("bright", "dark", 1.0));
        state.hold_blend("dim", "dark", 0.25);
        assert_eq!(state.status(), "light: dim\u{2192}dark 0.25 (held)");
    }

    #[test]
    fn hold_cycle_fraction_spans_the_descending_chain() {
        // Full cycle: two fades share the range; 0.75 is mid bright→dim,
        // 0.25 mid dim→dark.
        let mut state = LightState::new(cycle(), LightingMode::Auto);
        state.hold_cycle_fraction(1.0);
        assert_blend_eq(state.current, blend("bright", "dim", 0.0));
        state.hold_cycle_fraction(0.75);
        assert_blend_eq(state.current, blend("bright", "dim", 0.5));
        state.hold_cycle_fraction(0.25);
        assert_blend_eq(state.current, blend("dim", "dark", 0.5));
        state.hold_cycle_fraction(0.0);
        assert_blend_eq(state.current, blend("dim", "dark", 1.0));

        // Dim-less cycle: one fade, the fraction maps directly.
        let mut state = LightState::new(bright_dark(), LightingMode::Auto);
        state.hold_cycle_fraction(0.35);
        assert_blend_eq(state.current, blend("bright", "dark", 0.65));
    }

    #[test]
    fn resume_auto_is_continuous_on_the_path() {
        let mut state = LightState::new(cycle(), LightingMode::Bright);
        state.hold_blend("bright", "dim", 0.5);
        state.resume_auto().expect("resume on the path should succeed");
        tick_light(&mut state, 0.0);
        assert_blend_eq(state.current, blend("bright", "dim", 0.5));

        // The reversed expression of the same point also lands there.
        state.hold_blend("dim", "bright", 0.5);
        state.resume_auto().expect("reversed resume should succeed");
        tick_light(&mut state, 0.0);
        assert_blend_eq(state.current, blend("bright", "dim", 0.5));
    }

    #[test]
    fn resume_auto_off_the_path_enters_at_the_nearest_stop() {
        // Holding dim on a dim-less cycle: dim is not a stop → top stop.
        let mut state = LightState::new(bright_dark(), LightingMode::Dim);
        state.resume_auto().expect("resume off the path should succeed");
        tick_light(&mut state, 0.0);
        assert_blend_eq(state.current, Blend::preset("bright"));

        // Holding a mostly-dark blend on the full cycle: dark is a stop →
        // its hold.
        let mut state = LightState::new(cycle(), LightingMode::Bright);
        state.hold_blend("bright", "dark", 0.9);
        state.resume_auto().expect("resume near dark should succeed");
        tick_light(&mut state, 9.9);
        assert_blend_eq(state.current, Blend::preset("dark"));
    }

    #[test]
    fn resume_auto_while_running_errs() {
        let mut state = LightState::new(cycle(), LightingMode::Auto);
        assert!(state.resume_auto().is_err());
        assert_eq!(state.status(), "light: bright (auto)");
    }

    #[test]
    fn preset_parsing_accepts_only_the_vocabulary() {
        assert_eq!(light_preset_from_str("dim"), Some("dim"));
        assert_eq!(light_preset_from_str("banana"), None);
    }
}
