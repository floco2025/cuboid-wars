use super::*;

#[test]
fn sensitivity_tracks_give_more_space_to_slower_speeds() {
    let mut world = World::new();
    for setting in [SliderSetting::MouseSensitivity, SliderSetting::ZoomSensitivity] {
        let entity = world.spawn(slider(120.0, setting, 0.125, 4.0, 1.0, 3)).id();
        let range = world.get::<SliderRange>(entity).expect("slider range missing");
        let value = world.get::<SliderValue>(entity).expect("slider value missing");
        assert!((range.thumb_position(value.0) - 0.6).abs() < 0.00001);
        let positions =
            [0.125, 0.25, 0.5, 1.0, 2.0, 4.0].map(|value| range.thumb_position(setting.slider_value(value)));
        for pair in positions.windows(2) {
            assert!((pair[1] - pair[0] - 0.2).abs() < 0.00001);
        }
    }
}
