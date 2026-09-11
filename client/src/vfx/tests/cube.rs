use super::*;

#[test]
fn repeated_indices_allocate_one_cube_per_particle() {
    let indices = repeated_indices(3, CUBE_VERTICES.len(), &CUBE_INDICES);
    assert_eq!(indices.len(), CUBE_INDICES.len() * 3);
    assert_eq!(indices.iter().copied().max(), Some(71));
}
