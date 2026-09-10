use common::{
    map::Carriers,
    protocol::{CarrierId, Position},
};

// A reported carrier-local position at the carrier's rendered pose this
// frame, so a rider stays on its platform whatever its sample delay.
#[must_use]
pub(crate) fn rendered_carrier_position(
    carrier: CarrierId,
    pos: &Position,
    carriers: &Carriers,
    carrier_alpha: f32,
) -> Position {
    carriers.pose_between(carrier, carrier_alpha).transform_position(pos)
}
