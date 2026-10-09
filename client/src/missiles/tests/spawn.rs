use super::*;
use crate::constants::MISSILE_RADIUS;

// The collision/fuse ball (`MISSILE_RADIUS`) is what keeps the
// missile clear of geometry; the rendered mesh must fit inside it
// radially or missiles visibly clip walls they fly along. The two are
// deliberately tuned separately — this pins the invariant, not a ratio.
#[test]
fn rendered_missile_fits_inside_the_collision_ball() {
    let widest_radial_extent = MISSILE_BODY_RADIUS + MISSILE_FIN_SPAN;
    assert!(
        widest_radial_extent <= MISSILE_RADIUS,
        "missile mesh ({widest_radial_extent} m radial) exceeds the collision ball ({MISSILE_RADIUS} m)"
    );
}
