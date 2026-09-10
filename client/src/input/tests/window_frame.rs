use std::ptr::NonNull;

use raw_window_handle::WaylandWindowHandle;

use super::*;

#[test]
fn desktop_positions_round_trip_between_display_scales() {
    let point = IVec2::new(-2400, 160);
    for saved_scale in [1.0, 1.5, 2.0] {
        for restored_scale in [1.0, 1.5, 2.0] {
            let physical = (point.as_vec2() * saved_scale).as_ivec2();
            let saved = PositionSpace::Logical
                .encode(physical, saved_scale)
                .expect("logical position missing");
            assert_eq!(saved, point);
            assert_eq!(
                PositionSpace::Logical.decode(saved, restored_scale),
                Some((point.as_vec2() * restored_scale).as_ivec2())
            );
            let saved = PositionSpace::Physical
                .encode(point, saved_scale)
                .expect("physical position missing");
            assert_eq!(PositionSpace::Physical.decode(saved, restored_scale), Some(point));
        }
    }
}

#[test]
fn wayland_ignores_saved_position_and_reveals_the_window() {
    let handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::dangling()));
    let mut window = Window {
        visible: false,
        resolution: (900, 600).into(),
        ..default()
    };
    let mut frame = WindowedFrame {
        position: Some(IVec2::new(400, 200)),
        size: UVec2::new(900, 600),
        position_pending: true,
    };
    update_frame(&mut window, &mut frame, PositionSpace::from_handle(handle));
    assert!(window.visible);
    assert_eq!(window.position, WindowPosition::Automatic);
    assert_eq!(frame.position, None);
    assert!(!frame.position_pending);
    assert_eq!(frame.size, UVec2::new(900, 600));
    window.resolution.set(1000.0, 700.0);
    update_frame(&mut window, &mut frame, PositionSpace::Compositor);
    assert_eq!(frame.size, UVec2::new(1000, 700));
}
