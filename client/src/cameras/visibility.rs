use bevy::{camera::visibility::RenderLayers, prelude::*};

use super::{
    CameraViewMode, MainCameraMarker, RENDER_LAYER_CHARACTER_LABEL, RENDER_LAYER_LOCAL_PLAYER, RENDER_LAYER_MAIN_VIEW,
};
use crate::{players::LocalPlayerMarker, ui::floating_labels::CharacterLabelRenderLayerMarker};

#[derive(Component)]
pub struct LocalPlayerLabelMarker;

pub fn local_player_view_mode_system(
    view_mode: Res<CameraViewMode>,
    mut labels: Query<(Ref<LocalPlayerLabelMarker>, &mut Visibility)>,
    mut main_cameras: Query<(Ref<MainCameraMarker>, &mut RenderLayers)>,
) {
    let label_visibility = if view_mode.is_top_down() {
        // A hidden player, such as a dead local player, must also hide its labels.
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let mode_changed = view_mode.is_changed();
    // Labels can spawn without a mode change, so newly spawned ones still need the current mode.
    for (marker, mut visibility) in &mut labels {
        if mode_changed || marker.is_added() {
            *visibility = label_visibility;
        }
    }

    let mut camera_layers = RenderLayers::layer(0)
        .with(RENDER_LAYER_MAIN_VIEW)
        .with(RENDER_LAYER_CHARACTER_LABEL);
    if !view_mode.is_first_person() {
        camera_layers = camera_layers.with(RENDER_LAYER_LOCAL_PLAYER);
    }
    for (marker, mut layers) in &mut main_cameras {
        if (mode_changed || marker.is_added()) && *layers != camera_layers {
            *layers = camera_layers.clone();
        }
    }
}

#[derive(Component)]
pub(crate) struct LocalPlayerRenderLayerMarker;

// Scene meshes appear asynchronously, so tag each one once instead of rescanning the player hierarchy.
pub(crate) fn local_player_render_layer_system(
    mut commands: Commands,
    local_players: Query<(), With<LocalPlayerMarker>>,
    parents: Query<&ChildOf>,
    meshes: Query<
        Entity,
        (
            With<Mesh3d>,
            Added<Mesh3d>,
            Without<LocalPlayerRenderLayerMarker>,
            Without<CharacterLabelRenderLayerMarker>,
        ),
    >,
) {
    for mesh in &meshes {
        let mut ancestor = mesh;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if local_players.contains(ancestor) {
                commands.entity(mesh).insert((
                    RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER),
                    LocalPlayerRenderLayerMarker,
                ));
                break;
            }
        }
    }
}

#[derive(Component)]
pub(crate) struct LocalPlayerLightLayerMarker;

