use super::*;

#[test]
fn portal_views_cycle_forward_and_wrap() {
    assert_eq!(cycle_portal_views(0, 1), 1);
    assert_eq!(cycle_portal_views(2, 1), 4);
    assert_eq!(cycle_portal_views(8, 1), 0);
}

#[test]
fn portal_views_cycle_backward_and_wrap() {
    assert_eq!(cycle_portal_views(1, -1), 0);
    assert_eq!(cycle_portal_views(0, -1), 8);
}

#[test]
fn portal_views_off_the_ladder_step_to_a_neighbour() {
    assert_eq!(cycle_portal_views(6, 1), 8);
    assert_eq!(cycle_portal_views(6, -1), 4);
}
