mod assets;
mod checkpoint_paint;
mod checkpoints;
mod erasers;
mod fade;
mod geometry;
mod kind_assets;
mod spawn;
mod surface;

pub(crate) use assets::{FieldMeshes, KindVisual, PaneVisual, field_pane_mesh};
pub use checkpoints::SharedCheckpoint;
pub(crate) use checkpoints::{
    CheckpointAssets, CheckpointMarker, checkpoint_pennants_system, checkpoints_spawn_system,
};
pub use erasers::EraserMarker;
pub(crate) use erasers::{EraserAssets, erasers_spawn_system};
pub(crate) use fade::{FieldSurface, FieldSurfaces, fade_target, fields_fade_system};
pub(crate) use geometry::{VisualField, merge_fields};
pub use kind_assets::{FieldAssets, build_field_assets};
pub(crate) use spawn::{spawn_field_visual, spawn_patterned_surface};
pub(crate) use surface::{clip_surface_rects, surface_frame_rects};
