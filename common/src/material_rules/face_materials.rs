use std::collections::BTreeMap;

use bincode::{Decode, Encode};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FaceMaterials {
    pub north: String,
    pub south: String,
    pub east: String,
    pub west: String,
    pub top: String,
    pub bottom: String,
}

impl FaceMaterials {
    #[must_use]
    pub fn uniform(material: impl Into<String>) -> Self {
        let material = material.into();
        Self {
            north: material.clone(),
            south: material.clone(),
            east: material.clone(),
            west: material.clone(),
            top: material.clone(),
            bottom: material,
        }
    }

    #[must_use]
    pub fn is_uniform(&self) -> bool {
        self.north == self.south
            && self.north == self.east
            && self.north == self.west
            && self.north == self.top
            && self.north == self.bottom
    }

    #[must_use]
    pub fn primary(&self) -> &str {
        &self.top
    }

    fn faces_array(&self) -> [(&'static str, &str); 6] {
        [
            ("top", &self.top),
            ("bottom", &self.bottom),
            ("north", &self.north),
            ("south", &self.south),
            ("east", &self.east),
            ("west", &self.west),
        ]
    }
}

// On-disk shape uses an `all` shorthand: faces that match `all` are omitted.
// The struct itself always carries six explicit strings.
#[derive(Deserialize)]
struct FaceMaterialsDef {
    #[serde(default)]
    all: Option<String>,
    #[serde(default)]
    top: Option<String>,
    #[serde(default)]
    bottom: Option<String>,
    #[serde(default)]
    north: Option<String>,
    #[serde(default)]
    south: Option<String>,
    #[serde(default)]
    east: Option<String>,
    #[serde(default)]
    west: Option<String>,
}

impl<'de> Deserialize<'de> for FaceMaterials {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let def = FaceMaterialsDef::deserialize(deserializer)?;
        let pick = |face: Option<String>| -> Result<String, D::Error> {
            face.or_else(|| def.all.clone())
                .ok_or_else(|| de::Error::custom("missing material; provide `all` or per-face values"))
        };
        Ok(Self {
            top: pick(def.top)?,
            bottom: pick(def.bottom)?,
            north: pick(def.north)?,
            south: pick(def.south)?,
            east: pick(def.east)?,
            west: pick(def.west)?,
        })
    }
}

impl Serialize for FaceMaterials {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Pick the most common face value as `all`; ties broken alphabetically
        // so the output is deterministic.
        let faces = self.faces_array();
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for (_, value) in faces {
            *counts.entry(value).or_default() += 1;
        }
        let (most_common, max_count) = counts
            .iter()
            .max_by_key(|(name, count)| (**count, std::cmp::Reverse(*name)))
            .map(|(name, count)| ((*name).to_owned(), *count))
            .expect("FaceMaterials always has six faces");

        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        if max_count >= 2 {
            map.serialize_entry("all", &most_common)?;
            for (face, value) in faces {
                if value != most_common {
                    map.serialize_entry(face, value)?;
                }
            }
        } else {
            for (face, value) in faces {
                map.serialize_entry(face, value)?;
            }
        }
        map.end()
    }
}
