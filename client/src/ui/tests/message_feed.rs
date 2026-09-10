use super::*;

#[test]
fn full_alpha_preserves_rgb() {
    let color = color_with_full_alpha(Color::srgba(0.2, 0.4, 0.6, 0.25)).to_srgba();

    assert_eq!((color.red, color.green, color.blue, color.alpha), (0.2, 0.4, 0.6, 1.0));
}
