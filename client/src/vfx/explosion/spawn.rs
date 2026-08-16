use super::animation::{ExplosionLight, ExplosionPulse};
use super::assets::{ExplosionAssets, ExplosionRadii, shockwave_mesh};
use super::particles::{ExplosionVfxBudget, SurfacePlane};
use super::scorch::spawn_scorch_mark;
use super::scorch::{
    ScorchPlacement, ScorchStyle, surface_cross_section_diameter, wall_scorch_diameter, wall_scorch_placements,
};
use super::shards::spawn_shard_cloud;
use super::smoke::spawn_smoke_cloud;
use crate::{config::ExplosionVfxConfig, constants::*};
use bevy::{light::NotShadowCaster, prelude::*};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{MapLayout, Position},
};
use rand::rng;

#[derive(Clone, Copy)]
struct ExplosionSpec {
    center: Vec3,
    ground_y: f32,
    fireball_diameter: f32,
    blast_radius: Option<f32>,
}

// The world plumbing every explosion spawn draws from, bundled so the death
// handlers thread one context instead of seven parameters.
pub struct ExplosionSpawnCtx<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub budget: &'a mut ExplosionVfxBudget,
    pub explosion_assets: &'a ExplosionAssets,
    pub gameplay_config: &'a GameplayConfig,
    pub collision_world: Option<&'a CollisionWorld>,
    pub map_layout: Option<&'a MapLayout>,
}

pub fn spawn_actor_explosion(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    radii: &ExplosionRadii,
    actor_kind: &str,
    pos: Position,
) {
    let actor_physics = ctx
        .gameplay_config
        .actor(actor_kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics();
    let blast_radius = radii.actors.get(actor_kind).copied();
    let fireball_diameter = blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
        2.0 * radius * ctx.explosion_assets.config.fireball.blast_diameter_factor
    });
    spawn_explosion(
        commands,
        ctx.meshes,
        ctx.materials,
        ctx.budget,
        ctx.explosion_assets,
        ExplosionSpec {
            center: Vec3::new(pos.x, actor_physics.collider_center_y(pos.y), pos.z),
            ground_y: pos.y,
            fireball_diameter,
            blast_radius,
        },
        ctx.collision_world,
        ctx.map_layout,
    );
}

pub fn spawn_player_explosion(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    radii: &ExplosionRadii,
    pos: Position,
) {
    let player_physics = ctx.gameplay_config.player.physics();
    let blast_radius = (radii.player > 0.0).then_some(radii.player);
    spawn_explosion(
        commands,
        ctx.meshes,
        ctx.materials,
        ctx.budget,
        ctx.explosion_assets,
        ExplosionSpec {
            center: Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z),
            ground_y: pos.y,
            fireball_diameter: blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
                2.0 * radius * ctx.explosion_assets.config.fireball.blast_diameter_factor
            }),
            blast_radius,
        },
        ctx.collision_world,
        ctx.map_layout,
    );
}

// A missile detonation: the blast origin is the detonation point itself (no
// character body), radius from the shared missiles config.
pub fn spawn_missile_explosion(commands: &mut Commands, ctx: &mut ExplosionSpawnCtx, pos: Position) {
    let blast_radius = ctx.gameplay_config.missiles.blast_radius;
    spawn_explosion(
        commands,
        ctx.meshes,
        ctx.materials,
        ctx.budget,
        ctx.explosion_assets,
        ExplosionSpec {
            center: Vec3::from(pos),
            ground_y: pos.y,
            fireball_diameter: 2.0 * blast_radius * ctx.explosion_assets.config.fireball.blast_diameter_factor,
            blast_radius: Some(blast_radius),
        },
        ctx.collision_world,
        ctx.map_layout,
    );
}

