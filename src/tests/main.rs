use clap::error::ErrorKind;

use super::*;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    let mut argv = vec!["cuboid-wars"];
    argv.extend(args);
    Cli::try_parse_from(argv)
}

fn address(text: &str) -> SocketAddr {
    text.parse().expect("test address invalid")
}

#[test]
fn mode_flags_take_an_optional_address_and_default_to_localhost() {
    let cli = parse(&[]).expect("bare invocation rejected");
    assert!(cli.host.is_none() && cli.join.is_none() && cli.serve.is_none());
    let default = address(DEFAULT_ADDRESS);
    assert_eq!(parse(&["--host"]).expect("bare --host rejected").host, Some(default));
    assert_eq!(parse(&["--join"]).expect("bare --join rejected").join, Some(default));
    assert_eq!(parse(&["--serve"]).expect("bare --serve rejected").serve, Some(default));
    let lan = address("0.0.0.0:8080");
    assert_eq!(
        parse(&["--host", "0.0.0.0:8080", "--map", "hotel"])
            .expect("--host with an address rejected")
            .host,
        Some(lan)
    );
    assert_eq!(
        parse(&["--join", "0.0.0.0:8080", "--name", "Alex"])
            .expect("--join with an address rejected")
            .join,
        Some(lan)
    );
    assert_eq!(
        parse(&["--serve", "0.0.0.0:8080", "--server-hz", "60"])
            .expect("--serve with an address rejected")
            .serve,
        Some(lan)
    );
}

#[test]
fn modes_are_mutually_exclusive() {
    for pair in [["--host", "--join"], ["--host", "--serve"], ["--join", "--serve"]] {
        let error = parse(&pair).expect_err("two modes accepted");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }
}

#[test]
fn window_options_need_a_window_and_world_options_a_server() {
    for args in [
        &["--serve", "--name", "Alex"][..],
        &["--serve", "--window-x", "10"],
        &["--serve", "--windowed"],
        &["--serve", "--resolution", "1200x800"],
        &["--serve", "--look", "0,-10"],
        &["--serve", "--volume", "0.5"],
        &["--join", "--map", "hotel"],
        &["--join", "--server-hz", "30"],
        &["--join", "--god"],
        &["--join", "--peace"],
    ] {
        let error = parse(args).expect_err("option accepted in the wrong mode");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{args:?}");
    }
    assert!(parse(&["--serve", "--map", "hotel"]).is_ok());
    assert!(parse(&["--join", "--name", "Alex", "--volume", "0.5"]).is_ok());
    assert!(parse(&["--host", "--name", "Alex", "--map", "hotel"]).is_ok());
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
    assert!(!options.logging);

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
fn god_and_peace_enable_independently_in_every_server_mode() {
    let modes: [&[&str]; 3] = [&[], &["--host"], &["--serve"]];
    for mode in modes {
        for god in [false, true] {
            for peace in [false, true] {
                let mut args = mode.to_vec();
                if god {
                    args.push("--god");
                }
                if peace {
                    args.push("--peace");
                }
                let options = parse(&args)
                    .expect("startup mode arguments rejected")
                    .world
                    .server_options();
                assert_eq!(options.god, god, "{args:?}");
                assert_eq!(options.peace, peace, "{args:?}");
            }
        }
    }
}

#[test]
fn impairment_flags_need_join_and_default_to_five_percent_jitter() {
    assert_eq!(
        parse(&["--join"]).expect("bare --join rejected").impairment.jitter,
        0.05
    );
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

#[test]
fn rate_overrides_accept_positive_integers_in_every_server_mode() {
    let modes: [&[&str]; 3] = [&[], &["--host"], &["--serve"]];
    for mode in modes {
        let defaults = parse(mode).expect("default arguments invalid").world;
        assert!(defaults.server_hz.is_none());
        assert!(defaults.update_hz.is_none());
        assert!(defaults.snapshot_hz.is_none());
        for option in ["--server-hz", "--update-hz", "--snapshot-hz"] {
            for hz in ["1", "7", "30", "60"] {
                let mut argv = mode.to_vec();
                argv.extend([option, hz]);
                let world = parse(&argv).expect("rate rejected").world;
                assert_eq!(
                    world.update_hz.or(world.snapshot_hz).or(world.server_hz),
                    Some(hz.parse().expect("test rate invalid"))
                );
            }
            for hz in ["0", "-1", "10.5"] {
                let mut argv = mode.to_vec();
                argv.extend([option, hz]);
                assert!(parse(&argv).is_err());
            }
        }
    }
}
