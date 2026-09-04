use bevy::prelude::*;
use common::protocol::HexColor;

// Lit translucent material for a colour-keyed kind (barriers, light bridges).
// Lit because emissive is ignored on unlit materials; this Blend permutation
// only renders on a mesh with white vertex colours (`with_white_vertex_colors`).
// Callers move nothing but `base_color.alpha` afterwards (`color_with_alpha`).
pub(crate) fn translucent_kind_material(color: Color, alpha: f32, emissive: f32) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: color_with_alpha(color, alpha),
        emissive: LinearRgba::rgb(linear.red * emissive, linear.green * emissive, linear.blue * emissive),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

// Linear channels go into `srgba` on purpose: the shipped kind colours are
// seen through this mapping, so "fixing" it re-tints every barrier and bridge.
pub(crate) fn color_with_alpha(color: Color, alpha: f32) -> Color {
    let linear = color.to_linear();
    Color::srgba(linear.red, linear.green, linear.blue, alpha)
}

// Fraction of the remaining distance to cover this frame for a frame-rate
// independent exponential approach with time constant `tau_secs`.
pub(crate) fn ease_blend(delta_secs: f32, tau_secs: f32) -> f32 {
    1.0 - (-delta_secs / tau_secs).exp()
}

pub(crate) fn srgb_color(color: HexColor) -> Color {
    let [r, g, b] = color.0;
    Color::srgb_u8(r, g, b)
}

#[cfg(test)]
mod tests {
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
}
