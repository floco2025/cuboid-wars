use super::*;

#[test]
fn cover_toward_the_sun_stays_in_range_and_grows_with_coverage() {
    let directions = [
        Vec3::new(0.3, 0.8, 0.5).normalize(),
        Vec3::new(-0.7, 0.2, 0.1).normalize(),
        Vec3::new(0.0, 0.05, 1.0).normalize(),
    ];
    for direction in directions {
        let mut previous = 0.0;
        for step in 0..=10 {
            let coverage = step as f32 / 10.0;
            let density = cumulus_toward(direction, 123.4, coverage, 2.2, 0.0175);
            assert!((0.0..=1.0).contains(&density));
            assert!(density + 1e-6 >= previous, "coverage {coverage} thinned the cloud");
            previous = density;
        }
    }
}
