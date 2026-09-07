mod assets;
mod geometry;
mod spawn;

pub(crate) use assets::{FieldMaterials, FieldMeshes};
pub(crate) use geometry::{VisualField, merge_fields};
pub(crate) use spawn::{spawn_field_visual, spawn_framed_surface};
