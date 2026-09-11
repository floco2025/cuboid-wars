use super::*;
use common::protocol::{BridgeKindId, CarrierId, HexColor, LightBridge};

#[test]
fn bridges_use_translucent_panes_and_solid_frames() {
    let mut materials = Assets::default();
    let kinds = [KindDef {
        id: "blue".into(),
        color: HexColor([0, 0, 255]),
    }];
    let config = LightBridgeVfxConfig {
        emissive_brightness: 4.0,
        opacity: 0.8,
        unpowered_opacity: 0.3,
        fade_secs: 0.25,
    };
    let layout = MapLayout {
        light_bridges: (0..2)
            .map(|index| LightBridge {
                id: BridgeId(index),
                kind: BridgeKindId(0),
                switch: None,
                switch_inverted: false,
                x1: 0.0,
                z1: 0.0,
                x2: 2.0,
                z2: 2.0,
                y: 0.0,
                thickness: 0.1,
                level: 0,
                carrier: CarrierId::WORLD,
            })
            .collect(),
        ..Default::default()
    };
    let assets = build_bridge_assets(&mut materials, &kinds, &layout, config);
    assert_ne!(assets.bridges[0].surface, assets.bridges[1].surface);
    let material = materials
        .get(&assets.bridges[0].surface)
        .expect("bridge material missing");
    assert!(material.double_sided);
    assert_eq!(material.cull_mode, None);
    assert_eq!(material.alpha_mode, AlphaMode::Blend);
    assert_eq!(material.base_color.alpha(), config.unpowered_opacity);
    assert_eq!(material.emissive, LinearRgba::rgb(0.0, 0.0, config.emissive_brightness));
    let frame = materials
        .get(&assets.bridges[0].frame)
        .expect("bridge frame material missing");
    assert_eq!(frame.alpha_mode, AlphaMode::Opaque);
    assert!(frame.unlit);
    assert_eq!(frame.base_color, assets.field_color(BridgeId(0)));
}
