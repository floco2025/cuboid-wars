use std::{fs, path::Path};

use anyhow::{Context, Result};

use super::{
    schema::{MapDef, MapFile},
    validation::{canonicalize, validate_map},
};

pub(crate) fn load_map(path: &Path) -> Result<MapDef> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let mut file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    validate_map(&file.map).with_context(|| format!("validating map at {}", path.display()))?;
    canonicalize(&mut file.map);
    Ok(file.map)
}
