use clap::error::ErrorKind;

use super::*;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    let mut argv = vec!["cuboid-wars"];
    argv.extend(args);
    Cli::try_parse_from(argv)
}

#[test]
fn experiment_mode_accepts_only_its_script() {
    assert_eq!(
        parse(&["--experiment", "hotel"]).expect("experiment").experiment,
        Some("hotel".to_owned())
    );
    for arguments in [
        &["--host"][..],
        &["--join"],
        &["--serve"],
        &["--map", "obby"],
        &["--god"],
        &["--peace"],
        &["--name", "Reviewer"],
        &["--windowed"],
        &["--spawn", "0,0,0"],
        &["--server-hz", "60"],
        &["--lag-ms", "10"],
    ] {
        assert!(
            parse(&[&["--experiment", "trial.json"][..], arguments].concat()).is_err(),
            "{arguments:?}"
        );
    }
}

#[test]
fn playing_an_experiment_accepts_window_options_and_rejects_other_world_sources() {
    let cli = parse(&[
        "--play-experiment",
        "trial.json",
        "--windowed",
        "--look",
        "90,-20",
        "--name",
        "Player",
    ])
    .expect("interactive experiment rejected");
    assert_eq!(cli.play_experiment, Some("trial.json".to_owned()));
    assert!(cli.window.windowed);
    for args in [
        &["--experiment", "other.json"][..],
        &["--host"],
        &["--join"],
        &["--serve"],
        &["--map", "obby"],
        &["--spawn", "0,0,0"],
        &["--god"],
        &["--server-hz", "60"],
        &["--lag-ms", "10"],
    ] {
        assert_eq!(
            parse(&[&["--play-experiment", "trial.json"][..], args].concat())
                .expect_err("conflicting source accepted")
                .kind(),
            ErrorKind::ArgumentConflict
        );
    }
}

#[test]
fn review_window_and_view_options_parse_without_changing_saved_settings() {
    let cli = parse(&["--windowed", "--resolution", "1280x720", "--look", "90,-12.5"])
        .expect("review window arguments rejected");
    let options = cli.window.client_options(false);
    assert!(options.force_windowed);
    assert_eq!(options.window_width, Some(1280));
    assert_eq!(options.window_height, Some(720));
    assert_eq!(
        options.initial_view,
        Some(InitialViewDirection {
            bearing_degrees: 90.0,
            pitch_degrees: -12.5,
        })
    );
    // A display left of or above the main one has negative coordinates.
    let placed = parse(&["--window-x", "-312", "--window-y", "-40"]).expect("negative window position rejected");
    assert_eq!(
        (placed.window.window_x, placed.window.window_y),
        (Some(-312), Some(-40))
    );

    for invalid in ["1280", "0x720", "1280x0", "wide"] {
        assert!(parse(&["--resolution", invalid]).is_err(), "accepted {invalid:?}");
    }
    for invalid in ["north", "0", "0,91", "NaN,0"] {
        assert!(parse(&["--look", invalid]).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn single_player_spawn_override_accepts_finite_world_coordinates_only() {
    let cli = parse(&["--spawn=-14.5,4.4,27"]).expect("spawn override rejected");
    assert_eq!(
        cli.world.server_options().initial_spawn,
        Some(Position {
            x: -14.5,
            y: 4.4,
            z: 27.0,
        })
    );
    for mode in ["--host", "--join", "--serve"] {
        assert_eq!(
            parse(&[mode, "--spawn", "1,2,3"])
                .expect_err("spawn override accepted outside single-player")
                .kind(),
            ErrorKind::ArgumentConflict
        );
    }
    for invalid in ["1,2", "1,2,3,4", "NaN,2,3", "1,inf,3"] {
        assert!(parse(&["--spawn", invalid]).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn impairment_flags_need_join() {
    for flag in ["--jitter", "--drop"] {
        for value in ["0", "0.5", "1"] {
            assert!(parse(&["--join", flag, value]).is_ok());
        }
    }
    let error = parse(&["--lag-ms", "5"]).expect_err("impairment accepted in single-player");
    assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    for args in [&["--host", "--jitter", "0.5"][..], &["--serve", "--drop", "0.1"]] {
        let error = parse(args).expect_err("impairment accepted with another mode");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{args:?}");
    }
    assert!(
        parse(&["--host"]).is_ok(),
        "defaulted impairment must not demand --join"
    );
}

#[test]
fn impairment_flags_reject_invalid_fractions_before_connecting() {
    for flag in ["--jitter", "--drop"] {
        for value in ["-0.1", "1.1", "NaN", "inf", "-inf"] {
            let error = parse(&["--join", &format!("{flag}={value}")]).expect_err("invalid fraction accepted");
            assert_eq!(error.kind(), ErrorKind::ValueValidation);
        }
    }
}
