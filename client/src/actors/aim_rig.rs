use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::config::AimRigDef;

#[derive(Component)]
pub struct FixedFacingMarker;

#[derive(Component)]
pub struct AimJointMarker;

#[derive(Component)]
pub struct AimRig {
    yaw: Entity,
    pitch: Entity,
    parents: Vec<Entity>,
    pivot: Vec3,
    muzzle_distance: f32,
}

pub fn aim_rig_setup_system(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    definitions: Query<&AimRigDef>,
    transforms: Query<&Transform>,
) {
    let Ok(definition) = definitions.get(ready.entity) else {
        return;
    };
    let find = |name: &str| {
        children
            .iter_descendants(ready.entity)
            .find(|entity| names.get(*entity).is_ok_and(|node| node.as_str() == name))
    };
    let (Some(yaw), Some(pitch), Some(muzzle)) = (
        find(&definition.yaw_node),
        find(&definition.pitch_node),
        find(&definition.muzzle_node),
    ) else {
        error!(?definition, "model is missing configured aim rig nodes");
        return;
    };
    let actor = parents
        .get(ready.entity)
        .expect("aim rig model parent missing")
        .parent();
    let mut chain = Vec::new();
    let mut ancestor = parents.get(yaw).expect("aim yaw parent missing").parent();
    while ancestor != actor {
        chain.push(ancestor);
        ancestor = parents.get(ancestor).expect("aim rig ancestor missing").parent();
    }
    chain.reverse();
    commands.entity(actor).insert(AimRig {
        yaw,
        pitch,
        parents: chain,
        pivot: transforms.get(yaw).expect("aim rig yaw transform missing").translation,
        muzzle_distance: transforms
            .get(muzzle)
            .expect("aim rig muzzle transform missing")
            .translation
            .length(),
    });
    commands.entity(yaw).insert(AimJointMarker);
    commands.entity(pitch).insert(AimJointMarker);
}

impl AimRig {
    pub fn frame(
        &self,
        actor: &Transform,
        mut transform: impl FnMut(Entity) -> Option<Transform>,
    ) -> Option<GlobalTransform> {
        let mut frame = GlobalTransform::from(*actor);
        for parent in &self.parents {
            frame = frame.mul_transform(transform(*parent)?);
        }
        Some(frame)
    }

    pub fn pivot(&self, frame: &GlobalTransform) -> Vec3 {
        frame.transform_point(self.pivot)
    }

    pub fn muzzle_distance(&self, frame: &GlobalTransform) -> f32 {
        self.muzzle_distance * frame.scale().z
    }

    pub fn aim(
        &self,
        frame: &GlobalTransform,
        direction: Vec3,
        joints: &mut Query<&mut Transform, With<AimJointMarker>>,
    ) {
        let local = frame.rotation().inverse() * direction;
        let (yaw, pitch) = aim_rotations(local);
        if let Ok(mut transform) = joints.get_mut(self.yaw) {
            transform.rotation = yaw;
        }
        if let Ok(mut transform) = joints.get_mut(self.pitch) {
            transform.rotation = pitch;
        }
    }
}

