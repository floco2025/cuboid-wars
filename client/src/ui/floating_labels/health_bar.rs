use bevy::prelude::*;
use common::{health::health_ratio, protocol::Health};

use crate::ui::floating_labels::spawn::{FloatingHealthBarFill, floating_health_bar_fill_transform};

// Rescale each character's world-space health-bar fill quad to its tracked
// character's current health. The bar is plain geometry rendered by the main
// camera (no render target), so this directly and reliably reflects every
// `Health` change. Writes only on change to avoid dirtying transforms.
pub fn floating_health_bar_fill_system(
    health_query: Query<&Health>,
    mut fill_query: Query<(&FloatingHealthBarFill, &mut Transform)>,
) {
    for (fill, mut transform) in &mut fill_query {
        let Ok(health) = health_query.get(fill.tracked_entity) else {
            continue;
        };
        let target = floating_health_bar_fill_transform(health_ratio(*health, fill.max_health), fill.full_width);
        if transform.translation.x != target.translation.x || transform.scale.x != target.scale.x {
            transform.translation = target.translation;
            transform.scale = target.scale;
        }
    }
}
