use bevy::prelude::*;

use crate::systems::network::ServerReconciliation;
use common::{
    constants::{ACTOR_SPEED, PLAYER_HEIGHT, UPDATE_BROADCAST_INTERVAL},
    markers::ActorMarker,
    physics::{CharacterVerticalMotion, CollisionWorld, step_character_movement},
    protocol::{CharacterMoveIntent, Position},
};

type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static CharacterMoveIntent,
        &'static mut CharacterVerticalMotion,
        Option<&'static mut ServerReconciliation>,
    ),
    With<ActorMarker>,
>;

pub fn actors_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_world: Option<Res<CollisionWorld>>,
    mut query: ActorMovementQuery,
) {
    let delta = time.delta_secs();

    for (entity, mut pos, move_intent, mut motion, mut recon_option) in &mut query {
        let h_vel = move_intent.to_horizontal_velocity(ACTOR_SPEED);
        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
            let correction_time = (recon.rtt * 3.0).max(UPDATE_BROADCAST_INTERVAL);
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
            let total_delta = server_pos - Vec3::from(recon.client_pos);

            if total_delta.x.abs() >= 5.0 || total_delta.y.abs() >= 1.0 || total_delta.z.abs() >= 5.0 {
                *pos = recon.server_pos;
                motion.vertical_velocity = recon.server_velocity.y;
                commands.entity(entity).remove::<ServerReconciliation>();
                continue;
            }

            Position {
                x: h_vel.x.mul_add(delta, pos.x)
                    + total_delta.x * delta * correction_factor / UPDATE_BROADCAST_INTERVAL,
                y: pos.y,
                z: h_vel.z.mul_add(delta, pos.z)
                    + total_delta.z * delta * correction_factor / UPDATE_BROADCAST_INTERVAL,
            }
        } else {
            Position {
                x: h_vel.x.mul_add(delta, pos.x),
                y: pos.y,
                z: h_vel.z.mul_add(delta, pos.z),
            }
        };

        if let Some(collision_world) = collision_world.as_ref() {
            let step =
                step_character_movement(&pos, &motion, collision_world, false, target_pos.x, target_pos.z, delta);
            target_pos = step.position;
            motion.vertical_velocity = step.vertical_velocity;
        }

        *pos = target_pos;
    }
}

pub fn actors_transform_sync_system(mut query: Query<(&Position, &mut Transform), With<ActorMarker>>) {
    for (pos, mut transform) in &mut query {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y + PLAYER_HEIGHT / 2.0;
        transform.translation.z = pos.z;
    }
}
