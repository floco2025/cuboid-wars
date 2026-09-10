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
#[path = "tests/window_frame.rs"]
mod tests;
