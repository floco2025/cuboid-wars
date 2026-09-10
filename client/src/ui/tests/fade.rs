use super::*;

#[test]
fn fade_holds_then_falls_linearly() {
    assert_eq!(fade_out_alpha(5.0, 1.0), 1.0);
    assert_eq!(fade_out_alpha(0.5, 1.0), 0.5);
    assert_eq!(fade_out_alpha(0.0, 1.0), 0.0);
}
