use super::*;

#[test]
fn missing_map_returns_contextual_error() {
    let error = generate_map(
        "definitely-not-a-real-map",
        30,
        &crate::config::ServerGameplayConfig::load_default()
            .expect("gameplay config is invalid")
            .maps["hotel"]
            .settings,
        &BarrierKindTable::default(),
        &BridgeKindTable::default(),
        &SwitchTable::default(),
    )
    .err()
    .expect("missing map must fail");

    assert!(error.to_string().contains("failed to load map at"));
    let missing = PathBuf::from("definitely-not-a-real-map").join("layout.json");
    assert!(
        error
            .to_string()
            .contains(missing.to_str().expect("test path is not UTF-8"))
    );
}

#[test]
fn a_map_cannot_reference_an_alias_outside_its_host_catalog() {
    let config = crate::config::ServerGameplayConfig::load_default().expect("gameplay config is invalid");
    let mut settings = config.maps["obby"].settings.clone();
    settings.textures.remove("basement-floor");
    let error = generate_map(
        "obby",
        30,
        &settings,
        &BarrierKindTable::default(),
        &BridgeKindTable::default(),
        &SwitchTable::default(),
    )
    .err()
    .expect("undeclared map material was accepted");
    assert!(error.to_string().contains("basement-floor"), "{error}");
}
