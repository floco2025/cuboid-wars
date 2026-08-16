use bincode::{Decode, Encode};
use serde::{Deserialize, Deserializer, de};

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

    #[must_use]
    pub fn from_six(top: &str, bottom: &str, north: &str, south: &str, east: &str, west: &str) -> Self {
        Self {
            top: top.into(),
            bottom: bottom.into(),
            north: north.into(),
            south: south.into(),
            east: east.into(),
            west: west.into(),
        }
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
