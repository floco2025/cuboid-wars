use super::*;

#[test]
fn a_body_passes_the_fields_that_are_off_and_the_ones_it_holds_a_key_to() {
    assert!(passable_fields(&[], &[]).is_empty());
    assert_eq!(passable_fields(&[FieldId(1)], &[]), [FieldId(1)]);
    assert_eq!(passable_fields(&[], &[FieldId(2)]), [FieldId(2)]);
    assert_eq!(
        passable_fields(&[FieldId(3), FieldId(1)], &[FieldId(2), FieldId(1)]),
        [FieldId(1), FieldId(2), FieldId(3)]
    );
}
