use bevy::prelude::*;

use crate::markers::*;

pub fn setup_ui_system(mut commands: Commands) {
    commands.spawn((
        PlayerListMarker,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(10.0),
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
            font_size: 20.0,
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
            font_size: 20.0,
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
        BumpFlashMarker,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
        Visibility::Hidden,
    ));
}
