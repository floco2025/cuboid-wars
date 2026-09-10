use super::*;

#[test]
fn rate_overrides_accept_positive_integers_and_validate_relationships_after_loading_config() {
    let defaults = Args::try_parse_from(["server"]).expect("default arguments invalid");
    assert!(defaults.server_hz.is_none());
    assert!(defaults.update_hz.is_none());
    assert!(defaults.snapshot_hz.is_none());
    for option in ["--server-hz", "--update-hz", "--snapshot-hz"] {
        for hz in ["1", "7", "30", "60"] {
            let args = Args::try_parse_from(["server", option, hz]).expect("rate rejected");
            assert_eq!(
                args.update_hz.or(args.snapshot_hz).or(args.server_hz),
                Some(hz.parse().expect("test rate invalid"))
            );
        }
        for hz in ["0", "-1", "10.5"] {
            assert!(Args::try_parse_from(["server", option, hz]).is_err());
        }
    }
    assert!(Args::try_parse_from(["server", "--update-hz", "1", "--snapshot-hz", "30"]).is_ok());
}
