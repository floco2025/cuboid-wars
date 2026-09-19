use super::*;
use crate::{
    config::SwitchHold,
    protocol::{FieldKindId, FieldKindTable, SwitchDef, SwitchId, SwitchTable},
};

#[test]
fn appearance_kinds_round_trip_and_reject_switch_assignments() {
    let value = serde_json::json!({"id": "cyan", "color": "#30d8ff"});
    let kind: KindDef = serde_json::from_value(value).expect("valid kind rejected");
    let bytes = bincode::encode_to_vec(&kind, bincode::config::standard()).expect("kind encoding failed");
    let (decoded, _): (KindDef, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("kind decoding failed");
    assert_eq!(decoded, kind);
    assert!(
        serde_json::from_value::<KindDef>(serde_json::json!({"id": "cyan", "color": "#30d8ff", "switch": "lobby"}))
            .is_err()
    );
}

#[test]
fn a_switch_definition_defaults_its_hold_and_round_trips_on_the_wire() {
    for activation in ["auto", "momentary", "toggle"] {
        for trigger in ["never", "solo", "any", "all"] {
            let value = serde_json::json!({"id": "lobby", "activation": activation, "reset_on_player_death": trigger});
            let switch: SwitchDef = serde_json::from_value(value).expect("valid switch rejected");
            assert_eq!(switch.policy.held, SwitchHold::Any);
            let bytes = bincode::encode_to_vec(&switch, bincode::config::standard()).expect("switch encoding failed");
            let (decoded, _): (SwitchDef, _) =
                bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("switch decoding failed");
            assert_eq!(decoded, switch);
        }
    }
    let everyone: SwitchDef = serde_json::from_value(serde_json::json!({
        "id": "finale", "activation": "momentary", "reset_on_player_death": "never", "held": "everyone"
    }))
    .expect("everyone hold rejected");
    assert_eq!(everyone.policy.held, SwitchHold::Everyone);
    assert_eq!(everyone.color, None);
    let colored: SwitchDef = serde_json::from_value(serde_json::json!({
        "id": "show", "activation": "toggle", "reset_on_player_death": "never", "color": "#9b5de5"
    }))
    .expect("colored switch rejected");
    let bytes = bincode::encode_to_vec(&colored, bincode::config::standard()).expect("switch encoding failed");
    let (decoded, _): (SwitchDef, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("switch decoding failed");
    assert_eq!(
        decoded.color.expect("plate color missing after wire decode").0,
        [155, 93, 229]
    );
    for value in [
        serde_json::json!({"id": "lobby"}),
        serde_json::json!({"id": "lobby", "activation": "auto"}),
        serde_json::json!({"id": "lobby", "reset_on_player_death": "all"}),
        serde_json::json!({"id": "lobby", "activation": "hold", "reset_on_player_death": "all"}),
        serde_json::json!({"id": "lobby", "activation": "auto", "reset_on_player_death": "always"}),
        serde_json::json!({"id": "lobby", "activation": "auto", "reset_on_player_death": "never", "held": "all"}),
        serde_json::json!({"id": "show", "activation": "auto", "reset_on_player_death": "never", "color": "violet"}),
    ] {
        assert!(serde_json::from_value::<SwitchDef>(value).is_err());
    }
}

#[test]
fn rejects_duplicate_ids() {
    let err = FieldKindTable::from_ids(vec!["a".into(), "a".into()]).expect_err("duplicate ids loaded");
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn rejects_empty_id() {
    let err = FieldKindTable::from_ids(vec!["a".into(), "".into()]).expect_err("empty id loaded");
    assert!(err.to_string().contains("empty"));
}

fn field_kind_max() -> usize {
    FieldKindId::MAX.expect("field kind datagram cap missing")
}

#[test]
fn rejects_more_than_max_kinds() {
    let too_many: Vec<String> = (0..=field_kind_max()).map(|i| format!("k{i}")).collect();
    let err = FieldKindTable::from_ids(too_many).expect_err("over-max kinds loaded");
    assert!(err.to_string().contains("field_kinds has"));
    assert!(err.to_string().contains("max is"));
}

#[test]
fn accepts_exactly_max_field_kinds() {
    let kinds: Vec<String> = (0..field_kind_max()).map(|i| format!("k{i}")).collect();
    FieldKindTable::from_ids(kinds).expect("FieldKindId::MAX kinds rejected");
}

#[test]
fn a_table_without_a_cap_takes_more_than_the_field_kind_cap() {
    assert_eq!(SwitchId::MAX, None);
    let switches: Vec<String> = (0..=field_kind_max()).map(|i| format!("k{i}")).collect();
    SwitchTable::from_ids(switches).expect("switches past the field kind cap rejected");
}

#[test]
fn round_trip_index_and_id() {
    let table = FieldKindTable::from_ids(vec!["basement".into(), "boss_room".into()]).expect("two-kind table rejected");
    assert_eq!(table.index_of("basement"), Some(FieldKindId(0)));
    assert_eq!(table.index_of("boss_room"), Some(FieldKindId(1)));
    assert_eq!(table.index_of("unknown"), None);
    assert_eq!(table.id(FieldKindId(0)), Some("basement"));
    assert_eq!(table.id(FieldKindId(1)), Some("boss_room"));
    assert_eq!(table.id(FieldKindId(2)), None);
}

#[test]
fn resolve_names_the_table_noun() {
    let table = FieldKindTable::from_ids(vec!["skyway".into()]).expect("one-kind table rejected");
    assert_eq!(
        table.resolve("skyway").expect("registered kind unresolved"),
        FieldKindId(0)
    );
    let err = table.resolve("void").expect_err("unregistered kind resolved");
    assert!(err.to_string().contains("unknown field kind"), "{err}");
}
