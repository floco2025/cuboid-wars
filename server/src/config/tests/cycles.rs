use super::*;

fn ok_weather_cycle() -> WeatherCycleConfig {
    WeatherCycleConfig {
        min_clear_secs: 10.0,
        max_clear_secs: 20.0,
        min_rain_secs: 5.0,
        max_rain_secs: 8.0,
        ramp_in_secs: 2.0,
        fade_out_secs: 4.0,
    }
}

#[test]
fn weather_cycle_accepts_valid_config() {
    ok_weather_cycle()
        .validate("cycles.weather")
        .expect("valid weather cycle should pass");
}

#[test]
fn weather_cycle_rejects_min_clear_above_max() {
    let mut cycle = ok_weather_cycle();
    cycle.min_clear_secs = 30.0;
    let err = cycle
        .validate("cycles.weather")
        .expect_err("min_clear > max_clear must be rejected");
    assert!(err.to_string().contains("min_clear_secs"));
}

#[test]
fn weather_cycle_rejects_min_rain_above_max() {
    let mut cycle = ok_weather_cycle();
    cycle.min_rain_secs = 30.0;
    let err = cycle
        .validate("cycles.weather")
        .expect_err("min_rain > max_rain must be rejected");
    assert!(err.to_string().contains("min_rain_secs"));
}

#[test]
fn weather_cycle_rejects_non_positive_ramp() {
    let mut cycle = ok_weather_cycle();
    cycle.ramp_in_secs = 0.0;
    let err = cycle
        .validate("cycles.weather")
        .expect_err("zero ramp_in must be rejected");
    assert!(err.to_string().contains("ramp_in_secs"));
}

fn full_lighting_cycle() -> LightingCycleConfig {
    LightingCycleConfig {
        bright_secs: Some(240.0),
        dim_secs: Some(45.0),
        dark_secs: Some(120.0),
        bright_dim_secs: Some(20.0),
        dim_dark_secs: Some(20.0),
        bright_dark_secs: None,
    }
}

#[test]
fn lighting_cycle_parses_and_validates() {
    let cycle: LightingCycleConfig = serde_json::from_str(
        r#"{"bright_secs": 240.0, "dim_secs": 45.0, "dark_secs": 120.0, "bright_dim_secs": 20.0, "dim_dark_secs": 20.0, "bright_dark_secs": null}"#,
    )
    .expect("lighting cycle should deserialize");
    cycle
        .validate("cycles.lighting")
        .expect("valid lighting cycle should pass");
}

#[test]
fn lighting_cycle_rejects_non_positive_durations() {
    let ok = full_lighting_cycle();
    for (cycle, field) in [
        (
            LightingCycleConfig {
                bright_secs: Some(0.0),
                ..ok.clone()
            },
            "bright_secs",
        ),
        (
            LightingCycleConfig {
                dim_secs: Some(-1.0),
                ..ok.clone()
            },
            "dim_secs",
        ),
        (
            LightingCycleConfig {
                bright_dim_secs: Some(0.0),
                ..ok.clone()
            },
            "bright_dim_secs",
        ),
    ] {
        let err = cycle
            .validate("cycles.lighting")
            .expect_err("non-positive duration must be rejected");
        assert!(err.to_string().contains(field));
    }
}

#[test]
fn lighting_cycle_accepts_two_stop_variants() {
    let bright_dim = LightingCycleConfig {
        dark_secs: None,
        dim_dark_secs: None,
        ..full_lighting_cycle()
    };
    bright_dim
        .validate("cycles.lighting")
        .expect("bright+dim cycle should pass");

    let dim_dark = LightingCycleConfig {
        bright_secs: None,
        bright_dim_secs: None,
        ..full_lighting_cycle()
    };
    dim_dark
        .validate("cycles.lighting")
        .expect("dim+dark cycle should pass");

    let bright_dark = LightingCycleConfig {
        dim_secs: None,
        bright_dim_secs: None,
        dim_dark_secs: None,
        bright_dark_secs: Some(30.0),
        ..full_lighting_cycle()
    };
    bright_dark
        .validate("cycles.lighting")
        .expect("bright+dark cycle should pass");
}

#[test]
fn lighting_cycle_rejects_single_stop() {
    let cycle = LightingCycleConfig {
        dim_secs: None,
        dark_secs: None,
        bright_dim_secs: None,
        dim_dark_secs: None,
        ..full_lighting_cycle()
    };
    let err = cycle
        .validate("cycles.lighting")
        .expect_err("single-stop cycle must be rejected");
    assert!(err.to_string().contains("at least two"));
}

#[test]
fn lighting_cycle_rejects_missing_and_unused_fades() {
    let missing = LightingCycleConfig {
        bright_dim_secs: None,
        ..full_lighting_cycle()
    };
    let err = missing
        .validate("cycles.lighting")
        .expect_err("missing bright_dim fade must be rejected");
    assert!(err.to_string().contains("bright_dim_secs is required"));

    let unused = LightingCycleConfig {
        bright_dark_secs: Some(30.0),
        ..full_lighting_cycle()
    };
    let err = unused
        .validate("cycles.lighting")
        .expect_err("bright_dark fade with dim present must be rejected");
    assert!(err.to_string().contains("bright_dark_secs is not used"));

    let bright_dark_missing = LightingCycleConfig {
        dim_secs: None,
        bright_dim_secs: None,
        dim_dark_secs: None,
        bright_dark_secs: None,
        ..full_lighting_cycle()
    };
    let err = bright_dark_missing
        .validate("cycles.lighting")
        .expect_err("bright+dark cycle without its fade must be rejected");
    assert!(err.to_string().contains("bright_dark_secs is required"));
}
