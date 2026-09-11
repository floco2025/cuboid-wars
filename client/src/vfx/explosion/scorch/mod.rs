mod clip;
mod marks;
mod placement;
mod variants;

#[cfg(test)]
pub(super) use marks::ScorchMark;
pub use marks::scorch_marks_system;
pub(super) use marks::{SCORCH_SURFACE_OFFSET, spawn_scorch_mark};
pub(super) use placement::{
    SurfaceContact, ground_scorch_placement, surface_cross_section_diameter, wall_scorch_placements,
};
pub(crate) use variants::ScorchOutline;
pub(super) use variants::{ScorchStyle, ScorchVariant, scorch_variant};