fn aim_rotations(direction: Vec3) -> (Quat, Quat) {
    (
        Quat::from_rotation_y((-direction.x).atan2(-direction.z)),
        Quat::from_rotation_x(direction.y.atan2(direction.xz().length())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actors::{ActorInfo, ActorMap},
        characters::{AnimationToPlay, character_animation_system, characters_visual_turn_system},
        config::{AssetSet, ModelDef},
        players::{PlayerInfo, PlayerMap},
        vfx::{LaserBeam, laser_beam_update_system},
    };
    use bevy::{
        app::AnimationSystems,
        gltf::{Gltf, GltfAssetLabel, GltfPlugin},
        image::{CompressedImageFormatSupport, CompressedImageFormats, ImagePlugin},
        mesh::MeshPlugin,
        time::TimeUpdateStrategy,
        transform::TransformSystems,
        world_serialization::WorldSerializationPlugin,
    };
    use common::{
        config::GameplayConfig,
        physics::CollisionWorld,
        protocol::{
            ActorId, ActorMarker, BarrierKindTable, CarrierId, FaceYaw, Health, MapLayout, PlateState, Player,
            PlayerId, PlayerMoveIntent, Position, Wall,
        },
    };
    use std::time::{Duration, Instant};

    #[derive(Resource)]
    struct TestAimDirection(Vec3);

    fn aim_loaded_models(
        direction: Res<TestAimDirection>,
        rigs: Query<(&AimRig, &Transform), Without<AimJointMarker>>,
        transforms: Query<&Transform, Without<AimJointMarker>>,
        mut joints: Query<&mut Transform, With<AimJointMarker>>,
    ) {
        for (rig, actor) in &rigs {
            let frame = rig
                .frame(actor, |entity| transforms.get(entity).ok().copied())
                .expect("aim parent transform missing");
            rig.aim(&frame, direction.0, &mut joints);
        }
    }

    #[test]
    fn configured_aim_models_track_targets_while_their_animations_play() {
        let assets: serde_json::Value = serde_json::from_str(include_str!("../../../config/client/assets.json"))
            .expect("client assets JSON is invalid");
        for actor in assets["actors"].as_object().expect("actor assets missing").values() {
            let model: ModelDef = serde_json::from_value(actor["model"].clone()).expect("actor model is invalid");
            let Some(definition) = model.aim_rig.clone() else {
                continue;
            };
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                AssetPlugin {
                    file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
                    ..default()
                },
                TransformPlugin,
                WorldSerializationPlugin,
                ImagePlugin::default(),
                MeshPlugin,
                AnimationPlugin,
                GltfPlugin::default(),
            ));
            app.insert_resource(CompressedImageFormatSupport(CompressedImageFormats::NONE));
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 30.0)));
            app.insert_resource(TestAimDirection(Vec3::Z));
            app.add_systems(
                PostUpdate,
                aim_loaded_models
                    .after(AnimationSystems)
                    .before(TransformSystems::Propagate),
            );
            app.finish();
            app.cleanup();
            let actor_transform = Transform::from_xyz(3.0, 2.0, -1.0).with_rotation(Quat::from_rotation_y(0.6));
            let owner = app.world_mut().spawn(actor_transform).id();
            let server = app.world().resource::<AssetServer>().clone();
            let path = model.scene.split('#').next().expect("model path missing").to_owned();
            let gltf: Handle<Gltf> = server.load(path.clone());
            let entity = app
                .world_mut()
                .spawn((
                    WorldAssetRoot(server.load(model.scene.clone())),
                    Transform::from_xyz(model.x_offset, model.y_offset, model.z_offset)
                        .with_scale(Vec3::splat(model.scale))
                        .with_rotation(Quat::from_rotation_x(model.x_rotation_degrees.to_radians())),
                    definition.clone(),
                    ChildOf(owner),
                ))
                .observe(aim_rig_setup_system)
                .id();
            if let Some(speed) = model.animation_speed {
                let (graph, index) = AnimationGraph::from_clip(
                    server.load(GltfAssetLabel::Animation(model.animation_index).from_asset(path)),
                );
                let graph_handle = app.world_mut().resource_mut::<Assets<AnimationGraph>>().add(graph);
                app.world_mut()
                    .entity_mut(entity)
                    .insert(AnimationToPlay {
                        graph_handle,
                        index,
                        speed,
                    })
                    .observe(character_animation_system);
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            while !server.is_loaded_with_dependencies(&gltf) || app.world().get::<AimRig>(owner).is_none() {
                assert!(Instant::now() < deadline, "aim model failed to load: {}", model.scene);
                app.update();
                std::thread::sleep(Duration::from_millis(5));
            }
            for _ in 0..12 {
                app.update();
            }
            let before: Vec<_> = app
                .world_mut()
                .query::<(&Name, &Transform)>()
                .iter(app.world())
                .map(|(name, transform)| (name.to_string(), *transform))
                .collect();
            for direction in [Vec3::new(1.0, 0.4, -2.0), Vec3::new(-2.0, -0.3, 1.0)] {
                let direction = direction.normalize();
                app.insert_resource(TestAimDirection(direction));
                for _ in 0..5 {
                    app.update();
                }
                let rig = app.world().get::<AimRig>(owner).expect("aim rig missing");
                let frame = rig
                    .frame(&actor_transform, |entity| app.world().get::<Transform>(entity).copied())
                    .expect("aim parent missing");
                let expected = rig.pivot(&frame) + direction * rig.muzzle_distance(&frame);
                let muzzle = app
                    .world_mut()
                    .query::<(&Name, &GlobalTransform)>()
                    .iter(app.world())
                    .find(|(name, _)| name.as_str() == definition.muzzle_node)
                    .map(|(_, transform)| *transform)
                    .expect("configured muzzle missing");
                assert!(
                    muzzle.translation().abs_diff_eq(expected, 1e-4),
                    "animation or hierarchy displaced the aimed muzzle: {}",
                    model.scene
                );
                assert!(
                    (muzzle.rotation() * Vec3::NEG_Z).abs_diff_eq(direction, 1e-4),
                    "muzzle axis does not follow the beam: {}",
                    model.scene
                );
            }
            if model.animation_speed.is_some() {
                assert!(
                    app.world_mut()
                        .query::<(&Name, &Transform)>()
                        .iter(app.world())
                        .filter(
                            |(name, _)| ![&definition.yaw_node, &definition.pitch_node, &definition.muzzle_node]
                                .iter()
                                .any(|aim| name.as_str() == aim.as_str())
                        )
                        .any(
                            |(name, transform)| before.iter().any(|(old_name, old)| old_name == name.as_str()
                                && (old.rotation.angle_between(transform.rotation) > 0.01
                                    || old.translation.distance(transform.translation) > 0.001))
                        ),
                    "model animation did not move any flight hardware: {}",
                    model.scene
                );
            }
        }
    }

    #[test]
    fn articulated_barrel_tracks_every_quadrant_and_elevation() {
        for target in [
            Vec3::NEG_Z,
            Vec3::Z,
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::new(3.0, 4.0, -2.0),
            Vec3::new(-3.0, -4.0, 2.0),
        ] {
            let direction = target.normalize();
            let (yaw, pitch) = aim_rotations(direction);
            assert!((yaw * pitch * Vec3::NEG_Z).abs_diff_eq(direction, 1e-5));
        }
    }

    #[test]
    fn aiming_respects_rotated_bases_and_scaled_models() {
        let mut world = World::new();
        let yaw = world.spawn((Transform::default(), AimJointMarker)).id();
        let pitch = world.spawn((Transform::default(), AimJointMarker)).id();
        let actor = Transform::from_xyz(8.0, 2.0, -3.0).with_rotation(Quat::from_rotation_y(1.2));
        let model = world
            .spawn(Transform::from_xyz(0.0, -1.45, 0.0).with_scale(Vec3::splat(2.0)))
            .id();
        let rig = AimRig {
            yaw,
            pitch,
            parents: vec![model],
            pivot: Vec3::new(0.0, 1.45, 0.0),
            muzzle_distance: 0.572,
        };
        let direction = Vec3::new(-2.0, 1.0, 3.0).normalize();
        let mut state = world.query_filtered::<&mut Transform, With<AimJointMarker>>();
        let frame = rig
            .frame(&actor, |entity| world.get::<Transform>(entity).copied())
            .expect("aim parent missing");
        rig.aim(&frame, direction, &mut state.query_mut(&mut world));
        let yaw = world.get::<Transform>(yaw).expect("yaw transform missing");
        let pitch = world.get::<Transform>(pitch).expect("pitch transform missing");
        let forward = actor.rotation * yaw.rotation * pitch.rotation * Vec3::NEG_Z;
        assert!(forward.abs_diff_eq(direction, 1e-5));
        assert!(rig.pivot(&frame).abs_diff_eq(Vec3::new(8.0, 3.45, -3.0), 1e-5));
        assert!((rig.muzzle_distance(&frame) - 1.144).abs() < 1e-5);
    }

    #[test]
    fn beam_follows_the_muzzle_without_turning_the_base_or_bypassing_cover() {
        let source: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let gameplay: GameplayConfig = serde_json::from_value(serde_json::json!({
            "player": source["player"],
            "projectiles": source["weapons"]["projectiles"],
            "missiles": source["weapons"]["missiles"],
            "portals": source["weapons"]["portals"],
            "actors": source["actors"]["kinds"],
        }))
        .expect("client gameplay config is invalid");
        let target_height = gameplay.player.physics().hitbox.center_y_offset();
        let assets = AssetSet::load_default().expect("client assets rejected");
        let model = assets.actor_model("turret");
        let model_transform = Transform::from_xyz(0.0, model.y_offset, 0.0);
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<ActorMap>()
            .init_resource::<PlayerMap>()
            .init_resource::<PlateState>()
            .insert_resource(gameplay)
            .insert_resource(CollisionWorld::from_map_layout(
                &MapLayout::default(),
                &BarrierKindTable::default(),
            ))
            .add_systems(
                Update,
                (characters_visual_turn_system, laser_beam_update_system).chain(),
            );
        let yaw = app.world_mut().spawn((Transform::default(), AimJointMarker)).id();
        let pitch = app.world_mut().spawn((Transform::default(), AimJointMarker)).id();
        let base = Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(1.2));
        let model_entity = app.world_mut().spawn(model_transform).id();
        let actor = app
            .world_mut()
            .spawn((
                base,
                ActorMarker,
                FixedFacingMarker,
                FaceYaw(0.0),
                AimRig {
                    yaw,
                    pitch,
                    parents: vec![model_entity],
                    pivot: Vec3::new(0.0, 1.45, 0.0),
                    muzzle_distance: 0.572,
                },
            ))
            .id();
        app.world_mut().resource_mut::<ActorMap>().insert(
            ActorId(1),
            ActorInfo {
                entity: actor,
                kind: "turret".into(),
                anchor: None,
                beam: Default::default(),
            },
        );
        let target = app.world_mut().spawn(Transform::default()).id();
        let player = Player::new(
            "Player".into(),
            Position::default(),
            PlayerMoveIntent::default(),
            0.0,
            0,
            Health(100.0),
        );
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .insert(PlayerId(1), PlayerInfo::from_snapshot(target, &player, 0));
        let beam = app
            .world_mut()
            .spawn((
                LaserBeam {
                    actor: ActorId(1),
                    target: PlayerId(1),
                    started_tick: 0,
                    wander_width_fraction: 0.0,
                    wander_height_fraction: 0.0,
                    aim_height_fraction: 0.5,
                },
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        for (frame_index, aim) in [
            Vec3::new(2.0, 4.0, 3.0),
            Vec3::new(-3.0, 0.5, 2.0),
            Vec3::new(0.0, 1.45, -4.0),
        ]
        .into_iter()
        .enumerate()
        {
            let mut parent = app
                .world_mut()
                .get_mut::<Transform>(model_entity)
                .expect("aim parent missing");
            parent.translation = Vec3::new(0.03 * frame_index as f32, 0.05 * frame_index as f32, 0.0);
            parent.rotation = Quat::from_rotation_x(0.04 * frame_index as f32);
            app.world_mut()
                .get_mut::<Transform>(target)
                .expect("target transform missing")
                .translation = aim - Vec3::Y * target_height;
            app.update();
            let world = app.world();
            let actual_base = world.get::<Transform>(actor).expect("base transform missing");
            assert_eq!(*actual_base, base);
            let yaw = world.get::<Transform>(yaw).expect("yaw transform missing");
            let pitch = world.get::<Transform>(pitch).expect("pitch transform missing");
            let rig = world.get::<AimRig>(actor).expect("aim rig missing");
            let frame = rig
                .frame(&base, |entity| world.get::<Transform>(entity).copied())
                .expect("aim parent missing");
            let forward = frame.rotation() * yaw.rotation * pitch.rotation * Vec3::NEG_Z;
            let muzzle = rig.pivot(&frame) + forward * rig.muzzle_distance(&frame);
            let beam = world.get::<Transform>(beam).expect("beam transform missing");
            assert!(beam.transform_point(Vec3::NEG_Y * 0.5).abs_diff_eq(muzzle, 1e-5));
            assert!(beam.transform_point(Vec3::Y * 0.5).abs_diff_eq(aim, 1e-5));
        }
        let layout = MapLayout {
            walls: vec![Wall {
                x1: -2.0,
                z1: -0.3,
                x2: 2.0,
                z2: -0.3,
                width: 0.1,
                y: 0.0,
                height: 3.0,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        };
        app.insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()));
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(beam).expect("beam visibility missing"),
            Visibility::Hidden
        );
        app.world_mut().resource_mut::<PlayerMap>().remove(&PlayerId(1));
        app.update();
        assert!(app.world().get_entity(beam).is_err());
    }
}
