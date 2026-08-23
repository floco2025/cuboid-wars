use bevy::prelude::*;
use rand::{RngExt, rngs::ThreadRng};

use crate::config::{RainScheduleConfig, StartupWeather};
use common::protocol::Lighting;

// Server-authoritative lighting level, fully decoupled from the weather
// cycle — seeded from the map's `lighting`, then only "/light
// bright|dim|dark" changes it. Rides every snapshot.
#[derive(Resource)]
pub struct CurrentLighting(pub Lighting);

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
// in every snapshot.
#[derive(Resource)]
pub struct WeatherState {
    schedule: Option<RainScheduleConfig>,
    phase: WeatherPhase,
    intensity: f32,
}

impl WeatherState {
    #[must_use]
    pub fn new(schedule: Option<RainScheduleConfig>, startup: StartupWeather) -> Self {
        let mut rng = rand::rng();
        let (phase, intensity) = match (&schedule, startup) {
            (None, _) => (WeatherPhase::Clear { remaining_secs: 0.0 }, 0.0),
            (Some(s), StartupWeather::Clear) => (
                WeatherPhase::Clear {
                    remaining_secs: rng.random_range(s.min_clear_secs..=s.max_clear_secs),
                },
                0.0,
            ),
            (Some(s), StartupWeather::Rain) => (
                WeatherPhase::Raining {
                    remaining_secs: rng.random_range(s.min_rain_secs..=s.max_rain_secs),
                },
                1.0,
            ),
        };
        Self {
            schedule,
            phase,
            intensity,
        }
    }

    #[must_use]
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    // Admin override: begin a rain cycle now. Interrupting a fade scales the
    // ramp by the missing intensity, so the transition stays continuous
    // instead of snapping to zero and climbing back.
    pub fn force_rain_start(&mut self) -> Result<(), &'static str> {
        let Some(schedule) = &self.schedule else {
            return Err("this map has no rain schedule");
        };
        match self.phase {
            WeatherPhase::RampIn { .. } | WeatherPhase::Raining { .. } => Err("already raining"),
            WeatherPhase::Clear { .. } | WeatherPhase::FadeOut { .. } => {
                self.phase = WeatherPhase::RampIn {
                    remaining_secs: schedule.ramp_in_secs * (1.0 - self.intensity),
                };
                Ok(())
            }
        }
    }

    // Admin override: end the current rain cycle now, fading from the
    // current intensity (a mid-ramp stop fades from wherever the ramp got).
    pub fn force_rain_stop(&mut self) -> Result<(), &'static str> {
        let Some(schedule) = &self.schedule else {
            return Err("this map has no rain schedule");
        };
        match self.phase {
            WeatherPhase::Clear { .. } | WeatherPhase::FadeOut { .. } => Err("not raining"),
            WeatherPhase::RampIn { .. } | WeatherPhase::Raining { .. } => {
                self.phase = WeatherPhase::FadeOut {
                    remaining_secs: schedule.fade_out_secs * self.intensity,
                };
                Ok(())
            }
        }
    }
}

pub fn weather_system(time: Res<Time>, mut weather: ResMut<WeatherState>) {
    let mut rng = rand::rng();
    tick_weather(&mut weather, time.delta_secs(), &mut rng);
}

