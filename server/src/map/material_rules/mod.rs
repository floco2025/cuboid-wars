// Per-segment material lookup, used during compile to populate the
// `MapLayout::*_materials` vectors. After compile, the client renders from
// those vectors and never queries this module.
//
// Each floor / wall / ramp record in `map.json` carries six face materials
// (top, bottom, north, south, east, west). On disk those six can be packed
// into an `"all"` shorthand plus per-face overrides; in memory `FaceMaterials`
// always holds six explicit strings.
//
// `MaterialRules` is a thin handle around three lookup tables (floors keyed by
// `(level, col, row)`, walls keyed by `(level, normalized edge)`, ramps keyed
// by `(lower_level, col, row)` for each cell in the footprint). Resolution is
// a direct field read.

mod grid;
mod loading;
mod query;

pub use query::MaterialRules;
