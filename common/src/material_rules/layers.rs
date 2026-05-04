use std::collections::BTreeMap;

use serde::Deserialize;

pub(super) type LayerNames = BTreeMap<String, u8>;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum LayerRef {
    Number(u8),
    Name(String),
}

impl LayerRef {
    fn resolve(&self, layer_names: &LayerNames) -> Result<u8, String> {
        match self {
            Self::Number(level) => Ok(*level),
            Self::Name(name) => layer_names
                .get(name)
                .copied()
                .ok_or_else(|| format!("unknown material layer {name:?}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum LayerRefs {
    One(LayerRef),
    Many(Vec<LayerRef>),
}

impl LayerRefs {
    fn resolve(&self, layer_names: &LayerNames) -> Result<Vec<u8>, String> {
        match self {
            Self::One(level) => Ok(vec![level.resolve(layer_names)?]),
            Self::Many(levels) => levels
                .iter()
                .map(|level| level.resolve(layer_names))
                .collect::<Result<Vec<_>, _>>(),
        }
    }
}

pub(super) fn resolve_level_scope(
    levels: Option<&LayerRefs>,
    layer_names: &LayerNames,
) -> Result<Option<Vec<u8>>, String> {
    levels.map(|levels| levels.resolve(layer_names)).transpose()
}