// Six layers: fireball flash, ground shockwave ring, scorch mark, debris
// shard burst, smoke, and a fading point light. `center` is the blast origin
// (collider center); `ground_y` anchors the ring at the victim's feet.
fn spawn_explosion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    spec: ExplosionSpec,
    collision_world: Option<&CollisionWorld>,
    map_layout: Option<&MapLayout>,
) {
    let config = &explosion_assets.config;
    let ExplosionSpec {
        center,
        ground_y,
        fireball_diameter,
        blast_radius,
    } = spec;
    // `None` = cosmetic burst with no area damage (unknown-kind fallback):
    // shards and light size off the fireball, and no ring is spawned — a
    // ring always marks a real danger area.
    let reach_radius = blast_radius.unwrap_or(fireball_diameter * 0.5);
    let standing_distance = (center.y - ground_y).max(0.0) + EXPLOSION_SCORCH_SURFACE_OFFSET;
    let ground_surface =
        collision_world.and_then(|world| world.ground_surface_below(center, reach_radius.max(standing_distance)));

    // Start pulses at a tiny scale, not zero — a degenerate scale inverts to
    // NaN normals for one frame.
    let fireball_material = materials.add(explosion_assets.fireball_template.clone());
    commands.spawn((
        Mesh3d(explosion_assets.fireball_mesh.clone()),
        MeshMaterial3d(fireball_material.clone()),
        NotShadowCaster,
        Transform::from_translation(center).with_scale(Vec3::splat(0.01)),
        ExplosionPulse {
            elapsed: 0.0,
            lifetime: config.base_duration_secs * EXPLOSION_FIREBALL_LIFETIME_FACTOR,
            max_scale: fireball_diameter,
            start_alpha: EXPLOSION_FIREBALL_START_ALPHA,
            base_emissive: explosion_assets.fireball_template.emissive,
            material: fireball_material,
        },
    ));

    if let (Some(blast_radius), Some(surface)) = (blast_radius, ground_surface) {
        let ring_material = materials.add(explosion_assets.ring_template.clone());
        let ring_mesh = meshes.add(shockwave_mesh(
            collision_world,
            center,
            surface.normal,
            blast_radius * EXPLOSION_SHOCKWAVE_DIAMETER_FACTOR,
        ));
        commands.spawn((
            Mesh3d(ring_mesh),
            MeshMaterial3d(ring_material.clone()),
            NotShadowCaster,
            Transform {
                translation: surface.point + surface.normal * EXPLOSION_SHOCKWAVE_SURFACE_OFFSET,
                rotation: Quat::from_rotation_arc(Vec3::Y, surface.normal),
                scale: Vec3::splat(0.01),
            },
            ExplosionPulse {
                elapsed: 0.0,
                lifetime: config.base_duration_secs * EXPLOSION_SHOCKWAVE_LIFETIME_FACTOR,
                max_scale: 2.0 * blast_radius * EXPLOSION_SHOCKWAVE_DIAMETER_FACTOR,
                start_alpha: EXPLOSION_SHOCKWAVE_START_ALPHA,
                base_emissive: explosion_assets.ring_template.emissive,
                material: ring_material,
            },
        ));
    }

    let mut rng = rng();
    let scorch_diameter = 2.0 * reach_radius * config.scorches.blast_diameter_factor;
    let scorch_radius = scorch_diameter * 0.5;
    let scorch_style = ScorchStyle::random(explosion_assets.scorch_meshes.len(), &mut rng);
    if let Some(surface) = ground_surface {
        let distance = center.distance(surface.point);
        if let Some(diameter) = surface_cross_section_diameter(scorch_radius, distance) {
            spawn_scorch_mark(
                commands,
                materials,
                budget,
                explosion_assets,
                ScorchPlacement::on_surface(surface, diameter, scorch_style),
                scorch_style,
                EXPLOSION_SCORCH_MAX_ACTIVE,
            );
        }
    }
    if let Some(map_layout) = map_layout {
        for placement in wall_scorch_placements(
            map_layout,
            center,
            scorch_radius,
            EXPLOSION_SCORCH_WALL_REACH_FACTOR,
            scorch_style,
        ) {
            spawn_scorch_mark(
                commands,
                materials,
                budget,
                explosion_assets,
                placement,
                scorch_style,
                EXPLOSION_SCORCH_MAX_ACTIVE,
            );
        }
    } else if let Some(world) = collision_world {
        let wall_probe_distance = scorch_radius * EXPLOSION_SCORCH_WALL_REACH_FACTOR;
        for direction in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            if let Some(surface) = world.wall_surface_along_ray(center, direction, wall_probe_distance)
                && let Some(diameter) = wall_scorch_diameter(
                    scorch_radius,
                    center.distance(surface.point),
                    EXPLOSION_SCORCH_WALL_REACH_FACTOR,
                )
            {
                spawn_scorch_mark(
                    commands,
                    materials,
                    budget,
                    explosion_assets,
                    ScorchPlacement::on_surface(surface, diameter, scorch_style),
                    scorch_style,
                    EXPLOSION_SCORCH_MAX_ACTIVE,
                );
            }
        }
    }

    let ground_plane = ground_surface.map(|surface| SurfacePlane::from_hit(surface, center, reach_radius * 0.75));
    spawn_shard_cloud(
        commands,
        meshes,
        explosion_assets.shard_material.clone(),
        budget,
        collision_world,
        ground_plane,
        center,
        reach_radius,
        shard_count(reach_radius, config),
        config,
        &mut rng,
    );
    spawn_smoke_cloud(
        commands,
        meshes,
        explosion_assets.smoke_material.clone(),
        budget,
        center,
        reach_radius,
        smoke_count(reach_radius, config),
        config,
        &mut rng,
    );

    // Own entity: the light fades over the full master lifetime, outliving
    // the shorter fireball flash.
    if budget.reserve_light(EXPLOSION_LIGHT_MAX_ACTIVE) {
        let range = (reach_radius * EXPLOSION_LIGHT_RANGE_PER_RADIUS).max(EXPLOSION_LIGHT_MIN_RANGE);
        let intensity = config.light.intensity_lumens;
        commands.spawn((
            PointLight {
                color: EXPLOSION_LIGHT_COLOR,
                intensity,
                range,
                radius: 1.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_translation(center),
            ExplosionLight {
                elapsed: 0.0,
                lifetime: config.base_duration_secs,
                intensity,
                range,
            },
        ));
    }
}

