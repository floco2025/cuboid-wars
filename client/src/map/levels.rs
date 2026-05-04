pub(crate) fn visual_focus_level(y: f32) -> u8 {
    if y <= 0.0 {
        return 0;
    }
    (y / common::constants::LEVEL_HEIGHT).round().min(f32::from(u8::MAX)) as u8
}
