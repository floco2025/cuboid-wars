use bevy::{ecs::hierarchy::ChildSpawnerCommands, prelude::*};

use crate::{
    constants::{HEALTH_BAR_FILL_COLOR, HEALTH_BAR_TRACK_COLOR},
    markers::HealthBarFillMarker,
};
use common::{health::health_ratio, protocol::Health};

pub fn spawn_health_bar(
    parent: &mut ChildSpawnerCommands,
    tracked_entity: Entity,
    max_health: f32,
    current_health: f32,
    width: f32,
    height: f32,
) -> Entity {
    parent
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(height),
                justify_content: JustifyContent::FlexStart,
                ..default()
            },
            BackgroundColor(HEALTH_BAR_TRACK_COLOR),
        ))
        .with_children(|bar| {
            bar.spawn((
                HealthBarFillMarker {
                    tracked_entity,
                    max_health,
                },
                Node {
                    width: Val::Percent(health_ratio(Health(current_health), max_health) * 100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(HEALTH_BAR_FILL_COLOR),
            ));
        })
        .id()
}