fn shard_count(reach_radius: f32, config: &ExplosionVfxConfig) -> usize {
    scaled_particle_count(
        reach_radius,
        config.shards.count_per_radius_meter,
        EXPLOSION_REFERENCE_SHARDS_PER_METER,
        EXPLOSION_SHARD_MIN_COUNT,
        EXPLOSION_SHARD_MAX_COUNT,
    )
}

fn smoke_count(reach_radius: f32, config: &ExplosionVfxConfig) -> usize {
    scaled_particle_count(
        reach_radius,
        config.smoke.count_per_radius_meter,
        EXPLOSION_REFERENCE_SMOKE_PARTICLES_PER_METER,
        EXPLOSION_SMOKE_MIN_COUNT,
        EXPLOSION_SMOKE_MAX_COUNT,
    )
}

fn scaled_particle_count(reach_radius: f32, density: f32, default_density: f32, min: usize, max: usize) -> usize {
    if density <= 0.0 {
        return 0;
    }
    let scale = density / default_density;
    let scaled_min = (min as f32 * scale).round() as usize;
    let scaled_max = (max as f32 * scale).round() as usize;
    ((reach_radius * density).ceil() as usize).clamp(scaled_min.min(scaled_max), scaled_max)
}

#[cfg(test)]
mod tests {
    use super::super::scorch::ScorchMark;
    use super::*;
    use crate::map::GrassBurn;
    use common::constants::WALL_HEIGHT;
    use common::protocol::{BarrierKindTable, Floor, MapLayout, Wall};

    #[test]
    fn shard_count_clamps_to_bounds() {
        let config = ExplosionVfxConfig::default();
        assert_eq!(shard_count(0.1, &config), EXPLOSION_SHARD_MIN_COUNT);
        assert_eq!(shard_count(1000.0, &config), EXPLOSION_SHARD_MAX_COUNT);
        let mid = shard_count(10.0, &config);
        assert!(mid > EXPLOSION_SHARD_MIN_COUNT && mid < EXPLOSION_SHARD_MAX_COUNT);
    }

    #[test]
    fn shard_count_steps_up_with_blast_radius() {
        let config = ExplosionVfxConfig::default();
        // The three shipped kinds must be visibly distinct.
        assert!(shard_count(6.0, &config) < shard_count(10.0, &config));
        assert!(shard_count(10.0, &config) < shard_count(15.0, &config));
    }

    #[test]
    fn density_scales_particle_count_and_zero_disables_it() {
        let default = ExplosionVfxConfig::default();
        let mut half = default;
        half.shards.count_per_radius_meter *= 0.5;
        let mut disabled = default;
        disabled.shards.count_per_radius_meter = 0.0;

        assert_eq!(shard_count(10.0, &half), shard_count(10.0, &default) / 2);
        assert_eq!(shard_count(10.0, &disabled), 0);
    }

