use super::{CameraAim, CameraViewMode, MainCameraMarker};
use crate::{
    actors::ActorMap,
    characters::ball_character_hit,
    constants::CROSSHAIR_THIRD_PERSON_HEIGHT,
    players::{LocalPlayerMarker, MyPlayerId, PlayerMap},
};
use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    physics::CollisionWorld,
    protocol::{BarrierId, FaceYaw, PlateState, Position},
};

const AIM_DISTANCE: f32 = 1000.0;

pub fn camera_aim_system(
    mut aim: ResMut<CameraAim>,
    view: Res<CameraViewMode>,
    camera: Query<(&Transform, &Projection), With<MainCameraMarker>>,
    local_player: Query<&Position, With<LocalPlayerMarker>>,
    characters: Query<(&Position, &FaceYaw)>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    me: Res<MyPlayerId>,
    world: Res<CollisionWorld>,
    config: Res<GameplayConfig>,
    plates: Res<PlateState>,
) {
    let Ok(position) = local_player.single() else {
        return;
    };
    let Ok((camera, Projection::Perspective(projection))) = camera.single() else {
        return;
    };
    let eye = Vec3::new(position.x, position.y + config.player.eye_height(), position.z);
    let crosshair_height_offset = if view.is_first_person() {
        0.0
    } else {
        CROSSHAIR_THIRD_PERSON_HEIGHT
    };
    let direction = if view.is_first_person() {
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
            crosshair_direction(camera, projection, crosshair_height_offset),
            eye,
            &plates.open_barriers,
            candidates,
        )
    };
    *aim = CameraAim {
        origin: eye,
        direction,
        yaw: direction.x.atan2(direction.z),
        pitch: direction.y.clamp(-1.0, 1.0).asin(),
        crosshair_height_offset,
    };
}

fn crosshair_direction(camera: &Transform, projection: &PerspectiveProjection, height_offset: f32) -> Vec3 {
    let rise = 2.0 * height_offset * (projection.fov * 0.5).tan();
    (*camera.forward() + *camera.up() * rise).normalize()
}

fn third_person_aim(
    world: &CollisionWorld,
    camera: Vec3,
    forward: Vec3,
    eye: Vec3,
    open_barriers: &[BarrierId],
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
#[path = "tests/aim.rs"]
mod tests;
