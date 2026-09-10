use super::*;

#[test]
fn large_window_caps_height_and_keeps_aspect() {
    assert_eq!(scene_image_size(UVec2::new(5120, 2880), 1440), UVec2::new(2560, 1440));
}

#[test]
fn window_below_the_cap_renders_native() {
    assert_eq!(scene_image_size(UVec2::new(1920, 1080), 1440), UVec2::new(1920, 1080));
}
