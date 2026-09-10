use super::*;
use common::protocol::HexColor;

#[test]
fn bridges_use_translucent_panes_and_solid_frames() {
    let mut materials = Assets::default();
    let kinds = [KindDef {
        id: "blue".into(),
        color: HexColor([0, 0, 255]),
        switch: None,
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
