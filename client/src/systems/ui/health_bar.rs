use bevy::prelude::*;
use common::protocol::Health;

use crate::{markers::HealthBarFillMarker, spawning::health_ratio};

pub fn ui_health_bars_system(health_query: Query<&Health>, mut bar_query: Query<(&HealthBarFillMarker, &mut Node)>) {
    for (bar, mut node) in &mut bar_query {
        let Ok(health) = health_query.get(bar.tracked_entity) else {
            continue;
        };
        node.width = Val::Percent(health_ratio(health.0, bar.max_health) * 100.0);
    }
}
