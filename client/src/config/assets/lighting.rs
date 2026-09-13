use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WallLightModelDef {
    pub color: [f32; 3],
    pub flicker: bool,
    pub scene: String,
    pub scale: f32,
    pub offset_from_wall: f32,
    pub brightness: f32,
    pub range: f32,
    pub radius: f32,
    pub emissive_luminance: f32,
}
