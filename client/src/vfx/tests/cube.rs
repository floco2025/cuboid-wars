use super::*;

#[test]
fn repeated_indices_allocate_one_cube_per_particle() {
    let indices = repeated_indices(3, CUBE_VERTICES.len(), &CUBE_INDICES);
    assert_eq!(indices.len(), CUBE_INDICES.len() * 3);
    assert_eq!(indices.iter().copied().max(), Some(71));
}

#[test]
fn smoothstep_hits_endpoints_and_midpoint() {
    assert_eq!(smoothstep(0.0), 0.0);
    assert_eq!(smoothstep(1.0), 1.0);
    assert_eq!(smoothstep(0.5), 0.5);
}
