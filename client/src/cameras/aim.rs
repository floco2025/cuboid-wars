use super::{CameraAim, CameraViewMode, MainCameraMarker};
use crate::{
    actors::ActorMap,
    players::{LocalPlayerMarker, MyPlayerId, PlayerMap},
};
use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, ball_character_hit},
    protocol::{BarrierKindId, FaceYaw, PlateState, Position},
};

const AIM_DISTANCE: f32 = 1000.0;

pub fn camera_aim_system(
    mut aim: ResMut<CameraAim>,
    view: Res<CameraViewMode>,
    camera: Query<&Transform, With<MainCameraMarker>>,
    local_player: Query<(&Position, &FaceYaw), With<LocalPlayerMarker>>,
    characters: Query<(&Position, &FaceYaw)>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    me: Res<MyPlayerId>,
    world: Res<CollisionWorld>,
    config: Res<GameplayConfig>,
    plates: Res<PlateState>,
) {
    let Ok((position, face)) = local_player.single() else {
        return;
    };
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = Vec3::new(position.x, position.y + config.player.eye_height(), position.z);
    let direction = if view.is_top_down() {
        direction_from_yaw_pitch(face.0, 0.0)
    } else if view.is_first_person() {
        *camera.forward()
    } else {
        let candidates = players
            .iter()
            .filter(|(id, _)| **id != me.0)
            .filter_map(|(_, info)| {
                let (pos, yaw) = characters.get(info.entity).ok()?;
                Some((*pos, yaw.0, config.player.physics()))
            })
            .chain(actors.iter().filter_map(|(_, info)| {
                let (pos, yaw) = characters.get(info.entity).ok()?;
                Some((*pos, yaw.0, config.expect_actor(&info.kind).physics()))
            }));
        third_person_aim(
            &world,
            camera.translation,
            *camera.forward(),
            eye,
            &plates.open_barrier_kinds,
            candidates,
        )
    };
    *aim = CameraAim {
        origin: eye,
        direction,
        yaw: direction.x.atan2(direction.z),
        pitch: direction.y.clamp(-1.0, 1.0).asin(),
    };
}

fn third_person_aim(
    world: &CollisionWorld,
    camera: Vec3,
    forward: Vec3,
    eye: Vec3,
    open_barriers: &[BarrierKindId],
    candidates: impl Iterator<Item = (Position, f32, CharacterPhysicsConfig)>,
) -> Vec3 {
    // Only converge on targets in front of the shooter's plane, never on cover behind their shoulder.
    let start = camera + forward * (eye - camera).dot(forward).max(0.0);
    let mut distance = world
        .attack_surface_along_ray(start, forward, AIM_DISTANCE, open_barriers)
        .map_or(AIM_DISTANCE, |hit| hit.point.distance(start));
    for (pos, yaw, physics) in candidates {
        if let Some(hit) = ball_character_hit(&start.into(), forward * distance, 0.001, 1.0, &pos, yaw, physics) {
            distance *= hit.time_of_impact;
        }
    }
    (start + forward * distance - eye).try_normalize().unwrap_or(forward)
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{
        config::{HitboxConfig, MovementColliderConfig},
        protocol::{BarrierKindTable, CarrierId, MapLayout, Wall},
    };
    #[test]
    fn shoulder_camera_converges_on_near_character() {
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let physics = CharacterPhysicsConfig {
            movement_collider: MovementColliderConfig {
                radius: 0.3,
                height: 1.8,
            },
            hitbox: HitboxConfig {
                width: 0.6,
                depth: 0.6,
                height: 1.8,
                bottom_offset: 0.0,
            },
        };
        let eye = Vec3::new(0.0, 1.4, 0.0);
        let direction = third_person_aim(
            &world,
            Vec3::new(0.45, 1.4, 4.0),
            Vec3::NEG_Z,
            eye,
            &[],
            [(Position::from(Vec3::new(0.45, 0.0, -3.0)), 0.0, physics)].into_iter(),
        );
        assert!(direction.x > 0.1 && direction.z < -0.9);
        assert!(
            ball_character_hit(
                &eye.into(),
                direction * 10.0,
                0.01,
                1.0,
                &Position::from(Vec3::new(0.45, 0.0, -3.0)),
                0.0,
                physics
            )
            .is_some()
        );
    }

    #[test]
    fn shoulder_visibility_does_not_bypass_muzzle_cover() {
        let world = CollisionWorld::from_map_layout(
            &MapLayout {
                walls: vec![Wall {
                    x1: -1.0,
                    z1: -1.0,
                    x2: 0.3,
                    z2: -1.0,
                    width: 0.2,
                    y: 0.0,
                    height: 3.0,
                    level: 0,
                    carrier: CarrierId::WORLD,
                }],
                ..default()
            },
            &BarrierKindTable::default(),
        );
        let eye = Vec3::new(0.0, 1.4, 0.0);
        let camera = Vec3::new(0.45, 1.4, 4.0);
        assert!(world.world_surface_along_ray(camera, Vec3::NEG_Z, 20.0).is_none());
        let direction = third_person_aim(&world, camera, Vec3::NEG_Z, eye, &[], std::iter::empty());
        assert!(world.attack_surface_along_ray(eye, direction, 20.0, &[]).is_some());
        assert!(!world.projectile_path_clear(eye, direction * 2.0, 0.05, &[]));
    }
}
