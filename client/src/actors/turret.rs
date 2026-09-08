use bevy::{prelude::*, world_serialization::WorldInstanceReady};

#[derive(Component)]
pub struct TurretMarker;

#[derive(Component)]
pub struct TurretJointMarker;

#[derive(Component)]
pub struct TurretRig {
    yaw: Entity,
    pitch: Entity,
    model: Transform,
    pivot: Vec3,
    muzzle_distance: f32,
}

pub fn turret_rig_setup_system(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
) {
    let find = |name| {
        children
            .iter_descendants(ready.entity)
            .find(|entity| names.get(*entity).is_ok_and(|node| node.as_str() == name))
    };
    let (Some(yaw), Some(pitch), Some(muzzle)) = (find("TurretYaw"), find("TurretPitch"), find("TurretMuzzle")) else {
        error!("turret model is missing TurretYaw, TurretPitch, or TurretMuzzle");
        return;
    };
    let actor = parents.get(ready.entity).expect("turret model parent missing").parent();
    commands.entity(actor).insert(TurretRig {
        yaw,
        pitch,
        model: *transforms.get(ready.entity).expect("turret model transform missing"),
        pivot: transforms.get(yaw).expect("turret yaw transform missing").translation,
        muzzle_distance: transforms
            .get(muzzle)
            .expect("turret muzzle transform missing")
            .translation
            .length(),
    });
    commands.entity(yaw).insert(TurretJointMarker);
    commands.entity(pitch).insert(TurretJointMarker);
}

impl TurretRig {
    pub fn pivot(&self, actor: &Transform) -> Vec3 {
        actor.transform_point(self.model.transform_point(self.pivot))
    }

    pub fn muzzle_distance(&self, actor: &Transform) -> f32 {
        self.muzzle_distance * self.model.scale.z * actor.scale.z
    }

    pub fn aim(&self, actor: &Transform, direction: Vec3, joints: &mut Query<&mut Transform, With<TurretJointMarker>>) {
        let local = (actor.rotation * self.model.rotation).inverse() * direction;
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
        characters::characters_visual_turn_system,
        config::AssetSet,
        players::{PlayerInfo, PlayerMap},
        vfx::{LaserBeam, laser_beam_update_system},
    };
    use common::{
        config::GameplayConfig,
        physics::CollisionWorld,
        protocol::{
            ActorId, ActorMarker, BarrierKindTable, CarrierId, FaceYaw, Health, MapLayout, PlateState, Player,
            PlayerId, PlayerMoveIntent, Position, Wall,
        },
    };

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
        let yaw = world.spawn((Transform::default(), TurretJointMarker)).id();
        let pitch = world.spawn((Transform::default(), TurretJointMarker)).id();
        let actor = Transform::from_xyz(8.0, 2.0, -3.0).with_rotation(Quat::from_rotation_y(1.2));
        let rig = TurretRig {
            yaw,
            pitch,
            model: Transform::from_xyz(0.0, -1.45, 0.0).with_scale(Vec3::splat(2.0)),
            pivot: Vec3::new(0.0, 1.45, 0.0),
            muzzle_distance: 0.572,
        };
        let direction = Vec3::new(-2.0, 1.0, 3.0).normalize();
        let mut state = world.query_filtered::<&mut Transform, With<TurretJointMarker>>();
        rig.aim(&actor, direction, &mut state.query_mut(&mut world));
        let yaw = world.get::<Transform>(yaw).expect("yaw transform missing");
        let pitch = world.get::<Transform>(pitch).expect("pitch transform missing");
        let forward = actor.rotation * yaw.rotation * pitch.rotation * Vec3::NEG_Z;
        assert!(forward.abs_diff_eq(direction, 1e-5));
        assert!(rig.pivot(&actor).abs_diff_eq(Vec3::new(8.0, 3.45, -3.0), 1e-5));
        assert!((rig.muzzle_distance(&actor) - 1.144).abs() < 1e-5);
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
        let yaw = app.world_mut().spawn((Transform::default(), TurretJointMarker)).id();
        let pitch = app.world_mut().spawn((Transform::default(), TurretJointMarker)).id();
        let base = Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(1.2));
        let actor = app
            .world_mut()
            .spawn((
                base,
                ActorMarker,
                TurretMarker,
                FaceYaw(0.0),
                TurretRig {
                    yaw,
                    pitch,
                    model: model_transform,
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
        for aim in [
            Vec3::new(2.0, 4.0, 3.0),
            Vec3::new(-3.0, 0.5, 2.0),
            Vec3::new(0.0, 1.45, -4.0),
        ] {
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
            let forward = base.rotation * yaw.rotation * pitch.rotation * Vec3::NEG_Z;
            let muzzle = Vec3::new(0.0, 1.45, 0.0) + forward * 0.572;
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
