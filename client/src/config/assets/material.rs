use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct MaterialBinding {
    pub(super) material: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaterialDef {
    pub(crate) textures: TextureDef,
    #[serde(default)]
    pub tile_size: Option<f32>,
    pub metallic: f32,
    #[serde(rename = "roughness")]
    pub perceptual_roughness: f32,
    #[serde(default)]
    pub(crate) repeat: bool,
    #[serde(default)]
    pub(crate) linear_data_textures: bool,
}

impl MaterialDef {
    #[must_use]
    pub fn tile_size(&self) -> f32 {
        self.tile_size.unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TextureDef {
    pub(crate) base_color: String,
    pub(crate) normal: String,
    pub(crate) occlusion: String,
    pub(crate) metallic_roughness: String,
}

impl TextureDef {
    // Whether the normal map is DirectX (Y down) or OpenGL (Y up) convention,
    // read from the `-dx` / `-gl` suffix the texture packs carry; `None` when
    // the name says neither.
    pub(crate) fn normal_is_directx(&self) -> Option<bool> {
        let name = self.normal.to_ascii_lowercase();
        if name.contains("normal-dx") {
            Some(true)
        } else if name.contains("normal-gl") {
            Some(false)
        } else {
            None
        }
    }
}
