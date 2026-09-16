use super::*;
use common::protocol::{CarrierId, HexColor, LightBridge, SwitchId};

#[test]
fn bridge_kinds_colour_their_rails_and_impacts() {
    let mut materials = Assets::default();
    let kinds = [
        KindDef {
            id: "blue".into(),
            color: HexColor([0, 0, 255]),
        },
        KindDef {
            id: "green".into(),
            color: HexColor([0, 255, 0]),
        },
    ];
    let layout = MapLayout {
        light_bridges: (0..2)
            .map(|index| LightBridge {
                id: BridgeId(index),
                kind: BridgeKindId(index as u16),
                switch: Some(SwitchId(0)),
                switch_inverted: false,
                x1: 2.0 * index as f32,
                z1: 0.0,
                x2: 2.0 * index as f32 + 2.0,
                z2: 2.0,
                y: 0.0,
                thickness: 0.1,
                level: 0,
                carrier: CarrierId::WORLD,
            })
            .collect(),
        ..Default::default()
    };
    let assets = build_bridge_assets(&mut materials, &kinds, &layout, 4.0);
    assert_eq!(assets.field_color(BridgeId(0)), Color::srgb(0.0, 0.0, 1.0));
    assert_eq!(assets.field_color(BridgeId(1)), Color::srgb(0.0, 1.0, 0.0));
    let frame = materials
        .get(&assets.kind(BridgeKindId(0)).frame)
        .expect("bridge frame material missing");
    assert_eq!(frame.alpha_mode, AlphaMode::Opaque);
    assert!(!frame.unlit);
    assert_eq!(frame.emissive, LinearRgba::rgb(0.0, 0.0, 4.0));
}
