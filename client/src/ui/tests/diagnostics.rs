use super::*;

#[test]
fn fps_label_includes_render_size() {
    assert_eq!(fps_label(59.6, UVec2::new(2560, 1440)), "FPS: 60 | 2560x1440");
}
