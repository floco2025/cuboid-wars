use super::*;
use crate::config::BarrierPulseVfxConfig;
use common::protocol::HexColor;

#[test]
fn barriers_are_translucent_and_keys_use_solid_glowing_symbols() {
    let mut meshes = Assets::default();
    let mut materials = Assets::default();
    let kinds = [KindDef {
        id: "red".into(),
        color: HexColor([255, 0, 0]),
        switch: None,
    }];
    let config = BarrierVfxConfig {
        emissive_brightness: 7.0,
        opacity: 0.25,
        pulse: BarrierPulseVfxConfig {
            min_opacity: 0.1,
            frequency_hz: 0.5,
        },
    };
    let assets = build_barrier_assets(&mut meshes, &mut materials, &kinds, config, 3.0);
    let key_mesh = meshes.get(assets.key_mesh()).expect("key mesh missing");
    let positions = key_mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
        .expect("key mesh positions missing");
    assert!(positions.iter().all(|p| {
        p[0].abs() <= ITEM_KEY_SIZE / 2.0 && p[1].abs() <= ITEM_KEY_SIZE / 2.0 && p[2].abs() == ITEM_KEY_DEPTH / 2.0
    }));

    let key_material = materials
        .get(assets.key_material_for(BarrierKindId(0)))
        .expect("key material missing");
    assert_eq!(key_material.alpha_mode, AlphaMode::Opaque);
    assert_eq!(key_material.emissive, LinearRgba::rgb(3.0, 0.0, 0.0));

    let material = materials
        .get(assets.material_for(BarrierKindId(0)))
        .expect("barrier material missing");
    assert!(material.double_sided);
    assert_eq!(material.cull_mode, None);
    assert_eq!(material.alpha_mode, AlphaMode::Blend);
    assert_eq!(material.base_color.alpha(), config.opacity);
    assert_eq!(material.emissive, LinearRgba::rgb(config.emissive_brightness, 0.0, 0.0));
}
