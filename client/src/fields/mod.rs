mod assets;
mod erasers;
mod geometry;
mod spawn;
mod surface;

pub(crate) use assets::{FieldMeshes, KindVisual};
pub use erasers::EraserMarker;
pub(crate) use erasers::{EraserAssets, erasers_spawn_system};
pub(crate) use geometry::{VisualField, merge_fields};
pub(crate) use spawn::{spawn_field_visual, spawn_framed_surface};
pub(crate) use surface::{clip_surface_rects, surface_frame_rects};
