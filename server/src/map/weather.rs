use bevy::prelude::*;
use rand::{RngExt, rngs::ThreadRng};

use crate::config::{WeatherCycleConfig, WeatherMode};

// Cycles Clear → RampIn → Raining → FadeOut → Clear. Each variant carries
// its countdown; ramp fractions are derived from the config's fixed ramp
// lengths, so no totals need storing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum WeatherPhase {
    Clear { remaining_secs: f32 },
    RampIn { remaining_secs: f32 },
    Raining { remaining_secs: f32 },
    FadeOut { remaining_secs: f32 },
}

// Server-scheduled weather for the loaded map. `intensity` is the single
// authoritative scalar clients drive all rain presentation from; it ships
// in every snapshot. With `auto` off the current state holds (the map's
// concrete mode, or an admin override) until `/weather` changes it.
#[derive(Resource)]
pub struct WeatherState {
    schedule: WeatherCycleConfig,
    phase: WeatherPhase,
    intensity: f32,
    auto: bool,
}

impl WeatherState {
    #[must_use]
    pub fn new(schedule: WeatherCycleConfig, mode: WeatherMode) -> Self {
        let mut rng = rand::rng();
        let (phase, intensity) = match mode {
            WeatherMode::Clear | WeatherMode::Auto => (
                WeatherPhase::Clear {
                    remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
                },
                0.0,
            ),
            WeatherMode::Rain => (
                WeatherPhase::Raining {
                    remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
                },
                1.0,
            ),
        };
        Self {
            schedule,
            phase,
            intensity,
            auto: mode == WeatherMode::Auto,
        }
    }

    #[must_use]
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    // Admin override: rain now and hold it. Interrupting a fade scales the
    // ramp by the missing intensity, so the transition stays continuous
    // instead of snapping to zero and climbing back.
    pub fn hold_rain(&mut self) -> Result<(), &'static str> {
        match self.phase {
            WeatherPhase::RampIn { .. } | WeatherPhase::Raining { .. } => {
                if self.auto {
                    self.auto = false;
                    Ok(())
                } else {
                    Err("already raining")
                }
            }
            WeatherPhase::Clear { .. } | WeatherPhase::FadeOut { .. } => {
                self.phase = WeatherPhase::RampIn {
                    remaining_secs: self.schedule.ramp_in_secs * (1.0 - self.intensity),
                };
                self.auto = false;
                Ok(())
            }
        }
    }

    // Admin override: clear now and hold it, fading from the current
    // intensity (a mid-ramp stop fades from wherever the ramp got).
    pub fn hold_clear(&mut self) -> Result<(), &'static str> {
        match self.phase {
            WeatherPhase::Clear { .. } | WeatherPhase::FadeOut { .. } => {
                if self.auto {
                    self.auto = false;
                    Ok(())
                } else {
                    Err("not raining")
                }
            }
            WeatherPhase::RampIn { .. } | WeatherPhase::Raining { .. } => {
                self.phase = WeatherPhase::FadeOut {
                    remaining_secs: self.schedule.fade_out_secs * self.intensity,
                };
                self.auto = false;
                Ok(())
            }
        }
    }

    // Admin override: hand control back to the cycle. The held phase simply
    // keeps counting down, so the transition out is the scheduled one.
    pub fn resume_auto(&mut self) -> Result<(), &'static str> {
        if self.auto {
            return Err("weather cycle already running");
        }
        self.auto = true;
        Ok(())
    }

    #[must_use]
    pub fn status(&self) -> String {
        let phase = match self.phase {
            WeatherPhase::Clear { .. } => "clear".to_owned(),
            WeatherPhase::Raining { .. } => "rain".to_owned(),
            WeatherPhase::RampIn { .. } => format!("rain starting ({:.2})", self.intensity),
            WeatherPhase::FadeOut { .. } => format!("clearing ({:.2})", self.intensity),
        };
        let source = if self.auto { "auto" } else { "held" };
        format!("weather: {phase} ({source})")
    }
}

pub fn weather_system(time: Res<Time>, mut weather: ResMut<WeatherState>) {
    let mut rng = rand::rng();
    tick_weather(&mut weather, time.delta_secs(), &mut rng);
}

