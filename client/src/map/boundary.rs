use bevy::prelude::*;
use common::{
    map::BoundaryTimer,
    protocol::{MapLayout, PlayerGeneration, Position},
};

use crate::{
    config::ClientSettings,
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};

#[derive(Component)]
pub(super) struct BoundaryNotice;

pub(super) fn boundary_notice_system(
    mut commands: Commands,
    time: Res<Time>,
    layout: Res<MapLayout>,
    settings: Res<ClientSettings>,
    local: Res<LocalPlayerInfo>,
    players: Res<PlayerMap>,
    my_id: Res<MyPlayerId>,
    player: Query<&Position, With<LocalPlayerMarker>>,
    mut notice: Query<(&mut Text, &mut Visibility), With<BoundaryNotice>>,
    mut timer: Local<BoundaryTimer>,
    mut generation: Local<Option<PlayerGeneration>>,
) {
    let Some(grounds) = &layout.grounds else { return };
    if notice.is_empty() {
        commands.spawn((
            BoundaryNotice,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(settings.hud.font_sizes.banner),
                ..default()
            },
            TextColor(Color::WHITE),
            BackgroundColor(Color::srgba(0.08, 0.1, 0.12, 0.85)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(24.0),
                align_self: AlignSelf::Center,
                justify_self: JustifySelf::Center,
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            Visibility::Hidden,
        ));
        return;
    }
    let body = player.single().ok().filter(|_| !local.is_dead);
    let current = players.get(&my_id.0).map(|player| player.generation);
    if current != *generation {
        timer.0 = 0.0;
        *generation = current;
    }
    let outside = body.is_some_and(|pos| grounds.outside_boundary((*pos).into()));
    let remaining = timer.tick(outside, time.delta_secs(), grounds.settings.return_secs);
    for (mut text, mut visibility) in &mut notice {
        *visibility = if remaining.is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if let Some(secs) = remaining {
            text.0 = if secs > 0.0 {
                format!("Return to the grounds — returning in {}s", secs.ceil() as u32)
            } else {
                "Returning to your checkpoint or spawn…".into()
            };
        }
    }
}
