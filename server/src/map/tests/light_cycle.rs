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
