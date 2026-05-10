use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, de};

use super::{
    MaterialRules,
    layers::LayerNames,
    rules::{RuleSet, RuleSetDef},
};
use crate::map_geometry::MapGeometry;

const SUPPORTED_MAP_VERSION: u32 = 1;

impl<'de> Deserialize<'de> for MaterialRules {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MaterialRulesDef {
            #[serde(default)]
            grid_cols: Option<i32>,
            #[serde(default)]
            grid_rows: Option<i32>,
            #[serde(default, alias = "layer_names", alias = "level_names")]
            layers: LayerNamesDef,
            #[serde(rename = "material_rules")]
            rules: RuleSetDef,
        }

        let def = MaterialRulesDef::deserialize(deserializer)?;
        let rules = RuleSet::from_def(def.rules, &def.layers.0).map_err(de::Error::custom)?;
        let geometry = MapGeometry::new(def.grid_cols.unwrap_or(0), def.grid_rows.unwrap_or(0));
        Ok(Self { geometry, rules })
    }
}

#[derive(Debug, Clone, Default)]
struct LayerNamesDef(LayerNames);

impl<'de> Deserialize<'de> for LayerNamesDef {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Def {
            Ordered(Vec<String>),
            Named(BTreeMap<String, u8>),
        }

        let layers = match Def::deserialize(deserializer)? {
            Def::Ordered(names) => ordered_layer_names(names).map_err(de::Error::custom)?,
            Def::Named(names) => names,
        };
        Ok(Self(layers))
    }
}

fn ordered_layer_names(names: Vec<String>) -> std::result::Result<LayerNames, String> {
    let mut layers = LayerNames::new();
    for (idx, name) in names.into_iter().enumerate() {
        let level =
            u8::try_from(idx).map_err(|_| "material layer list cannot contain more than 256 layers".to_owned())?;
        if layers.insert(name.clone(), level).is_some() {
            return Err(format!("duplicate material layer name {name:?}"));
        }
    }
    Ok(layers)
}

#[derive(Deserialize)]
struct MapFile {
    version: u32,
    map: MapBody,
}

#[derive(Deserialize)]
struct MapBody {
    grid_cols: i32,
    grid_rows: i32,
    #[serde(default)]
    levels: Vec<MapLevel>,
    #[serde(default)]
    material_rules: Option<RuleSetDef>,
}

#[derive(Deserialize)]
struct MapLevel {
    #[serde(default)]
    name: Option<String>,
}

impl MaterialRules {
    pub fn load_default() -> Result<Self> {
        Self::load_from_map_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/server/map.json"
        )))
    }

    fn load_from_map_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let file: MapFile =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        anyhow::ensure!(
            file.version == SUPPORTED_MAP_VERSION,
            "unsupported map file version {} (expected {})",
            file.version,
            SUPPORTED_MAP_VERSION
        );
        let layers = layer_names_from_levels(&file.map.levels)?;
        let rules_def = file.map.material_rules.unwrap_or(RuleSetDef::Flat(Vec::new()));
        let rules = RuleSet::from_def(rules_def, &layers).map_err(anyhow::Error::msg)?;
        let geometry = MapGeometry::new(file.map.grid_cols, file.map.grid_rows);
        Ok(Self { geometry, rules })
    }
}

fn layer_names_from_levels(levels: &[MapLevel]) -> Result<LayerNames> {
    let mut layers = LayerNames::new();
    for (idx, level) in levels.iter().enumerate() {
        let Some(name) = &level.name else { continue };
        let layer = u8::try_from(idx).map_err(|_| anyhow::anyhow!("map cannot contain more than 256 levels"))?;
        if layers.insert(name.clone(), layer).is_some() {
            anyhow::bail!("duplicate level name {name:?}");
        }
    }
    Ok(layers)
}
