use super::super::ZoneVolume;
use bevy_math::Vec3;

#[test]
fn extension_measures_the_shortest_3d_distance_to_the_authored_box() {
    let volume = ZoneVolume {
        min: Vec3::ZERO,
        max: Vec3::new(10.0, 8.0, 6.0),
    };
    assert!(volume.contains(Vec3::new(5.0, 7.0, 3.0), 0.0));
    assert!(volume.contains(volume.max, 0.0));
    assert!(!volume.contains(volume.max + Vec3::Y * 0.01, 0.0));
    for direction in [Vec3::X, Vec3::Y, Vec3::Z] {
        assert!(volume.contains(volume.max + direction * 5.0, 5.0));
        assert!(!volume.contains(volume.max + direction * 5.01, 5.0));
    }
    assert!(volume.contains(volume.max + Vec3::new(3.0, 4.0, 0.0), 5.0));
    assert!(!volume.contains(volume.max + Vec3::new(4.0, 4.0, 0.0), 5.0));
}
