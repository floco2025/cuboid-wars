use super::*;

#[test]
fn impairment_flags_accept_fraction_bounds_and_default_to_five_percent_jitter() {
    let args = Args::try_parse_from(["client"]).expect("default arguments rejected");
    assert_eq!(args.jitter, 0.05);
    for flag in ["--jitter", "--drop"] {
        for value in ["0", "0.5", "1"] {
            assert!(Args::try_parse_from(["client", flag, value]).is_ok());
        }
    }
}

#[test]
fn impairment_flags_reject_invalid_fractions_before_connecting() {
    for flag in ["--jitter", "--drop"] {
        for value in ["-0.1", "1.1", "NaN", "inf", "-inf"] {
            let error =
                Args::try_parse_from(["client", &format!("{flag}={value}")]).expect_err("invalid fraction accepted");
            assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
        }
    }
}
