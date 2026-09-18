mod nesting;
mod records;
mod surfaces;
pub use nesting::{nested_cycle, placed_definitions};
mod validation;
pub(crate) use validation::{checkpoint_numbers, plates};
pub use validation::{validate_catalog, validate_document, validate_map};
mod shape;