pub(crate) fn local_player_light_layer_system(
    mut commands: Commands,
    lights: Query<
        (Entity, Option<&RenderLayers>),
        (
            Or<(With<DirectionalLight>, With<PointLight>, With<SpotLight>)>,
            Without<LocalPlayerLightLayerMarker>,
        ),
    >,
) {
    for (entity, layers) in &lights {
        let layers = layers.cloned().unwrap_or_default().with(RENDER_LAYER_LOCAL_PLAYER);
        commands.entity(entity).insert((layers, LocalPlayerLightLayerMarker));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_person_hides_local_labels_and_excludes_local_mesh_layer() {
        let mut app = App::new();
        app.insert_resource(CameraViewMode::FirstPerson)
            .add_systems(Update, local_player_view_mode_system);
        let label = app
            .world_mut()
            .spawn((LocalPlayerLabelMarker, Visibility::Visible))
            .id();
        let camera = app.world_mut().spawn((MainCameraMarker, RenderLayers::default())).id();

        app.update();

        assert_eq!(app.world().entity(label).get::<Visibility>(), Some(&Visibility::Hidden));
        let layers = app
            .world()
            .entity(camera)
            .get::<RenderLayers>()
            .expect("main camera render layers missing");
        assert!(layers.intersects(&RenderLayers::layer(RENDER_LAYER_MAIN_VIEW)));
        assert!(layers.intersects(&RenderLayers::layer(RENDER_LAYER_CHARACTER_LABEL)));
        assert!(!layers.intersects(&RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER)));
    }

    #[test]
    fn top_down_camera_includes_local_player_layer_and_labels_inherit_player_visibility() {
        let mut app = App::new();
        app.insert_resource(CameraViewMode::TopDown)
            .add_systems(Update, local_player_view_mode_system);
        let label = app.world_mut().spawn((LocalPlayerLabelMarker, Visibility::Hidden)).id();
        let camera = app.world_mut().spawn((MainCameraMarker, RenderLayers::default())).id();

        app.update();

        let layers = app
            .world()
            .entity(camera)
            .get::<RenderLayers>()
            .expect("main camera render layers missing");
        assert!(layers.intersects(&RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER)));
        assert_eq!(
            app.world().entity(label).get::<Visibility>(),
            Some(&Visibility::Inherited)
        );

        let remote_label = app.world_mut().spawn(Visibility::Inherited).id();
        app.insert_resource(CameraViewMode::ThirdPerson);
        app.update();
        assert_eq!(app.world().get::<Visibility>(label), Some(&Visibility::Hidden));
        assert_eq!(
            app.world().get::<Visibility>(remote_label),
            Some(&Visibility::Inherited)
        );
        assert!(
            app.world()
                .get::<RenderLayers>(camera)
                .expect("main camera render layers missing")
                .intersects(&RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER))
        );
    }

    #[test]
    fn local_player_descendant_meshes_and_lights_share_render_layer() {
        let mut app = App::new();
        app.add_systems(
            Update,
            (local_player_render_layer_system, local_player_light_layer_system),
        );
        let mesh = app.world_mut().spawn(Mesh3d::default()).id();
        let player = app.world_mut().spawn(LocalPlayerMarker).add_child(mesh).id();
        let light = app.world_mut().spawn(PointLight::default()).id();

        app.update();

        assert!(app.world().entity(player).contains::<LocalPlayerMarker>());
        let mesh_layers = app
            .world()
            .entity(mesh)
            .get::<RenderLayers>()
            .expect("local mesh render layers missing");
        assert_eq!(mesh_layers, &RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER));
        let light_layers = app
            .world()
            .entity(light)
            .get::<RenderLayers>()
            .expect("light render layers missing");
        assert!(light_layers.intersects(&RenderLayers::layer(0)));
        assert!(light_layers.intersects(&RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER)));
    }

    #[test]
    fn newly_added_local_label_uses_current_view_mode() {
        for view in [CameraViewMode::FirstPerson, CameraViewMode::ThirdPerson] {
            let mut app = App::new();
            app.insert_resource(view)
                .add_systems(Update, local_player_view_mode_system);
            app.world_mut().spawn((MainCameraMarker, RenderLayers::default()));
            app.update();

            let label = app
                .world_mut()
                .spawn((LocalPlayerLabelMarker, Visibility::Visible))
                .id();
            app.update();

            assert_eq!(app.world().entity(label).get::<Visibility>(), Some(&Visibility::Hidden));
        }
    }
    #[test]
    fn meshes_spawned_after_update_have_local_layers_before_visibility_checks() {
        use crate::cameras::camera_plugin;
        use bevy::{app::SpawnScene, camera::visibility::VisibilitySystems};

        let mut app = App::new();
        camera_plugin(&mut app);
        app.world_mut().spawn(LocalPlayerMarker);
        app.add_systems(
            SpawnScene,
            |mut commands: Commands, player: Single<Entity, With<LocalPlayerMarker>>| {
                commands.entity(*player).with_children(|root| {
                    root.spawn(Name::new("Head")).with_children(|head| {
                        head.spawn((Name::new("Eye"), Mesh3d::default()));
                    });
                });
            },
        );
        app.add_systems(
            PostUpdate,
            (|meshes: Query<&RenderLayers, With<Mesh3d>>| {
                let layers = meshes
                    .single()
                    .expect("eye mesh layer missing before visibility checks");
                assert_eq!(layers, &RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER));
                assert!(!layers.intersects(&RenderLayers::layer(0)));
            })
            .in_set(VisibilitySystems::CheckVisibility),
        );
        app.world_mut().run_schedule(SpawnScene);
        app.world_mut().run_schedule(PostUpdate);
    }
}
