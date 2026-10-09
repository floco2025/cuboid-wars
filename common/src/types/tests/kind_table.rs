use super::*;
use crate::protocol::{FieldId, FieldTable};

fn field_kind_max() -> usize {
    FieldId::MAX.expect("field kind datagram cap missing")
}

#[test]
fn rejects_duplicate_and_empty_ids() {
    let err = FieldTable::from_ids(vec!["a".into(), "a".into()]).expect_err("duplicate ids loaded");
    assert!(err.to_string().contains("duplicate"));
    let err = FieldTable::from_ids(vec!["a".into(), "".into()]).expect_err("empty id loaded");
    assert!(err.to_string().contains("empty"));
}

#[test]
fn accepts_up_to_max_field_kinds_and_no_more() {
    let kinds: Vec<String> = (0..field_kind_max()).map(|i| format!("k{i}")).collect();
    FieldTable::from_ids(kinds).expect("FieldId::MAX kinds rejected");
    let too_many: Vec<String> = (0..=field_kind_max()).map(|i| format!("k{i}")).collect();
    let err = FieldTable::from_ids(too_many).expect_err("over-max kinds loaded");
    assert!(err.to_string().contains("max is"));
}
