mod assets;
mod checkpoint_paint;
mod checkpoints;
mod erasers;
mod fade;
mod field_assets;
mod geometry;
mod spawn;
mod surface;

pub(crate) use assets::{FieldMeshes, FieldVisual, PaneVisual, field_pane_mesh};
pub use checkpoints::SharedCheckpoint;
pub(crate) use checkpoints::{
    CheckpointAssets, CheckpointMarker, checkpoint_pennants_system, checkpoints_spawn_system,
};
pub use erasers::EraserMarker;
pub(crate) use erasers::{EraserAssets, erasers_spawn_system};
pub(crate) use fade::{FieldPiece, FieldSurface, FieldSurfaces, fade_target, fields_fade_system};
pub use field_assets::{FieldAssets, build_field_assets};
pub(crate) use geometry::{VisualField, merge_fields};
pub(crate) use spawn::{spawn_field_visual, spawn_patterned_surface};
pub(crate) use surface::{clip_surface_rects, surface_frame_rects};
