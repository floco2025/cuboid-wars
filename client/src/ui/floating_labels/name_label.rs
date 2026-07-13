use bevy::{prelude::*, ui::UiScale};

use crate::{
    config::ClientSettings,
    constants::{LABEL_RENDER_FRAMES, LABEL_TEXT_PADDING_X, LABEL_TEXT_PADDING_Y},
    ui::floating_labels::{
        LabelCamera,
        spawn::{FloatingLabelPaddingMarker, FloatingLabelTextMarker},
    },
};

// Render each player's name label texture for the few frames after spawn
// (`render_ttl` primed at spawn), then idle forever. The name never changes,
// so a one-time render is enough; the multi-frame window (not a single frame)
// makes that render land reliably regardless of frame timing. Health bars are
// separate geometry now, so this no longer reacts to health at all.
pub fn player_name_label_render_system(mut characters: Query<&mut LabelCamera>, mut cameras: Query<&mut Camera>) {
    for mut label_cam in &mut characters {
        let active = label_cam.render_ttl > 0;
        if let Ok(mut cam) = cameras.get_mut(label_cam.camera)
            && cam.is_active != active
        {
            cam.is_active = active;
        }
        if label_cam.render_ttl > 0 {
            label_cam.render_ttl -= 1;
        }
    }
}

// The name label renders into a fixed-size texture, but the global HUD
// `UiScale` multiplies layout for render-target UI too (Bevy applies it to
// every UI root regardless of target camera). Divide the label's font size
// and padding by the scale so the baked texture stays pixel-identical at any
// window size, and re-arm the label cameras to re-render it. Also
// compensates labels spawned after a resize (`Added` markers), so
// `spawn_player` needs no knowledge of the scale.
pub fn floating_label_scale_compensation_system(
    ui_scale: Res<UiScale>,
    client_settings: Res<ClientSettings>,
    mut texts: Query<(&mut TextFont, Ref<FloatingLabelTextMarker>)>,
    mut paddings: Query<(&mut Node, Ref<FloatingLabelPaddingMarker>)>,
    mut label_cameras: Query<&mut LabelCamera>,
) {
    let scale = ui_scale.0;
    assert!(scale > 0.0, "HUD UiScale is not positive despite the scale floor");
    let rescale_all = ui_scale.is_changed();

    // Always derive from the config base value, never the current component
    // value — no cumulative drift across repeated rescales.
    let font_size = client_settings.hud.font_sizes.floating_label / scale;
    for (mut font, marker) in &mut texts {
        if rescale_all || marker.is_added() {
            font.font_size = FontSize::Px(font_size);
        }
    }
    let padding = UiRect::axes(
        Val::Px(LABEL_TEXT_PADDING_X / scale),
        Val::Px(LABEL_TEXT_PADDING_Y / scale),
    );
    for (mut node, marker) in &mut paddings {
        if rescale_all || marker.is_added() {
            node.padding = padding;
        }
    }

    // Freshly spawned labels arrive with a primed ttl; only a scale change
    // needs existing textures re-rendered.
    if rescale_all {
        for mut label_camera in &mut label_cameras {
            label_camera.render_ttl = LABEL_RENDER_FRAMES;
        }
    }
}
