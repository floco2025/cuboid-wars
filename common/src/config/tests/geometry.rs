use crate::test_geometry::{LEVEL_HEIGHT, sizes};

#[test]
fn level_for_y_tolerates_a_small_dip_and_clamps_below_ground() {
    let sizes = sizes();
    assert_eq!(sizes.level_for_y(0.0), 0);
    assert_eq!(sizes.level_for_y(-0.4), 0);
    assert_eq!(sizes.level_for_y(-10.0), 0);
    assert_eq!(sizes.level_for_y(LEVEL_HEIGHT - 0.2), 1);
    assert_eq!(sizes.level_for_y(LEVEL_HEIGHT - 0.6), 0);
    assert_eq!(sizes.level_for_y(2.0 * LEVEL_HEIGHT + 1.0), 2);
}
