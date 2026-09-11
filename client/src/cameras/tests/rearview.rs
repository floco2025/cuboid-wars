use super::*;
use crate::test_fixtures;

#[test]
fn rearview_uses_live_follow_fov_independently_of_top_down() {
    let mut app = App::new();
    app.insert_resource(test_fixtures::client_settings())
        .init_resource::<CameraViewMode>()
        .init_resource::<UiScale>()
        .insert_resource(SceneRenderTarget {
            handle: Handle::default(),
            size: UVec2::new(1200, 800),
        })
        .add_systems(Update, local_player_rearview_viewport_system);
    app.world_mut().spawn(Window::default());
    let camera = app
        .world_mut()
        .spawn((RearviewCameraMarker, Camera::default(), Projection::default()))
        .id();
    for fov in [75.0_f32, 105.0] {
        app.world_mut().resource_mut::<ClientSettings>().preferences.fov_degrees = fov;
        app.update();
        let Projection::Perspective(projection) = app
            .world()
            .get::<Projection>(camera)
            .expect("rearview projection missing")
        else {
            panic!("rearview projection is not perspective");
        };
        assert_eq!(projection.fov, fov.to_radians());
    }
}
