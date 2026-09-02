use bevy::prelude::*;

use super::{
    console::spawn_console,
    crosshair::CrosshairMarker,
    diagnostics::{DiagnosticsColumnMarker, FpsMarker, RttMarker},
    hud_banner::spawn_hud_banner,
    message_feed::spawn_message_feed,
    player_list::PlayerListMarker,
    quest_panel::QuestPanelMarker,
};
use crate::{
    config::ClientSettings,
    constants::{HUD_EDGE_MARGIN_PX, HUD_ROW_GAP_PX},
};

// Marker for the death overlay — a red translucent full-screen panel shown
// while the local player is dead.
#[derive(Component)]
pub struct DeathOverlayMarker;

pub fn setup_ui_system(mut commands: Commands, client_settings: Res<ClientSettings>) {
    let hud_font_size = client_settings.hud.font_sizes.player_list;
    commands.spawn((
        PlayerListMarker,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(HUD_EDGE_MARGIN_PX),
            top: Val::Px(HUD_EDGE_MARGIN_PX),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(HUD_ROW_GAP_PX),
            ..default()
        },
    ));

    // Quest panel: top-right, rows filled by `ui_quest_panel_rebuild_system`.
    let quest_panel_top = quest_panel_top(&client_settings);
    commands.spawn((
        QuestPanelMarker,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(HUD_EDGE_MARGIN_PX),
            top: quest_panel_top,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(HUD_ROW_GAP_PX),
            ..default()
        },
    ));

    commands.spawn((
        CrosshairMarker,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            width: Val::Px(0.0),
            height: Val::Px(0.0),
            ..default()
        },
    ));

    // RTT above FPS in one auto-stacking column, so the rows can't overlap
    // at any font size.
    commands
        .spawn((
            DiagnosticsColumnMarker,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(HUD_EDGE_MARGIN_PX),
                bottom: Val::Px(HUD_EDGE_MARGIN_PX),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(HUD_ROW_GAP_PX),
                ..default()
            },
        ))
        .with_children(|column| {
            column.spawn((
                RttMarker,
                Text::new("RTT: --ms"),
                TextFont {
                    font_size: FontSize::Px(hud_font_size),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            column.spawn((
                FpsMarker,
                Text::new("FPS: -- | --x--"),
                TextFont {
                    font_size: FontSize::Px(hud_font_size),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });

    commands.spawn((
        DeathOverlayMarker,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 0.0, 0.0, 0.0)),
        Visibility::Hidden,
    ));

    spawn_hud_banner(&mut commands, &client_settings);

    // Feed rows above the console prompt in one bottom-right column, so the
    // two can't overlap at any font size.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(HUD_EDGE_MARGIN_PX),
            bottom: Val::Px(HUD_EDGE_MARGIN_PX),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(HUD_ROW_GAP_PX),
            ..default()
        })
        .with_children(|column| {
            spawn_message_feed(column, &client_settings);
            spawn_console(column, &client_settings);
        });
}

// The rear-view mirror also lives top-right; with it enabled the quest
// panel drops below it (mirror is `height_ratio` of window height tall,
// plus the small edge inset) so the two don't overlap. Percent keeps the
// offset in step with the ratio-sized mirror across window sizes.
fn quest_panel_top(client_settings: &ClientSettings) -> Val {
    let rearview = &client_settings.camera.rearview;
    if rearview.enabled {
        Val::Percent(rearview.height_ratio.mul_add(100.0, 4.0))
    } else {
        Val::Px(HUD_EDGE_MARGIN_PX)
    }
}

// Re-applies the offset when the settings menu toggles the mirror.
pub fn ui_quest_panel_offset_system(
    client_settings: Res<ClientSettings>,
    mut panel: Single<&mut Node, With<QuestPanelMarker>>,
) {
    if !client_settings.is_changed() {
        return;
    }
    let top = quest_panel_top(&client_settings);
    if panel.top != top {
        panel.top = top;
    }
}
