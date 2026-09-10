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
fn zero_texture_anisotropy_is_rejected_by_path() {
    let mut config = rendering();
    config.texture_anisotropy = 0;
    assert!(error_for(config).contains("rendering.texture_anisotropy"));
}

#[test]
fn zero_shadow_map_size_is_rejected_by_path() {
    let mut config = rendering();
    config.shadow_map_size = 0;
    assert!(error_for(config).contains("rendering.shadow_map_size"));
}

#[test]
fn negative_bloom_intensity_is_rejected_by_path() {
    let mut config = rendering();
    config.bloom.intensity = -0.1;
    assert!(error_for(config).contains("rendering.bloom.intensity"));
}

#[test]
fn non_finite_bloom_threshold_is_rejected_by_path() {
    let mut config = rendering();
    config.bloom.threshold = f32::NAN;
    assert!(error_for(config).contains("rendering.bloom.threshold"));
}

#[test]
fn negative_bloom_threshold_softness_is_rejected_by_path() {
    let mut config = rendering();
    config.bloom.threshold_softness = -1.0;
    assert!(error_for(config).contains("rendering.bloom.threshold_softness"));
}
