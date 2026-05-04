use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, de};

use super::{
    MaterialRules,
    layers::LayerNames,
    rules::{RuleSet, RuleSetDef},
};

const SUPPORTED_ASSET_VERSION: u32 = 1;

#[derive(Deserialize)]
struct MaterialRulesFile {
    version: u32,
    #[serde(flatten)]
    rules: MaterialRules,
}

impl<'de> Deserialize<'de> for MaterialRules {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MaterialRulesDef {
            #[serde(default, alias = "layer_names", alias = "level_names")]
            layers: LayerNamesDef,
            #[serde(rename = "material_rules")]
            rules: RuleSetDef,
        }

        let def = MaterialRulesDef::deserialize(deserializer)?;
        let rules = RuleSet::from_def(def.rules, &def.layers.0).map_err(de::Error::custom)?;
        Ok(Self { rules })
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

impl MaterialRules {
    pub fn load_default() -> Result<Self> {
        Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/client/assets.json"
        )))
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let file: MaterialRulesFile =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        anyhow::ensure!(
            file.version == SUPPORTED_ASSET_VERSION,
            "unsupported asset config version {} (expected {})",
            file.version,
            SUPPORTED_ASSET_VERSION
        );
        Ok(file.rules)
    }
}
