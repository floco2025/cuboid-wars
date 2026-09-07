use bevy::{
    prelude::*,
    window::{PrimaryWindow, RawHandleWrapper, WindowMode, WindowPosition},
};
use raw_window_handle::RawWindowHandle;

#[derive(Resource, Clone, Copy)]
pub struct WindowedFrame {
    pub position: Option<IVec2>,
    pub size: UVec2,
    // macOS creation positions the content, while runtime placement positions the frame.
    pub position_pending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PositionSpace {
    Logical,
    Physical,
    Compositor,
}

impl PositionSpace {
    fn from_handle(handle: RawWindowHandle) -> Self {
        match handle {
            RawWindowHandle::AppKit(_) => Self::Logical,
            RawWindowHandle::Win32(_) | RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_) => Self::Physical,
            _ => Self::Compositor,
        }
    }

    fn encode(self, physical: IVec2, scale: f32) -> Option<IVec2> {
        match self {
            Self::Logical => Some((physical.as_vec2() / scale).round().as_ivec2()),
            Self::Physical => Some(physical),
            Self::Compositor => None,
        }
    }

    fn decode(self, saved: IVec2, scale: f32) -> Option<IVec2> {
        match self {
            Self::Logical => Some((saved.as_vec2() * scale).round().as_ivec2()),
            Self::Physical => Some(saved),
            Self::Compositor => None,
        }
    }
}

pub fn windowed_frame_system(
    mut windows: Query<(&mut Window, &RawHandleWrapper), With<PrimaryWindow>>,
    mut frame: ResMut<WindowedFrame>,
) {
    let Ok((mut window, handle)) = windows.single_mut() else {
        return;
    };
    update_frame(
        &mut window,
        &mut frame,
        PositionSpace::from_handle(handle.get_window_handle()),
    );
}

fn update_frame(window: &mut Window, frame: &mut WindowedFrame, space: PositionSpace) {
    if space == PositionSpace::Compositor {
        frame.position = None;
        frame.position_pending = false;
        window.visible = true;
    }
    if !matches!(window.mode, WindowMode::Windowed) {
        return;
    }
    let scale = window.resolution.scale_factor();
    if frame.position_pending {
        frame.position_pending = false;
        if let Some(physical) = frame.position.and_then(|saved| space.decode(saved, scale)) {
            window.position = WindowPosition::At(physical);
        }
        return;
    }
    window.visible = true;
    let size = window.size().round().as_uvec2();
    if !size.cmpgt(UVec2::ZERO).all() {
        return;
    }
    frame.size = size;
    if let WindowPosition::At(physical) = window.position {
        frame.position = space.encode(physical, scale);
    }
}

#[cfg(test)]
mod tests {
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
}
