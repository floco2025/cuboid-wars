use super::*;

#[test]
fn cover_is_deterministic_but_has_no_old_map_sized_repeat() {
    for position in [
        Vec2::new(-0.01, 11.0),
        Vec2::new(19.0, -0.01),
        Vec2::new(-213.5, -417.25),
    ] {
        let cover = TerrainCover::at(position);
        let same = TerrainCover::at(position);
        assert_eq!(cover.soil, same.soil);
        assert_eq!(cover.dry, same.dry);
        assert_eq!(cover.shade, same.shade);
        let shifted = TerrainCover::at(position + Vec2::splat(512.0));
        assert!(
            (cover.soil - shifted.soil).abs() > 0.0001
                || (cover.dry - shifted.dry).abs() > 0.0001
                || (cover.shade - shifted.shade).abs() > 0.0001
        );
    }
}

#[test]
fn brown_soil_is_bare_and_green_density_varies() {
    let mut green_densities = Vec::new();
    let mut found_soil = false;
    for z in -100..100 {
        for x in -100..100 {
            let position = Vec2::new(x as f32 * 1.7, z as f32 * 1.7);
            let cover = TerrainCover::at(position);
            let density = cover.grass_density(position);
            assert!((0.0..=1.0).contains(&density));
            if cover.soil >= 0.28 {
                found_soil = true;
                assert_eq!(density, 0.0);
            } else if cover.soil <= 0.04 {
                green_densities.push(density);
            }
        }
    }
    assert!(found_soil);
    let min = green_densities.iter().copied().fold(f32::INFINITY, f32::min);
    let max = green_densities.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(max - min > 0.3);
}
