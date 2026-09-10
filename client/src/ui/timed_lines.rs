use std::iter::once;

use bevy::prelude::*;

use super::fade::fade_out_alpha;
#[cfg(test)]
use crate::constants::HUD_LINE_FADE_SECS;

// A column of rows that each live for a while and fade out — the HUD banner
// and the message feed. Rows are the root's children in arrival order,
// oldest first; the root hides while it has none.
#[derive(Component)]
pub struct TimedLines {
    pub max_rows: usize,
    // Alpha of the root's `BackgroundColor` behind a full-strength row; the
    // longest-lived row's fade modulates it so a band goes with its last
    // line. Zero for a root without a background.
    pub background_alpha: f32,
}

#[derive(Component)]
pub struct TimedLine {
    pub remaining_secs: f32,
    pub fade_out_secs: f32,
}

// Ages every row, fades it over its own final fade interval, despawns it
// once expired, and expires the oldest rows beyond the cap on the spot.
pub fn ui_timed_lines_system(
    mut commands: Commands,
    time: Res<Time>,
    mut roots: Query<(
        &TimedLines,
        Option<&Children>,
        &mut Visibility,
        Option<&mut BackgroundColor>,
    )>,
    mut rows: Query<(&mut TimedLine, Option<&Children>)>,
    mut colors: Query<&mut TextColor>,
) {
    let delta = time.delta_secs();
    for (root, children, mut visibility, background) in &mut roots {
        let row_entities: Vec<Entity> = children.map(|children| children.iter().collect()).unwrap_or_default();
        let overflow = row_entities.len().saturating_sub(root.max_rows);
        let mut alive = 0;
        let mut strongest = 0.0_f32;
        for (index, &row) in row_entities.iter().enumerate() {
            let Ok((mut line, texts)) = rows.get_mut(row) else {
                continue;
            };
            line.remaining_secs = if index < overflow {
                0.0
            } else {
                line.remaining_secs - delta
            };
            if line.remaining_secs <= 0.0 {
                commands.entity(row).despawn();
                continue;
            }
            alive += 1;
            let fade = fade_out_alpha(line.remaining_secs, line.fade_out_secs);
            strongest = strongest.max(fade);
            // A row is either one text or a row of text runs.
            let texts = texts.into_iter().flat_map(|texts| texts.iter());
            for entity in once(row).chain(texts) {
                if let Ok(mut color) = colors.get_mut(entity)
                    && color.0.alpha() != fade
                {
                    color.0.set_alpha(fade);
                }
            }
        }
        if let Some(mut background) = background {
            let alpha = root.background_alpha * strongest;
            if background.0.alpha() != alpha {
                background.0.set_alpha(alpha);
            }
        }
        // A stable nonempty row count should not retrigger visibility propagation every frame.
        visibility.set_if_neq(if alive == 0 {
            Visibility::Hidden
        } else {
            Visibility::Visible
        });
    }
}

#[cfg(test)]
#[path = "tests/timed_lines.rs"]
mod tests;
