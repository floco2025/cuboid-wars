use super::*;

const fn level(level: u8, span: u8) -> MapLevel {
    MapLevel { level, span }
}

#[test]
fn one_span_rule_covers_floors_ramps_ladders_and_stacked_barriers() {
    let focused = FocusedMapLevel(Some(2));

    assert_eq!(
        map_level_visibility(FocusedMapLevel(None), level(1, 0)),
        Visibility::Visible
    );
    // A floor on its storey only.
    assert_eq!(map_level_visibility(focused, level(2, 0)), Visibility::Visible);
    assert_eq!(map_level_visibility(focused, level(1, 0)), Visibility::Hidden);
    // A ramp from storey 1 reaches storey 2.
    assert_eq!(map_level_visibility(focused, level(1, 1)), Visibility::Visible);
    assert_eq!(map_level_visibility(focused, level(0, 1)), Visibility::Hidden);
    // A ladder climbing two storeys from the ground, and a barrier
    // stacked over two storeys from 1.
    assert_eq!(map_level_visibility(focused, level(0, 2)), Visibility::Visible);
    assert_eq!(map_level_visibility(focused, level(0, 1)), Visibility::Hidden);
    assert_eq!(
        map_level_visibility(FocusedMapLevel(Some(3)), level(1, 1)),
        Visibility::Hidden
    );
}

#[test]
fn light_bridges_erasers_and_checkpoints_follow_level_focus_like_floors() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(FocusedMapLevel(Some(1)))
        .add_systems(
            Update,
            (map_level_focus_visibility_system, added_map_level_visibility_system).chain(),
        );
    let bridge = app
        .world_mut()
        .spawn((LightBridgeMarker, level(2, 0), Visibility::Visible))
        .id();
    let eraser = app
        .world_mut()
        .spawn((EraserMarker, level(2, 0), Visibility::Visible))
        .id();
    let checkpoint = app
        .world_mut()
        .spawn((CheckpointMarker, level(2, 0), Visibility::Visible))
        .id();
    let visibility = |app: &App| {
        [bridge, eraser, checkpoint].map(|entity| {
            *app.world()
                .get::<Visibility>(entity)
                .expect("map element lost its visibility")
        })
    };

    app.update();
    assert_eq!(visibility(&app), [Visibility::Hidden; 3]);

    app.insert_resource(FocusedMapLevel(Some(2)));
    app.update();
    assert_eq!(visibility(&app), [Visibility::Visible; 3]);
}

#[test]
fn a_record_on_a_moving_carrier_shows_on_every_storey_it_may_reach() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(FocusedMapLevel(Some(1)))
        .add_systems(
            Update,
            (map_level_focus_visibility_system, added_map_level_visibility_system).chain(),
        );
    // A lift's slab: its own storey 2, one more through the motion.
    let slab = app
        .world_mut()
        .spawn((GroundMarker, level(2, 1), Visibility::Visible))
        .id();
    let visibility = |app: &App| *app.world().get::<Visibility>(slab).expect("slab lost its visibility");

    app.update();
    assert_eq!(visibility(&app), Visibility::Hidden);

    for focused in [2, 3] {
        app.insert_resource(FocusedMapLevel(Some(focused)));
        app.update();
        assert_eq!(visibility(&app), Visibility::Visible, "focused on {focused}");
    }
}
