use super::*;

#[test]
fn hex_color_parses_either_case_and_prints_lowercase() {
    assert_eq!(
        HexColor::parse("#ff0000").expect("lowercase rejected"),
        HexColor([255, 0, 0])
    );
    let mixed = HexColor::parse("#0080FF").expect("mixed case rejected");
    assert_eq!(mixed, HexColor([0, 128, 255]));
    assert_eq!(mixed.to_string(), "#0080ff");
}

#[test]
fn hex_color_rejects_anything_but_hash_and_six_digits() {
    for bad in ["ff0000", "#fff", "", "#gggggg", "#aébbb", "#ff00000"] {
        assert!(HexColor::parse(bad).is_err(), "{bad:?} parsed");
    }
}