fn tick_weather(state: &mut WeatherState, delta: f32, rng: &mut ThreadRng) {
    let schedule = state.schedule.clone();
    let remaining = match &mut state.phase {
        WeatherPhase::Clear { remaining_secs }
        | WeatherPhase::RampIn { remaining_secs }
        | WeatherPhase::Raining { remaining_secs }
        | WeatherPhase::FadeOut { remaining_secs } => {
            *remaining_secs -= delta;
            *remaining_secs
        }
    };
    if remaining <= 0.0 {
        state.phase = match state.phase {
            // Held states re-roll forever — only `hold_rain` / `hold_clear`
            // / `resume_auto` leave them.
            WeatherPhase::Clear { .. } if !state.auto => WeatherPhase::Clear {
                remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
            },
            WeatherPhase::Clear { .. } => WeatherPhase::RampIn {
                remaining_secs: schedule.ramp_in_secs,
            },
            WeatherPhase::RampIn { .. } => WeatherPhase::Raining {
                remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
            },
            WeatherPhase::Raining { .. } if !state.auto => WeatherPhase::Raining {
                remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
            },
            WeatherPhase::Raining { .. } => WeatherPhase::FadeOut {
                remaining_secs: schedule.fade_out_secs,
            },
            WeatherPhase::FadeOut { .. } => WeatherPhase::Clear {
                remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
            },
        };
    }

    state.intensity = match state.phase {
        WeatherPhase::Clear { .. } => 0.0,
        WeatherPhase::Raining { .. } => 1.0,
        WeatherPhase::RampIn { remaining_secs } => 1.0 - remaining_secs / schedule.ramp_in_secs,
        WeatherPhase::FadeOut { remaining_secs } => remaining_secs / schedule.fade_out_secs,
    }
    .clamp(0.0, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle() -> WeatherCycleConfig {
        WeatherCycleConfig {
            min_clear_secs: 10.0,
            max_clear_secs: 20.0,
            min_rain_secs: 5.0,
            max_rain_secs: 8.0,
            ramp_in_secs: 2.0,
            fade_out_secs: 4.0,
        }
    }

    fn tick(state: &mut WeatherState, delta: f32) {
        tick_weather(state, delta, &mut rand::rng());
    }

    #[test]
    fn initial_clear_duration_is_within_bounds() {
        let state = WeatherState::new(cycle(), WeatherMode::Auto);
        let WeatherPhase::Clear { remaining_secs } = state.phase else {
            panic!("weather must start clear, got {:?}", state.phase);
        };
        assert!((10.0..=20.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn mode_clear_holds_clear_forever() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Clear);
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn mode_rain_starts_raining_and_holds() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Rain);
        let WeatherPhase::Raining { remaining_secs } = state.phase else {
            panic!("rain mode must start raining, got {:?}", state.phase);
        };
        assert!((5.0..=8.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 1.0);

        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Raining { .. }));
        assert_eq!(state.intensity(), 1.0);
    }

    #[test]
    fn mode_auto_cycles_through_all_phases_with_bounded_durations() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Auto);

        // Exhaust the clear stretch.
        tick(&mut state, 25.0);
        assert!(matches!(state.phase, WeatherPhase::RampIn { .. }));

        // Mid-ramp the intensity is strictly between the endpoints.
        tick(&mut state, 1.0);
        assert!(matches!(state.phase, WeatherPhase::RampIn { .. }));
        assert!(state.intensity() > 0.0 && state.intensity() < 1.0);

        tick(&mut state, 1.0);
        let WeatherPhase::Raining { remaining_secs } = state.phase else {
            panic!("expected rain after the ramp, got {:?}", state.phase);
        };
        assert!((5.0..=8.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 1.0);

        tick(&mut state, 8.0);
        assert!(matches!(state.phase, WeatherPhase::FadeOut { .. }));
        tick(&mut state, 2.0);
        assert!(state.intensity() > 0.0 && state.intensity() < 1.0);

        tick(&mut state, 2.0);
        let WeatherPhase::Clear { remaining_secs } = state.phase else {
            panic!("expected clear after the fade, got {:?}", state.phase);
        };
        assert!((10.0..=20.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn hold_rain_from_clear_ramps_in_and_holds() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Clear);

        state.hold_rain().expect("hold_rain from clear should succeed");
        assert_eq!(
            state.phase,
            WeatherPhase::RampIn {
                remaining_secs: cycle().ramp_in_secs
            }
        );

        tick(&mut state, 3.0);
        assert_eq!(state.intensity(), 1.0);
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Raining { .. }));
        assert!(state.hold_rain().is_err(), "second hold_rain must report raining");
    }

    #[test]
    fn hold_rain_mid_fade_keeps_intensity_continuous() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Auto);
        state.phase = WeatherPhase::FadeOut { remaining_secs: 2.0 };
        tick(&mut state, 0.0);
        let mid_fade = state.intensity();
        assert!(mid_fade > 0.0 && mid_fade < 1.0);

        state.hold_rain().expect("hold_rain mid-fade should succeed");
        tick(&mut state, 0.0);

        assert!((state.intensity() - mid_fade).abs() < 1e-3, "no intensity jump");
        assert!(matches!(state.phase, WeatherPhase::RampIn { .. }));
    }

    #[test]
    fn hold_clear_while_raining_fades_out_and_holds() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Rain);

        state.hold_clear().expect("hold_clear while raining should succeed");
        assert_eq!(
            state.phase,
            WeatherPhase::FadeOut {
                remaining_secs: cycle().fade_out_secs
            }
        );

        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert!(state.hold_clear().is_err(), "second hold_clear must report not raining");
    }

    #[test]
    fn hold_pauses_a_running_cycle_in_place() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Auto);
        state
            .hold_clear()
            .expect("holding the auto clear stretch should succeed");
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));

        let mut state = WeatherState::new(cycle(), WeatherMode::Auto);
        tick(&mut state, 25.0);
        tick(&mut state, 2.0);
        assert!(matches!(state.phase, WeatherPhase::Raining { .. }));
        state.hold_rain().expect("holding the auto rain stretch should succeed");
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Raining { .. }));
        assert_eq!(state.intensity(), 1.0);
    }

    #[test]
    fn resume_auto_continues_the_cycle() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Clear);
        state.resume_auto().expect("resume from a held state should succeed");
        assert!(state.resume_auto().is_err(), "second resume must report running");

        // The held clear stretch now ends into a ramp on its own.
        tick(&mut state, 25.0);
        assert!(matches!(state.phase, WeatherPhase::RampIn { .. }));
    }

    #[test]
    fn status_names_phase_and_source() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Clear);
        assert_eq!(state.status(), "weather: clear (held)");
        state.resume_auto().expect("resume from held clear should succeed");
        assert_eq!(state.status(), "weather: clear (auto)");
        state.hold_rain().expect("hold rain from clear should succeed");
        tick(&mut state, 3.0);
        assert_eq!(state.status(), "weather: rain (held)");
    }

    #[test]
    fn intensity_rises_monotonically_during_ramp() {
        let mut state = WeatherState::new(cycle(), WeatherMode::Auto);
        tick(&mut state, 25.0);

        let mut last = state.intensity();
        for _ in 0..10 {
            tick(&mut state, 0.1);
            assert!(state.intensity() >= last);
            last = state.intensity();
        }
    }
}
