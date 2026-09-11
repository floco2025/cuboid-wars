use super::*;

fn rendering() -> RenderingConfig {
    RenderingConfig {
        opaque_renderer: OpaqueRenderer::Auto,
        directional_shadows: true,
        shadow_map_size: 2048,
        mipmaps: true,
        texture_anisotropy: 8,
        bloom: BloomConfig {
            enabled: true,
            intensity: 0.15,
            threshold: 2.5,
            threshold_softness: 0.4,
        },
    }
}

fn error_for(config: RenderingConfig) -> String {
    config
        .validate()
        .expect_err("invalid rendering config accepted")
        .to_string()
}

#[test]
fn invalid_rendering_controls_name_the_field() {
    for (edit, field) in [
        (
            (|c: &mut RenderingConfig| c.texture_anisotropy = 0) as fn(&mut RenderingConfig),
            "texture_anisotropy",
        ),
        (|c| c.shadow_map_size = 0, "shadow_map_size"),
        (|c| c.bloom.intensity = -0.1, "bloom.intensity"),
        (|c| c.bloom.threshold = f32::NAN, "bloom.threshold"),
        (|c| c.bloom.threshold_softness = -1.0, "bloom.threshold_softness"),
    ] {
        let mut config = rendering();
        edit(&mut config);
        let error = error_for(config);
        assert!(error.contains(&format!("rendering.{field}")), "{error}");
    }
}