fn tick_weather(state: &mut WeatherState, delta: f32, rng: &mut ThreadRng) {
    let Some(schedule) = state.schedule.clone() else {
        state.intensity = 0.0;
        return;
    };

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
            // Without `auto` the clear and rain stretches re-roll forever —
            // only `force_rain_start` / `force_rain_stop` leave them.
            WeatherPhase::Clear { .. } if !schedule.auto => WeatherPhase::Clear {
                remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
            },
            WeatherPhase::Clear { .. } => WeatherPhase::RampIn {
                remaining_secs: schedule.ramp_in_secs,
            },
            WeatherPhase::RampIn { .. } => WeatherPhase::Raining {
                remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
            },
            WeatherPhase::Raining { .. } if !schedule.auto => WeatherPhase::Raining {
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

    fn schedule() -> RainScheduleConfig {
        RainScheduleConfig {
            auto: true,
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
    fn no_schedule_stays_clear_forever() {
        let mut state = WeatherState::new(None, StartupWeather::Clear);
        for _ in 0..100 {
            tick(&mut state, 10.0);
        }
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn initial_clear_duration_is_within_bounds() {
        let state = WeatherState::new(Some(schedule()), StartupWeather::Clear);
        let WeatherPhase::Clear { remaining_secs } = state.phase else {
            panic!("weather must start clear, got {:?}", state.phase);
        };
        assert!((10.0..=20.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn startup_rain_begins_at_full_intensity_and_ends_on_schedule() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Rain);
        let WeatherPhase::Raining { remaining_secs } = state.phase else {
            panic!("startup rain must begin raining, got {:?}", state.phase);
        };
        assert!((5.0..=8.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 1.0);

        tick(&mut state, 8.0);
        assert!(matches!(state.phase, WeatherPhase::FadeOut { .. }));
    }

    #[test]
    fn startup_rain_without_schedule_stays_clear() {
        let mut state = WeatherState::new(None, StartupWeather::Rain);
        tick(&mut state, 1.0);
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn cycle_advances_through_all_phases_with_bounded_durations() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Clear);

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
    fn manual_mode_rains_and_clears_only_when_forced() {
        // The shipped hotel setup: no automatic transitions.
        let mut state = WeatherState::new(
            Some(RainScheduleConfig {
                auto: false,
                ..schedule()
            }),
            StartupWeather::Clear,
        );

        // Far past every clear duration: still clear.
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert_eq!(state.intensity(), 0.0);

        // The admin command starts it, and far past every rain duration
        // it is still pouring.
        state.force_rain_start().expect("forced start must work without auto");
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Raining { .. }));
        assert_eq!(state.intensity(), 1.0);

        // The admin command ends it, and it stays clear after.
        state.force_rain_stop().expect("forced stop must work without auto");
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn force_start_from_clear_ramps_in() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Clear);

        state.force_rain_start().expect("start from clear should succeed");

        assert_eq!(
            state.phase,
            WeatherPhase::RampIn {
                remaining_secs: schedule().ramp_in_secs
            }
        );
        assert!(state.force_rain_start().is_err(), "second start must report raining");
    }

    #[test]
    fn force_start_mid_fade_keeps_intensity_continuous() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Clear);
        state.phase = WeatherPhase::FadeOut { remaining_secs: 2.0 };
        tick(&mut state, 0.0);
        let mid_fade = state.intensity();
        assert!(mid_fade > 0.0 && mid_fade < 1.0);

        state.force_rain_start().expect("start mid-fade should succeed");
        tick(&mut state, 0.0);

        assert!((state.intensity() - mid_fade).abs() < 1e-3, "no intensity jump");
        assert!(matches!(state.phase, WeatherPhase::RampIn { .. }));
    }

    #[test]
    fn force_stop_while_raining_fades_out() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Clear);
        state.phase = WeatherPhase::Raining { remaining_secs: 100.0 };
        tick(&mut state, 0.0);

        state.force_rain_stop().expect("stop while raining should succeed");

        assert_eq!(
            state.phase,
            WeatherPhase::FadeOut {
                remaining_secs: schedule().fade_out_secs
            }
        );
        assert!(state.force_rain_stop().is_err(), "second stop must report not raining");
    }

    #[test]
    fn force_stop_from_clear_errs() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Clear);
        assert!(state.force_rain_stop().is_err());
    }

    #[test]
    fn forcing_without_schedule_errs() {
        let mut state = WeatherState::new(None, StartupWeather::Clear);
        assert!(state.force_rain_start().is_err());
        assert!(state.force_rain_stop().is_err());
    }

    #[test]
    fn intensity_rises_monotonically_during_ramp() {
        let mut state = WeatherState::new(Some(schedule()), StartupWeather::Clear);
        tick(&mut state, 25.0);

        let mut last = state.intensity();
        for _ in 0..10 {
            tick(&mut state, 0.1);
            assert!(state.intensity() >= last);
            last = state.intensity();
        }
    }
}
