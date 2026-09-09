use bevy::prelude::*;

use super::{
    third_person::third_person_transform,
    top_down::{topdown_camera_transform, window_aspect_ratio},
};
use crate::{
    cameras::{CameraViewMode, FollowCamera, MainCameraMarker, TopDownCameraYaw},
    characters::PreviousTickPosition,
    config::ClientSettings,
    players::{CameraShake, LocalPlayerInfo, LocalPlayerMarker},
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{MapLayout, MapSettings, Position},
};

// Update camera position to follow local player. Physics ticks at 30 Hz;
// interpolate between last-tick and current-tick positions so the camera
// stays smooth at the render rate.
pub fn local_player_camera_sync_system(
    local_player_query: Query<(&Position, &PreviousTickPosition), With<LocalPlayerMarker>>,
    map_layout: Res<MapLayout>,
    map_settings: Res<MapSettings>,
    windows: Query<&Window>,
    fixed_time: Res<Time<Fixed>>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
    mut view_mode: ResMut<CameraViewMode>,
    top_down_camera_yaw: Res<TopDownCameraYaw>,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
    local_player_info: Res<LocalPlayerInfo>,
    collision_world: Res<CollisionWorld>,
    mut third: ResMut<FollowCamera>,
    time: Res<Time>,
) {
    let Some((current_pos, prev_pos)) = local_player_query.iter().next() else {
        return;
    };
    let interp = prev_pos.lerp_to(*current_pos, fixed_time.overstep_fraction());
    let interpolated = Position {
        x: interp.x,
        y: interp.y,
        z: interp.z,
    };
    let player_pos = &interpolated;

    let Ok((mut camera_transform, mut projection, maybe_shake)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Perspective(persp) = projection.as_mut() else {
        return;
    };

    persp.fov = if view_mode.is_top_down() {
        client_settings.camera.top_down.fov_degrees
    } else {
        client_settings.preferences.fov_degrees
    }
    .to_radians();

    if view_mode.is_top_down() {
        *camera_transform = topdown_camera_transform(
            player_pos,
            Some(&map_layout),
            map_settings.geometry,
            window_aspect_ratio(&windows),
            persp.fov,
            top_down_camera_yaw.0,
            client_settings.camera.top_down.margin,
            client_settings.camera.top_down.tilt_degrees,
        );
        return;
    }

    let mut config = client_settings.camera.follow;
    let eye_height = gameplay_config.player.eye_height();
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        local_player_info.stored_yaw,
        local_player_info.stored_pitch,
        0.0,
    );
    if third.distance > config.first_person_distance {
        let blend = third.pivot_blend();
        let height = eye_height + (config.pivot_height - eye_height) * blend;
        config.shoulder_offset *= blend;
        let pivot = Vec3::new(player_pos.x, player_pos.y + height, player_pos.z);
        let near_half_height = persp.near * (persp.fov * 0.5).tan();
        let radius = config.collision_radius.max(
            Vec3::new(
                near_half_height * window_aspect_ratio(&windows),
                near_half_height,
                persp.near,
            )
            .length(),
        );
        *camera_transform = third_person_transform(
            &collision_world,
            pivot,
            rotation,
            config,
            radius,
            time.delta_secs(),
            &mut third,
        );
    } else {
        third.arm_distance = 0.0;
        third.previous_pivot = None;
    }

    if third.arm_distance > config.first_person_distance {
        view_mode.set_if_neq(CameraViewMode::ThirdPerson);
    } else {
        view_mode.set_if_neq(CameraViewMode::FirstPerson);
        third.locked = true;
        camera_transform.rotation = rotation;
        camera_transform.translation = Vec3::new(player_pos.x, player_pos.y + eye_height, player_pos.z);
    }
    if let Some(shake) = maybe_shake {
        camera_transform.translation += Vec3::new(shake.offset_x, shake.offset_y, shake.offset_z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actors::ActorMap,
        cameras::{CameraAim, RENDER_LAYER_LOCAL_PLAYER, camera_aim_system},
        constants::CROSSHAIR_THIRD_PERSON_HEIGHT,
        players::{MyPlayerId, PlayerMap, camera::visibility::local_player_view_mode_system},
        test_geometry,
    };
    use bevy::camera::visibility::RenderLayers;
    use common::protocol::{BarrierKindTable, CarrierId, FaceYaw, PlateState, PlayerId, Wall};
    use std::time::Duration;

    fn world(wall: bool) -> CollisionWorld {
        let layout = MapLayout {
            walls: if wall {
                vec![Wall {
                    x1: -5.0,
                    z1: 0.8,
                    x2: 5.0,
                    z2: 0.8,
                    width: 0.2,
                    y: 0.0,
                    height: 4.0,
                    level: 0,
                    carrier: CarrierId::WORLD,
                }]
            } else {
                vec![]
            },
            ..default()
        };
        CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default())
    }

    fn app() -> (App, Entity, f32) {
        let source: serde_json::Value = serde_json::from_str(include_str!("../../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let gameplay: GameplayConfig = serde_json::from_value(serde_json::json!({
            "player": source["player"], "actors": source["actors"]["kinds"],
            "projectiles": source["weapons"]["projectiles"], "missiles": source["weapons"]["missiles"],
            "portals": source["weapons"]["portals"],
        }))
        .expect("client gameplay config is invalid");
        let eye_height = gameplay.player.eye_height();
        let mut settings = ClientSettings::load_default().expect("client settings are invalid");
        settings.camera.follow = test_geometry::follow_camera();
        let mut app = App::new();
        app.insert_resource(gameplay)
            .insert_resource(settings)
            .insert_resource(test_geometry::map_settings())
            .insert_resource(world(false))
            .init_resource::<MapLayout>()
            .init_resource::<Time>()
            .init_resource::<Time<Fixed>>()
            .init_resource::<CameraViewMode>()
            .init_resource::<FollowCamera>()
            .init_resource::<TopDownCameraYaw>()
            .init_resource::<LocalPlayerInfo>()
            .init_resource::<CameraAim>()
            .init_resource::<PlayerMap>()
            .init_resource::<ActorMap>()
            .init_resource::<PlateState>()
            .insert_resource(MyPlayerId(PlayerId(1)))
            .add_systems(
                Update,
                (
                    local_player_camera_sync_system,
                    camera_aim_system,
                    local_player_view_mode_system,
                )
                    .chain(),
            );
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(1.0 / 60.0));
        app.world_mut().spawn((
            LocalPlayerMarker,
            FaceYaw(0.0),
            Position::default(),
            PreviousTickPosition(Position::default()),
        ));
        let camera = app
            .world_mut()
            .spawn((
                MainCameraMarker,
                Camera3d::default(),
                Transform::default(),
                Projection::default(),
                RenderLayers::default(),
            ))
            .id();
        (app, camera, eye_height)
    }

    fn assert_view(app: &App, camera: Entity, eye_height: f32, first_person: bool) {
        assert_eq!(app.world().resource::<CameraViewMode>().is_first_person(), first_person);
        let layers = app.world().get::<RenderLayers>(camera).expect("camera layers missing");
        assert_eq!(
            layers.intersects(&RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER)),
            !first_person
        );
        let pose = app.world().get::<Transform>(camera).expect("camera transform missing");
        if first_person {
            assert_eq!(pose.translation, Vec3::Y * eye_height);
        } else {
            let threshold = app
                .world()
                .resource::<ClientSettings>()
                .camera
                .follow
                .first_person_distance;
            assert!(pose.translation.z > threshold);
        }
    }

    #[test]
    fn zoom_snap_and_body_visibility_change_on_the_same_frame() {
        let (mut app, camera, eye_height) = app();
        app.update();
        assert_view(&app, camera, eye_height, true);
        for distance in [1.2, 0.8, 0.701, 0.7, 0.0, 1.0] {
            app.world_mut().resource_mut::<FollowCamera>().distance = distance;
            app.update();
            assert_view(&app, camera, eye_height, distance <= 0.7);
        }
    }

    #[test]
    fn obstruction_snaps_to_eye_and_preserves_zoom_for_recovery() {
        let (mut app, camera, eye_height) = app();
        app.world_mut().resource_mut::<FollowCamera>().distance = 3.0;
        app.update();
        assert_view(&app, camera, eye_height, false);
        app.insert_resource(world(true));
        app.update();
        assert_view(&app, camera, eye_height, true);
        assert_eq!(app.world().resource::<FollowCamera>().distance, 3.0);
        app.insert_resource(world(false));
        for _ in 0..30 {
            app.update();
            let first = app.world().resource::<CameraViewMode>().is_first_person();
            assert_view(&app, camera, eye_height, first);
        }
        assert_view(&app, camera, eye_height, false);
    }

    #[test]
    fn inward_scroll_while_obstruction_forces_first_person_clears_saved_zoom() {
        let (mut app, camera, eye_height) = app();
        app.world_mut().resource_mut::<FollowCamera>().distance = 3.0;
        app.insert_resource(world(true));
        app.update();
        assert_view(&app, camera, eye_height, true);
        let config = app.world().resource::<ClientSettings>().camera.follow;
        let view = *app.world().resource::<CameraViewMode>();
        app.world_mut()
            .resource_mut::<FollowCamera>()
            .zoom(view, 0.1, 1.0, config);

        app.insert_resource(world(false));
        for _ in 0..30 {
            app.update();
            assert_view(&app, camera, eye_height, true);
        }
        assert_eq!(app.world().resource::<FollowCamera>().distance, 0.0);
    }

    #[test]
    fn crosshair_height_is_fixed_in_third_person_and_recentres_when_obstructed() {
        let (mut app, _, _) = app();
        app.update();
        assert_eq!(app.world().resource::<CameraAim>().crosshair_height_offset, 0.0);
        for distance in [1.0, 3.0, 6.0] {
            app.world_mut().resource_mut::<FollowCamera>().distance = distance;
            app.update();
            let offset = app.world().resource::<CameraAim>().crosshair_height_offset;
            assert_eq!(offset, CROSSHAIR_THIRD_PERSON_HEIGHT);
        }
        app.insert_resource(world(true));
        app.update();
        assert_eq!(app.world().resource::<CameraAim>().crosshair_height_offset, 0.0);
        app.insert_resource(world(false));
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<CameraAim>().crosshair_height_offset,
            CROSSHAIR_THIRD_PERSON_HEIGHT
        );
        app.insert_resource(CameraViewMode::TopDown);
        app.update();
        assert_eq!(app.world().resource::<CameraAim>().crosshair_height_offset, 0.0);
    }
}
