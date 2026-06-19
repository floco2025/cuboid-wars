use bevy::prelude::*;

use super::{
    hud::{CrosshairMarker, FpsMarker, RttMarker},
    message_feed::spawn_message_feed_root,
    player_list::PlayerListMarker,
    quest_panel::QuestPanelMarker,
};
use crate::{config::ClientSettings, constants::HUD_EDGE_MARGIN_PX};

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
            row_gap: Val::Px(5.0),
            ..default()
        },
    ));

    // Quest panel: top-right, rows filled by `ui_quest_panel_rebuild_system`.
    // The rear-view mirror also lives top-right; when it's enabled, drop the
    // panel below it (mirror is `height_ratio` of window height tall, plus the
    // small edge inset) so the two don't overlap. Percent keeps the offset in
    // step with the ratio-sized mirror across window sizes.
    let rearview = &client_settings.camera.rearview;
    let quest_panel_top = if rearview.enabled {
        Val::Percent(rearview.height_ratio.mul_add(100.0, 4.0))
    } else {
        Val::Px(HUD_EDGE_MARGIN_PX)
    };
    commands.spawn((
        QuestPanelMarker,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(HUD_EDGE_MARGIN_PX),
            top: quest_panel_top,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        },
    ));

    let crosshair_size = 20.0;
    let crosshair_thickness = 2.0;
    let crosshair_color = Color::srgba(1.0, 1.0, 1.0, 0.8);

    commands
        .spawn((
            CrosshairMarker,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-crosshair_size / 2.0),
                    top: Val::Px(-crosshair_thickness / 2.0),
                    width: Val::Px(crosshair_size),
                    height: Val::Px(crosshair_thickness),
                    ..default()
                },
                BackgroundColor(crosshair_color),
            ));
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-crosshair_thickness / 2.0),
                    top: Val::Px(-crosshair_size / 2.0),
                    width: Val::Px(crosshair_thickness),
                    height: Val::Px(crosshair_size),
                    ..default()
                },
                BackgroundColor(crosshair_color),
            ));
        });

    commands.spawn((
        RttMarker,
        Text::new("RTT: --ms"),
        TextFont {
            font_size: FontSize::Px(hud_font_size),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            bottom: Val::Px(40.0),
            ..default()
        },
    ));

    commands.spawn((
        FpsMarker,
        Text::new("FPS: --"),
        TextFont {
            font_size: FontSize::Px(hud_font_size),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            bottom: Val::Px(10.0),
            ..default()
        },
    ));

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

    spawn_message_feed_root(&mut commands);
}
