use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::config::AimRigDef;

#[derive(Component)]
pub struct FixedFacingMarker;

#[derive(Component)]
pub struct AimJointMarker;

#[derive(Component)]
pub struct AimRig {
    pub(super) yaw: Entity,
    pub(super) pitch: Entity,
    pub(super) parents: Vec<Entity>,
    pub(super) pivot: Vec3,
    pub(super) muzzle_distance: f32,
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
    // `pivot` is the yaw node's translation and `muzzle_distance` the
    // muzzle's local offset, which together locate the muzzle only when the
    // pitch node turns about the yaw origin and the muzzle hangs straight
    // off the pitch node.
    let rig_is_wired = parents.get(pitch).is_ok_and(|parent| parent.parent() == yaw)
        && parents.get(muzzle).is_ok_and(|parent| parent.parent() == pitch)
        && transforms
            .get(pitch)
            .is_ok_and(|transform| transform.translation.abs_diff_eq(Vec3::ZERO, 1e-3));
    if !rig_is_wired {
        error!(
            ?definition,
            "aim rig pitch node must sit at the yaw node's origin with the muzzle as its direct child"
        );
        return;
    }
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

pub(super) fn aim_rotations(direction: Vec3) -> (Quat, Quat) {
    (
        Quat::from_rotation_y((-direction.x).atan2(-direction.z)),
        Quat::from_rotation_x(direction.y.atan2(direction.xz().length())),
    )
}
