//! Map source rules shared by the server and the native editor binding.
mod authoring;
mod diagnostics;
pub mod geometry;
pub mod load;
pub mod schema;
pub mod settings;
mod transforms;
mod values;

pub use authoring::dispatch;
pub use load::load_map;
pub use settings::{MAP_CONTENT_KEYS, merge_map_settings};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointResponse {
    #[default]
    Stop,
    Destroy,
}

// Registry names become file names, so path separators are never valid.
#[must_use]
pub fn is_valid_map_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
