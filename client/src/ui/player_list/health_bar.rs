use bevy::{ecs::hierarchy::ChildSpawnerCommands, prelude::*};
use common::{health::health_ratio, protocol::Health};

use crate::constants::{HEALTH_BAR_FILL_COLOR, HEALTH_BAR_TRACK_COLOR};

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

pub fn ui_health_bar_fill_system(health_query: Query<&Health>, mut bar_query: Query<(&HealthBarFill, &mut Node)>) {
    for (bar, mut node) in &mut bar_query {
        let Ok(health) = health_query.get(bar.tracked_entity) else {
            continue;
        };
        let width = Val::Percent(health_ratio(*health, bar.max_health) * 100.0);
        // Unconditional writes would dirty every bar's `Node` each frame and
        // force UI relayout even when health is unchanged.
        if node.width != width {
            node.width = width;
        }
    }
}
