mod geometry;
mod levels;

pub use geometry::MapGeometry;
pub use levels::{RampAxis, compute_player_level, find_support_floor, is_on_ramp, ramp_axis, ramp_surface_at};
