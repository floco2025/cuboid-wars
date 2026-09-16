use super::*;

#[test]
fn field_material_keeps_the_translucent_base_and_carries_the_tuning() {
    let material = field_material(Color::srgb(1.0, 0.0, 0.0), 0.4, 2.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
    assert!(material.base.double_sided);
    assert_eq!(material.base.cull_mode, None);
    assert_eq!(material.base.base_color.alpha(), 0.4);
    assert_eq!(material.base.emissive, LinearRgba::rgb(2.0, 0.0, 0.0));
    assert_eq!(
        material.extension.pattern,
        Vec4::new(
            FIELD_HEX_CELL_SIZE,
            FIELD_HEX_LINE_WIDTH,
            FIELD_HEX_LINE_GLOW,
            FIELD_FRESNEL_POWER
        )
    );
    assert_eq!(
        material.extension.shape.xy(),
        Vec2::new(FIELD_FACE_OPACITY_RATIO, FIELD_EDGE_GLOW)
    );
}
