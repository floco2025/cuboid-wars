use bevy::prelude::*;
use rand::{RngExt, rngs::ThreadRng};

use crate::config::RainScheduleConfig;

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
    pub fn new(schedule: Option<RainScheduleConfig>) -> Self {
        let mut rng = rand::rng();
        let phase = WeatherPhase::Clear {
            remaining_secs: schedule
                .as_ref()
                .map_or(0.0, |s| rng.random_range(s.min_clear_secs..=s.max_clear_secs)),
        };
        Self {
            schedule,
            phase,
            intensity: 0.0,
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
            // With auto-start disabled the clear stretch just re-rolls
            // forever — only `force_rain_start` leaves it.
            WeatherPhase::Clear { .. } if !schedule.auto_start => WeatherPhase::Clear {
                remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
            },
            WeatherPhase::Clear { .. } => WeatherPhase::RampIn {
                remaining_secs: schedule.ramp_in_secs,
            },
            WeatherPhase::RampIn { .. } => WeatherPhase::Raining {
                remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
            },
            // With auto-end disabled the rain stretch re-rolls forever —
            // only `force_rain_stop` leaves it.
            WeatherPhase::Raining { .. } if !schedule.auto_end => WeatherPhase::Raining {
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
            auto_start: true,
            auto_end: true,
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
        let mut state = WeatherState::new(None);
        for _ in 0..100 {
            tick(&mut state, 10.0);
        }
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn initial_clear_duration_is_within_bounds() {
        let state = WeatherState::new(Some(schedule()));
        let WeatherPhase::Clear { remaining_secs } = state.phase else {
            panic!("weather must start clear, got {:?}", state.phase);
        };
        assert!((10.0..=20.0).contains(&remaining_secs));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn cycle_advances_through_all_phases_with_bounded_durations() {
        let mut state = WeatherState::new(Some(schedule()));

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
    fn auto_start_disabled_stays_clear_until_forced() {
        let mut state = WeatherState::new(Some(RainScheduleConfig {
            auto_start: false,
            ..schedule()
        }));

        // Far past every clear duration: still clear.
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert_eq!(state.intensity(), 0.0);

        // The admin command still starts it, and the rolled rain duration
        // still ends it by itself.
        state
            .force_rain_start()
            .expect("forced start must work without auto_start");
        tick(&mut state, 3.0);
        assert_eq!(state.intensity(), 1.0);
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn all_admin_mode_rains_and_clears_only_when_forced() {
        // The shipped hotel setup: both automatic transitions disabled.
        let mut state = WeatherState::new(Some(RainScheduleConfig {
            auto_start: false,
            auto_end: false,
            ..schedule()
        }));
        state.force_rain_start().expect("start from clear should succeed");

        // Far past every rain duration: still pouring.
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Raining { .. }));
        assert_eq!(state.intensity(), 1.0);

        // The admin command still ends it, and it stays clear after.
        state.force_rain_stop().expect("forced stop must work without auto_end");
        for _ in 0..100 {
            tick(&mut state, 30.0);
        }
        assert!(matches!(state.phase, WeatherPhase::Clear { .. }));
        assert_eq!(state.intensity(), 0.0);
    }

    #[test]
    fn force_start_from_clear_ramps_in() {
        let mut state = WeatherState::new(Some(schedule()));

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
        let mut state = WeatherState::new(Some(schedule()));
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
        let mut state = WeatherState::new(Some(schedule()));
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
        let mut state = WeatherState::new(Some(schedule()));
        assert!(state.force_rain_stop().is_err());
    }

    #[test]
    fn forcing_without_schedule_errs() {
        let mut state = WeatherState::new(None);
        assert!(state.force_rain_start().is_err());
        assert!(state.force_rain_stop().is_err());
    }

    #[test]
    fn intensity_rises_monotonically_during_ramp() {
        let mut state = WeatherState::new(Some(schedule()));
        tick(&mut state, 25.0);

        let mut last = state.intensity();
        for _ in 0..10 {
            tick(&mut state, 0.1);
            assert!(state.intensity() >= last);
            last = state.intensity();
        }
    }
}
