use std::time::Duration;

use bevy::math::Vec3;
use common::{
    math::PHYSICS_EPSILON,
    physics::{CollisionWorld, PortalSet},
    protocol::{FieldId, Position},
};

use super::motion::FieldImpact;
use super::{PROJECTILE_EVENT_LIMIT, ProjectileEvent, ProjectileMotion, SurfaceBounce, earliest_projectile_event};
use crate::characters::BallCharacterHit;

pub struct ProjectileEnvironment<'a> {
    pub delta: Duration,
    pub gravity: f32,
    pub collision_world: &'a CollisionWorld,
    pub portals: &'a PortalSet,
    pub open_fields: &'a [FieldId],
}

pub enum ProjectileFlightEvent<T> {
    Expired,
    Bounce {
        bounce: SurfaceBounce,
        speed_before: f32,
        velocity: Vec3,
    },
    Portal {
        entry: Vec3,
        exit: Vec3,
    },
    Hit {
        target: T,
        hit: BallCharacterHit,
        position: Vec3,
        velocity: Vec3,
    },
    Field {
        impact: FieldImpact,
        speed: f32,
    },
}

pub struct ProjectileStep<T> {
    pub position: Position,
    pub previous_position: Position,
    pub terminated: bool,
    pub events: Vec<ProjectileFlightEvent<T>>,
}

// The rendered client and experiments use this same tick. Character queries
// supply their own observed bodies; presentation and reliable hit reports stay
// with the caller, so flight never needs renderer assets or server authority.
pub fn step_projectile<T: Copy>(
    start: Position,
    projectile: &mut ProjectileMotion,
    world: &ProjectileEnvironment<'_>,
    mut overlaps_shooter: impl FnMut(&ProjectileMotion, &Position) -> bool,
    mut character_hit: impl FnMut(&ProjectileMotion, &Position, f32) -> Option<(T, BallCharacterHit)>,
) -> ProjectileStep<T> {
    let mut result = ProjectileStep {
        position: start,
        previous_position: start,
        terminated: false,
        events: Vec::new(),
    };
    projectile.lifetime.tick(world.delta);
    if projectile.lifetime.is_finished() {
        result.terminated = true;
        result.events.push(ProjectileFlightEvent::Expired);
        return result;
    }
    let delta = world.delta.as_secs_f32();
    projectile.apply_gravity(delta, world.gravity);
    projectile.apply_drag(delta);
    let mut remaining = delta;
    for _ in 0..PROJECTILE_EVENT_LIMIT {
        if remaining <= PHYSICS_EPSILON {
            break;
        }
        if !projectile.left_shooter && !overlaps_shooter(projectile, &result.position) {
            projectile.left_shooter = true;
        }
        let hit = character_hit(projectile, &result.position, remaining);
        let field = projectile.field_collision_t(&result.position, remaining, world.collision_world, world.open_fields);
        let hop = world.portals.projectile_hop(
            result.position.into(),
            projectile.velocity,
            remaining,
            projectile.radius,
            delta,
        );
        let excluded = hop.map_or(&[][..], |hop| hop.entry_backing);
        let surface = projectile.surface_collision_t(&result.position, remaining, world.collision_world, excluded);
        match earliest_projectile_event(
            hit.map(|(_, hit)| hit.time_of_impact),
            field,
            surface,
            hop.map(|hop| hop.t),
        ) {
            ProjectileEvent::Field => {
                let impact = projectile
                    .terminate_at_field(&result.position, remaining, world.collision_world, world.open_fields)
                    .expect("field event missing its collision");
                result.position = impact.point.into();
                result.events.push(ProjectileFlightEvent::Field {
                    impact,
                    speed: projectile.velocity.length(),
                });
                result.terminated = true;
                break;
            }
            ProjectileEvent::Surface => {
                let speed_before = projectile.velocity.length();
                let bounce = projectile
                    .bounce_at_world_surface(&result.position, remaining, world.collision_world, excluded)
                    .expect("surface event missing its collision");
                result.position = bounce.position;
                remaining = bounce.remaining_delta;
                result.events.push(ProjectileFlightEvent::Bounce {
                    bounce,
                    speed_before,
                    velocity: projectile.velocity,
                });
            }
            ProjectileEvent::Portal => {
                let hop = hop.expect("portal event missing its crossing");
                projectile.velocity = hop.exit_velocity;
                result.position = hop.exit_pos.into();
                result.previous_position = result.position;
                remaining *= 1.0 - hop.t;
                result.events.push(ProjectileFlightEvent::Portal {
                    entry: hop.entry_point,
                    exit: hop.exit_pos,
                });
            }
            ProjectileEvent::Hit => {
                let (target, hit) = hit.expect("character event missing its hit");
                let position = Vec3::from(result.position) + projectile.velocity * remaining * hit.time_of_impact;
                result.position = position.into();
                result.events.push(ProjectileFlightEvent::Hit {
                    target,
                    hit,
                    position,
                    velocity: projectile.velocity,
                });
                result.terminated = true;
                break;
            }
            ProjectileEvent::Fly => {
                result.position = (Vec3::from(result.position) + projectile.velocity * remaining).into();
                break;
            }
        }
    }
    result
}
