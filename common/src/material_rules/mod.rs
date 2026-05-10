// Per-segment material lookup.
//
// Each floor / wall / ramp record in `map.json` carries six face materials
// (top, bottom, north, south, east, west). On disk those six can be packed
// into an `"all"` shorthand plus per-face overrides; in memory `FaceMaterials`
// always holds six explicit strings.
//
// `MaterialRules` is a thin handle around three lookup tables (floors keyed by
// `(level, col, row)`, walls keyed by `(level, normalized edge)`, ramps keyed
// by `(lower_level, col, row)` for each cell in the footprint) plus the
// item materials map. Resolution is a direct field read.

mod face_materials;
mod grid;
mod loading;
mod query;

pub use face_materials::FaceMaterials;
pub use query::MaterialRules;
