use bevy::{camera::visibility::RenderLayers, prelude::*};

use crate::{
    cameras::{CameraViewMode, MainCameraMarker},
    constants::{CHARACTER_LABEL_RENDER_LAYER, LOCAL_PLAYER_RENDER_LAYER, MAIN_VIEW_RENDER_LAYER},
    players::{LocalPlayerInfo, LocalPlayerMarker},
    ui::floating_labels::CharacterLabelRenderLayer,
};

pub fn local_player_visibility_sync_system(
    view_mode: Res<CameraViewMode>,
    local_player_info: Res<LocalPlayerInfo>,
    mut local_player_query: Query<&mut Visibility, With<LocalPlayerMarker>>,
    mut main_camera: Query<&mut RenderLayers, With<MainCameraMarker>>,
) {
    let desired_visibility = if local_player_info.is_dead {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };

    for mut visibility in &mut local_player_query {
        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
    }

    let mut camera_layers = RenderLayers::layer(0)
        .with(MAIN_VIEW_RENDER_LAYER)
        .with(CHARACTER_LABEL_RENDER_LAYER);
    if view_mode.is_top_down() {
        camera_layers = camera_layers.with(LOCAL_PLAYER_RENDER_LAYER);
    }
    for mut layers in &mut main_camera {
        if *layers != camera_layers {
            *layers = camera_layers.clone();
        }
    }
}

#[derive(Component)]
pub(crate) struct LocalPlayerRenderLayer;

pub(crate) fn local_player_render_layer_system(
    mut commands: Commands,
    local_players: Query<Entity, With<LocalPlayerMarker>>,
    children: Query<&Children>,
    meshes: Query<
        (),
        (
            With<Mesh3d>,
            Without<LocalPlayerRenderLayer>,
            Without<CharacterLabelRenderLayer>,
        ),
    >,
) {
    for player in &local_players {
        for descendant in children.iter_descendants(player) {
            if meshes.contains(descendant) {
                commands
                    .entity(descendant)
                    .insert((RenderLayers::layer(LOCAL_PLAYER_RENDER_LAYER), LocalPlayerRenderLayer));
            }
        }
    }
}

#[derive(Component)]
pub(crate) struct LocalPlayerLightLayer;

pub(crate) fn local_player_light_layer_system(
    mut commands: Commands,
    lights: Query<
        (Entity, Option<&RenderLayers>),
        (
            Or<(With<DirectionalLight>, With<PointLight>, With<SpotLight>)>,
            Without<LocalPlayerLightLayer>,
        ),
    >,
) {
    for (entity, layers) in &lights {
        let layers = layers.cloned().unwrap_or_default().with(LOCAL_PLAYER_RENDER_LAYER);
        commands.entity(entity).insert((layers, LocalPlayerLightLayer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_person_hides_local_meshes_from_main_camera_only() {
        let mut app = App::new();
        app.insert_resource(CameraViewMode::FirstPerson)
            .insert_resource(LocalPlayerInfo::default())
            .add_systems(Update, local_player_visibility_sync_system);
        let player = app.world_mut().spawn((LocalPlayerMarker, Visibility::Hidden)).id();
        let camera = app.world_mut().spawn((MainCameraMarker, RenderLayers::default())).id();

        app.update();

        assert_eq!(
            app.world().entity(player).get::<Visibility>(),
            Some(&Visibility::Visible)
        );
        let layers = app
            .world()
            .entity(camera)
            .get::<RenderLayers>()
            .expect("main camera render layers missing");
        assert!(layers.intersects(&RenderLayers::layer(MAIN_VIEW_RENDER_LAYER)));
        assert!(layers.intersects(&RenderLayers::layer(CHARACTER_LABEL_RENDER_LAYER)));
        assert!(!layers.intersects(&RenderLayers::layer(LOCAL_PLAYER_RENDER_LAYER)));
    }

    #[test]
    fn top_down_camera_includes_local_player_layer() {
        let mut app = App::new();
        app.insert_resource(CameraViewMode::TopDown)
            .insert_resource(LocalPlayerInfo::default())
            .add_systems(Update, local_player_visibility_sync_system);
        app.world_mut().spawn((LocalPlayerMarker, Visibility::Visible));
        let camera = app.world_mut().spawn((MainCameraMarker, RenderLayers::default())).id();

        app.update();

        let layers = app
            .world()
            .entity(camera)
            .get::<RenderLayers>()
            .expect("main camera render layers missing");
        assert!(layers.intersects(&RenderLayers::layer(LOCAL_PLAYER_RENDER_LAYER)));
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
        assert_eq!(mesh_layers, &RenderLayers::layer(LOCAL_PLAYER_RENDER_LAYER));
        let light_layers = app
            .world()
            .entity(light)
            .get::<RenderLayers>()
            .expect("light render layers missing");
        assert!(light_layers.intersects(&RenderLayers::layer(0)));
        assert!(light_layers.intersects(&RenderLayers::layer(LOCAL_PLAYER_RENDER_LAYER)));
    }
}
