use super::*;

#[test]
fn ease_blend_is_zero_for_no_time_and_approaches_one() {
    assert_eq!(ease_blend(0.0, 0.5), 0.0);
    let one_tau = ease_blend(0.5, 0.5);
    assert!((one_tau - (1.0 - (-1.0f32).exp())).abs() < 1e-6);
    assert!(ease_blend(50.0, 0.5) > 0.999_99);
}

#[test]
fn color_with_alpha_keeps_linear_channels_and_sets_alpha() {
    let color = Color::srgb(0.5, 0.25, 1.0);
    let linear = color.to_linear();
    let result = color_with_alpha(color, 0.3);
    assert_eq!(result, Color::srgba(linear.red, linear.green, linear.blue, 0.3));
    assert_eq!(result.alpha(), 0.3);
}
