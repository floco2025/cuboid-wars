use std::iter::once;

use bevy::prelude::*;

use super::fade::fade_out_alpha;
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
}

// Ages every row, fades it over its final `HUD_LINE_FADE_SECS`, despawns it
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
            let fade = fade_out_alpha(line.remaining_secs, HUD_LINE_FADE_SECS);
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
        visibility.set_if_neq(if alive == 0 {
            Visibility::Hidden
        } else {
            Visibility::Visible
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_systems(Update, ui_timed_lines_system);
        app
    }

    fn root(app: &mut App, max_rows: usize) -> Entity {
        app.world_mut()
            .spawn((
                TimedLines {
                    max_rows,
                    background_alpha: 0.5,
                },
                BackgroundColor(Color::BLACK),
                Visibility::Hidden,
            ))
            .id()
    }

    fn row(app: &mut App, root: Entity, secs: f32) -> Entity {
        app.world_mut()
            .spawn((
                TimedLine { remaining_secs: secs },
                ChildOf(root),
                TextColor(Color::WHITE),
            ))
            .id()
    }

    fn advance(app: &mut App, secs: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(secs));
        app.update();
    }

    fn rows_of(app: &App, root: Entity) -> Vec<Entity> {
        app.world()
            .get::<Children>(root)
            .map(|children| children.iter().collect())
            .unwrap_or_default()
    }

    fn visibility(app: &App, root: Entity) -> Visibility {
        *app.world().get::<Visibility>(root).expect("root visibility")
    }

    fn text_alpha(app: &App, entity: Entity) -> f32 {
        app.world().get::<TextColor>(entity).expect("row text color").0.alpha()
    }

    fn band_alpha(app: &App, root: Entity) -> f32 {
        app.world()
            .get::<BackgroundColor>(root)
            .expect("root background")
            .0
            .alpha()
    }

    #[test]
    fn rows_expire_independently_and_the_root_hides_when_empty() {
        let mut app = app();
        let root = root(&mut app, 5);
        let short = row(&mut app, root, 2.0);
        let long = row(&mut app, root, 4.0);

        advance(&mut app, 1.0);
        assert_eq!(rows_of(&app, root), [short, long]);
        assert_eq!(visibility(&app, root), Visibility::Visible);

        advance(&mut app, 1.5);
        assert_eq!(rows_of(&app, root), [long]);

        advance(&mut app, 2.0);
        assert!(rows_of(&app, root).is_empty());
        assert_eq!(visibility(&app, root), Visibility::Hidden);
    }

    #[test]
    fn rows_fade_over_their_final_seconds_and_the_band_follows_the_strongest() {
        let mut app = app();
        let root = root(&mut app, 5);
        let fading = row(&mut app, root, HUD_LINE_FADE_SECS);
        let fresh = row(&mut app, root, 10.0);

        advance(&mut app, HUD_LINE_FADE_SECS / 2.0);

        assert!((text_alpha(&app, fading) - 0.5).abs() < 1e-5);
        assert_eq!(text_alpha(&app, fresh), 1.0);
        assert_eq!(
            band_alpha(&app, root),
            0.5,
            "background at its base alpha while a row is fresh"
        );

        advance(&mut app, 10.0);
        assert_eq!(band_alpha(&app, root), 0.0);
    }

    #[test]
    fn overflow_expires_the_oldest_rows() {
        let mut app = app();
        let root = root(&mut app, 2);
        let oldest = row(&mut app, root, 10.0);
        let middle = row(&mut app, root, 10.0);
        let newest = row(&mut app, root, 10.0);

        advance(&mut app, 0.0);

        assert_eq!(rows_of(&app, root), [middle, newest]);
        assert!(app.world().get_entity(oldest).is_err());
    }
}
