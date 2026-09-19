use super::*;
use crate::constants::FIELD_FRAME_BODY_TINT;
use common::protocol::HexColor;

#[test]
fn barrier_kinds_have_glowing_rails_and_solid_glowing_key_symbols() {
    let mut meshes = Assets::default();
    let mut materials = Assets::default();
    let kinds = [KindDef {
        id: "red".into(),
        color: HexColor([255, 0, 0]),
    }];
    let assets = build_barrier_assets(&mut meshes, &mut materials, &kinds, &MapLayout::default(), 2.0, 3.0);
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

    let frame = materials
        .get(&assets.kind(BarrierKindId(0)).frame)
        .expect("frame material missing");
    assert_eq!(frame.alpha_mode, AlphaMode::Opaque);
    assert!(!frame.unlit);
    assert_eq!(
        frame.base_color.to_linear(),
        LinearRgba::rgb(FIELD_FRAME_BODY_TINT, 0.0, 0.0)
    );
    assert_eq!(frame.emissive, LinearRgba::rgb(2.0, 0.0, 0.0));
    assert_eq!(assets.base_color(BarrierKindId(0)), Color::srgb(1.0, 0.0, 0.0));
}
