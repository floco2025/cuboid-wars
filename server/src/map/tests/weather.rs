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
