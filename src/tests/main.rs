use super::*;

fn join(args: &[&str]) -> Result<ImpairmentArgs, clap::Error> {
    let mut argv = vec!["cuboid-wars", "join"];
    argv.extend(args);
    match Cli::try_parse_from(argv)?.mode {
        Some(Mode::Join { impairment, .. }) => Ok(impairment),
        _ => panic!("join did not parse as the join mode"),
    }
}

fn world(mode: &[&str], args: &[&str]) -> Result<WorldArgs, clap::Error> {
    let mut argv = vec!["cuboid-wars"];
    argv.extend(mode);
    argv.extend(args);
    let cli = Cli::try_parse_from(argv)?;
    Ok(match cli.mode {
        None => cli.world,
        Some(Mode::Host { world, .. } | Mode::Serve { world, .. }) => world,
        Some(Mode::Join { .. }) => panic!("join has no world arguments"),
    })
}

#[test]
fn impairment_flags_accept_fraction_bounds_and_default_to_five_percent_jitter() {
    assert_eq!(join(&[]).expect("default arguments rejected").jitter, 0.05);
    for flag in ["--jitter", "--drop"] {
        for value in ["0", "0.5", "1"] {
            assert!(join(&[flag, value]).is_ok());
        }
    }
}

#[test]
fn impairment_flags_reject_invalid_fractions_before_connecting() {
    for flag in ["--jitter", "--drop"] {
        for value in ["-0.1", "1.1", "NaN", "inf", "-inf"] {
            let error = join(&[&format!("{flag}={value}")]).expect_err("invalid fraction accepted");
            assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
        }
    }
}

#[test]
fn rate_overrides_accept_positive_integers_in_every_server_mode() {
    let modes: [&[&str]; 3] = [&[], &["host"], &["serve"]];
    for mode in modes {
        let defaults = world(mode, &[]).expect("default arguments invalid");
        assert!(defaults.server_hz.is_none());
        assert!(defaults.update_hz.is_none());
        assert!(defaults.snapshot_hz.is_none());
        for option in ["--server-hz", "--update-hz", "--snapshot-hz"] {
            for hz in ["1", "7", "30", "60"] {
                let args = world(mode, &[option, hz]).expect("rate rejected");
                assert_eq!(
                    args.update_hz.or(args.snapshot_hz).or(args.server_hz),
                    Some(hz.parse().expect("test rate invalid"))
                );
            }
            for hz in ["0", "-1", "10.5"] {
                assert!(world(mode, &[option, hz]).is_err());
            }
        }
    }
}

#[test]
fn top_level_arguments_do_not_mix_with_subcommands() {
    assert!(Cli::try_parse_from(["cuboid-wars", "--map", "hotel", "host"]).is_err());
    assert!(Cli::try_parse_from(["cuboid-wars", "join", "--map", "hotel"]).is_err());
    assert!(Cli::try_parse_from(["cuboid-wars", "serve", "--name", "Alice"]).is_err());
}
