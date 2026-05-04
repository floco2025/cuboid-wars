use bevy::prelude::*;
use common::{health::health_ratio, protocol::Health};

use crate::spawning::HealthBarFill;

pub fn ui_health_bars_system(health_query: Query<&Health>, mut bar_query: Query<(&HealthBarFill, &mut Node)>) {
    for (bar, mut node) in &mut bar_query {
        let Ok(health) = health_query.get(bar.tracked_entity) else {
            continue;
        };
        node.width = Val::Percent(health_ratio(*health, bar.max_health) * 100.0);
    }
}
