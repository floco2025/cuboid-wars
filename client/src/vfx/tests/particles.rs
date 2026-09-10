use super::*;

fn cloud() -> ParticleCloud {
    ParticleCloud::new("test", Handle::default())
}

fn particle() -> ParticleSpawn {
    ParticleSpawn {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        acceleration: Vec3::ZERO,
        start_size: 1.0,
        end_size: 0.0,
        stretch: Vec3::ONE,
        fades: true,
        lifetime: 1.0,
        color: Vec3::ONE,
    }
}

fn spike(cloud: &mut ParticleCloud, count: usize) {
    for _ in 0..count {
        cloud.spawn(particle());
    }
}

#[test]
fn cloud_grows_immediately_on_spike() {
    let mut cloud = cloud();
    spike(&mut cloud, 100);

    assert_eq!(cloud.particles.len(), 100, "spawns past capacity must not be dropped");
    assert_eq!(cloud.advance(0.0), Some(128), "100 live grows to the next power of two");
    assert_eq!(cloud.advance(0.0), None, "no repeat resize while the load is steady");
}

#[test]
fn recent_peak_holds_capacity() {
    let mut cloud = cloud();
    spike(&mut cloud, 100);
    cloud.advance(0.0);

    // All particles expire, but the spike is still inside the peak
    // window — capacity must not drop yet.
    assert_eq!(cloud.advance(1.0), None);
    assert!(cloud.particles.is_empty());
    assert_eq!(cloud.capacity, 128);
}

#[test]
fn cloud_shrinks_after_spike_leaves_the_window() {
    let mut cloud = cloud();
    spike(&mut cloud, 100);
    cloud.advance(0.0);
    cloud.advance(1.0);

    assert_eq!(
        cloud.advance(SHRINK_WINDOW_SECS),
        None,
        "spike still in the previous window"
    );
    assert_eq!(
        cloud.advance(SHRINK_WINDOW_SECS),
        Some(MIN_CAPACITY),
        "both windows past the spike release the capacity"
    );
}

#[test]
fn particle_mesh_keeps_fixed_capacity() {
    let mesh = particle_mesh(2);

    assert_eq!(mesh.count_vertices(), 2 * CUBE_VERTICES.len());
    assert_eq!(mesh.indices().map(Indices::len), Some(2 * CUBE_INDICES.len()));
}