    #[test]
    fn large_grounded_explosion_spawns_one_sized_scorch_and_grass_burn() {
        let map_layout = MapLayout {
            floors: vec![Floor {
                x1: -10.0,
                z1: -10.0,
                x2: 10.0,
                z2: 10.0,
                y: 0.0,
                thickness: 1.0,
                level: 0,
            }],
            ..default()
        };
        let collision_world = CollisionWorld::from_map_layout(&map_layout, &BarrierKindTable::default());
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let config = ExplosionVfxConfig::default();
        let explosion_assets = ExplosionAssets::new(&mut meshes, &mut materials, config);
        let mut budget = ExplosionVfxBudget::default();
        let mut world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = Commands::new(&mut queue, &world);
            spawn_explosion(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut budget,
                &explosion_assets,
                ExplosionSpec {
                    center: Vec3::new(0.0, 1.0, 0.0),
                    ground_y: 0.0,
                    fireball_diameter: 15.0,
                    blast_radius: Some(15.0),
                },
                Some(&collision_world),
                Some(&map_layout),
            );
        }
        queue.apply(&mut world);

        let mut marks = world.query::<(&ScorchMark, &Transform)>();
        let marks: Vec<_> = marks.iter(&world).collect();
        assert_eq!(marks.len(), 1);
        let transform = marks[0].1;
        assert!((transform.translation.y - EXPLOSION_SCORCH_SURFACE_OFFSET).abs() < 0.001);
        let scorch_radius = 15.0 * config.scorches.blast_diameter_factor;
        let expected_diameter = 2.0 * scorch_radius.mul_add(scorch_radius, -1.0).sqrt();
        assert_eq!(transform.scale, Vec3::splat(expected_diameter));
        drop(marks);
        assert_eq!(world.query::<&GrassBurn>().iter(&world).count(), 1);
    }

    #[test]
    fn explosion_next_to_wall_spawns_wall_scorch_mark() {
        let map_layout = MapLayout {
            walls: vec![Wall {
                x1: -10.0,
                z1: 1.0,
                x2: 10.0,
                z2: 1.0,
                width: 0.2,
                level: 0,
            }],
            floors: vec![Floor {
                x1: -10.0,
                z1: -10.0,
                x2: 10.0,
                z2: 10.0,
                y: 0.0,
                thickness: 1.0,
                level: 0,
            }],
            ..default()
        };
        let collision_world = CollisionWorld::from_map_layout(&map_layout, &BarrierKindTable::default());
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let explosion_assets = ExplosionAssets::new(&mut meshes, &mut materials, ExplosionVfxConfig::default());
        let mut budget = ExplosionVfxBudget::default();
        let mut world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = Commands::new(&mut queue, &world);
            spawn_explosion(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut budget,
                &explosion_assets,
                ExplosionSpec {
                    center: Vec3::new(0.0, 1.0, 0.0),
                    ground_y: 0.0,
                    fireball_diameter: 6.0,
                    blast_radius: Some(6.0),
                },
                Some(&collision_world),
                Some(&map_layout),
            );
        }
        queue.apply(&mut world);

        let mut marks = world.query::<(&ScorchMark, &Transform)>();
        let marks: Vec<_> = marks.iter(&world).collect();
        assert_eq!(marks.len(), 2);
        let wall_transform = marks
            .iter()
            .find_map(|(_, transform)| {
                let surface_normal = transform.rotation * Vec3::Y;
                (surface_normal.dot(Vec3::NEG_Z) > 0.999).then_some(*transform)
            })
            .expect("expected wall-aligned scorch mark");
        assert_eq!(wall_transform.scale.y, 1.0);
        assert!(wall_transform.scale.x > wall_transform.scale.z);
        assert!(wall_transform.translation.y - wall_transform.scale.z * 0.5 < 0.0);
        assert!((wall_transform.translation.y / wall_transform.scale.z).abs() < 0.35);
        assert!(wall_transform.translation.y + wall_transform.scale.z * 0.5 <= WALL_HEIGHT + 0.001);
        drop(marks);
        assert_eq!(world.query::<&GrassBurn>().iter(&world).count(), 1);
    }
}
