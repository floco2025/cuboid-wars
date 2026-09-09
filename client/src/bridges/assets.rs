use bevy::prelude::*;

use crate::{config::LightBridgeVfxConfig, fields::KindVisual, vfx::srgb_color};
use common::protocol::{BridgeKindId, KindDef};

// Sharing each kind's material keeps every bridge of that kind fading
// together. Indexed by `BridgeKindId`, in the map's kind order.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) kinds: Vec<KindVisual>,
}

impl BridgeAssets {
    pub fn material_for(&self, kind: BridgeKindId) -> &Handle<StandardMaterial> {
        &self.kinds[usize::from(kind.0)].surface
    }

    pub fn base_color(&self, kind: BridgeKindId) -> Color {
        self.kinds[usize::from(kind.0)].base_color
    }
}

pub fn build_bridge_assets(
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    config: LightBridgeVfxConfig,
) -> BridgeAssets {
    let kinds = kinds
        .iter()
        .map(|kind| {
            KindVisual::new(
                materials,
                srgb_color(kind.color),
                config.unpowered_opacity,
                config.emissive_brightness,
            )
        })
        .collect();
    BridgeAssets { kinds }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::HexColor;

    #[test]
    fn bridges_use_translucent_panes_and_solid_frames() {
        let mut materials = Assets::default();
        let kinds = [KindDef {
            id: "blue".into(),
            color: HexColor([0, 0, 255]),
            pressure_switch: Default::default(),
        }];
        let config = LightBridgeVfxConfig {
            emissive_brightness: 4.0,
            opacity: 0.8,
            unpowered_opacity: 0.3,
            fade_secs: 0.25,
        };
        let assets = build_bridge_assets(&mut materials, &kinds, config);
        let material = materials
            .get(assets.material_for(BridgeKindId(0)))
            .expect("bridge material missing");
        assert!(material.double_sided);
        assert_eq!(material.cull_mode, None);
        assert_eq!(material.alpha_mode, AlphaMode::Blend);
        assert_eq!(material.base_color.alpha(), config.unpowered_opacity);
        assert_eq!(material.emissive, LinearRgba::rgb(0.0, 0.0, config.emissive_brightness));
        let frame = materials
            .get(&assets.kinds[0].frame)
            .expect("bridge frame material missing");
        assert_eq!(frame.alpha_mode, AlphaMode::Opaque);
        assert!(frame.unlit);
        assert_eq!(frame.base_color, assets.base_color(BridgeKindId(0)));
    }
}
