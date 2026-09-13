use super::*;

#[test]
fn cover_repeats_across_positive_and_negative_texture_boundaries() {
    let period = TERRAIN_COVER_SIZE as f32;
    for position in [
        Vec2::new(-0.01, 11.0),
        Vec2::new(19.0, -0.01),
        Vec2::new(-213.5, -417.25),
    ] {
        let cover = TerrainCover::at(position);
        for offset in [Vec2::X * period, Vec2::Y * period, Vec2::splat(-period)] {
            let repeated = TerrainCover::at(position + offset);
            assert!((cover.soil - repeated.soil).abs() < 0.0001);
            assert!((cover.dry - repeated.dry).abs() < 0.0001);
            assert!((cover.shade - repeated.shade).abs() < 0.0001);
        }
    }
}

#[test]
fn mask_encodes_the_cover_used_for_grass_placement_as_linear_data() {
    let image = TerrainCover::image();
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
    let data = image.data.expect("terrain cover image data missing");
    for (x, z) in [(0, 0), (31, 127), (511, 511)] {
        let cover = TerrainCover::at(Vec2::new(x as f32 + 0.5, z as f32 + 0.5));
        let i = ((z * TERRAIN_COVER_SIZE + x) * 4) as usize;
        assert!((data[i] as f32 / 255.0 - cover.soil).abs() <= 0.5 / 255.0);
        assert!((data[i + 1] as f32 / 255.0 - cover.dry).abs() <= 0.5 / 255.0);
        assert!((data[i + 2] as f32 / 255.0 * 2.0 - cover.shade).abs() <= 1.0 / 255.0);
    }
}
