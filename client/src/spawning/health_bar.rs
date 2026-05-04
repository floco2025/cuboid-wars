use bevy::{ecs::hierarchy::ChildSpawnerCommands, prelude::*};

use crate::constants::{HEALTH_BAR_FILL_COLOR, HEALTH_BAR_TRACK_COLOR};
use common::{health::health_ratio, protocol::Health};

// Lives on the inner fill node of a health bar UI tree. Tracks the entity
// whose `Health` drives the fill width plus the maximum health (constant
// per-character, captured at spawn so the update system doesn't need to
// look it up every frame).
#[derive(Component)]
pub struct HealthBarFill {
    pub tracked_entity: Entity,
    pub max_health: f32,
}

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
                HealthBarFill {
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
