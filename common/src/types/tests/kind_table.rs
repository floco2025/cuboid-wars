use super::*;
use crate::{
    config::SwitchHold,
    protocol::{FieldDef, FieldId, FieldTable, SwitchDef, SwitchId, SwitchTable},
};

#[test]
fn a_field_definition_starts_on_without_a_switch_and_round_trips_on_the_wire() {
    let plain = serde_json::json!({"id": "cyan", "color": "#30d8ff"});
    let field: FieldDef = serde_json::from_value(plain.clone()).expect("valid field rejected");
    assert_eq!((field.switch.as_deref(), field.initially_on), (None, true));
    assert_eq!(serde_json::to_value(&field).expect("field does not serialize"), plain);

    let wired = serde_json::json!({"id": "cyan", "color": "#30d8ff", "switch": "lobby", "initially_on": false});
    let field: FieldDef = serde_json::from_value(wired.clone()).expect("wired field rejected");
    assert_eq!((field.switch.as_deref(), field.initially_on), (Some("lobby"), false));
    assert_eq!(serde_json::to_value(&field).expect("field does not serialize"), wired);
    let bytes = bincode::encode_to_vec(&field, bincode::config::standard()).expect("field encoding failed");
    let (decoded, _): (FieldDef, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("field decoding failed");
    assert_eq!(decoded, field);
    assert!(
        serde_json::from_value::<FieldDef>(serde_json::json!({"id": "cyan", "color": "#30d8ff", "kind": 1})).is_err()
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
    let err = FieldTable::from_ids(vec!["a".into(), "a".into()]).expect_err("duplicate ids loaded");
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn rejects_empty_id() {
    let err = FieldTable::from_ids(vec!["a".into(), "".into()]).expect_err("empty id loaded");
    assert!(err.to_string().contains("empty"));
}

fn field_kind_max() -> usize {
    FieldId::MAX.expect("field kind datagram cap missing")
}

#[test]
fn rejects_more_than_max_kinds() {
    let too_many: Vec<String> = (0..=field_kind_max()).map(|i| format!("k{i}")).collect();
    let err = FieldTable::from_ids(too_many).expect_err("over-max kinds loaded");
    assert!(err.to_string().contains("fields has"));
    assert!(err.to_string().contains("max is"));
}

#[test]
fn accepts_exactly_max_field_kinds() {
    let kinds: Vec<String> = (0..field_kind_max()).map(|i| format!("k{i}")).collect();
    FieldTable::from_ids(kinds).expect("FieldId::MAX kinds rejected");
}

#[test]
fn a_table_without_a_cap_takes_more_than_the_field_kind_cap() {
    assert_eq!(SwitchId::MAX, None);
    let switches: Vec<String> = (0..=field_kind_max()).map(|i| format!("k{i}")).collect();
    SwitchTable::from_ids(switches).expect("switches past the field kind cap rejected");
}

#[test]
fn round_trip_index_and_id() {
    let table = FieldTable::from_ids(vec!["basement".into(), "boss_room".into()]).expect("two-kind table rejected");
    assert_eq!(table.index_of("basement"), Some(FieldId(0)));
    assert_eq!(table.index_of("boss_room"), Some(FieldId(1)));
    assert_eq!(table.index_of("unknown"), None);
    assert_eq!(table.id(FieldId(0)), Some("basement"));
    assert_eq!(table.id(FieldId(1)), Some("boss_room"));
    assert_eq!(table.id(FieldId(2)), None);
}

#[test]
fn resolve_names_the_table_noun() {
    let table = FieldTable::from_ids(vec!["skyway".into()]).expect("one-kind table rejected");
    assert_eq!(table.resolve("skyway").expect("registered kind unresolved"), FieldId(0));
    let err = table.resolve("void").expect_err("unregistered kind resolved");
    assert!(err.to_string().contains("unknown field \"void\""), "{err}");
}
