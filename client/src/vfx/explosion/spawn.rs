use super::{
    animation::{ExplosionLight, ExplosionPulse},
    assets::{BlastRadii, ExplosionAssets, shockwave_mesh},
    particles::{ExplosionVfxBudget, SurfacePlane},
    scorch::{
        SCORCH_SURFACE_OFFSET, ScorchStyle, SurfaceContact, ground_scorch_placement, spawn_scorch_mark,
        surface_cross_section_diameter, wall_scorch_placements,
    },
    shards::spawn_shard_cloud,
    smoke::spawn_smoke_cloud,
};
use crate::{carriers::CarrierEntities, constants::*};
use bevy::{light::NotShadowCaster, prelude::*};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{MapLayout, Position},
};
use rand::rng;

const SHOCKWAVE_SURFACE_OFFSET: f32 = 0.05;

#[derive(Clone, Copy)]
struct ExplosionSpec {
    center: Vec3,
    ground_y: f32,
    fireball_diameter: f32,
    blast_radius: Option<f32>,
}

// The world plumbing every explosion spawn draws from, so callers hand over
// one context.
pub struct ExplosionSpawnCtx<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub budget: &'a mut ExplosionVfxBudget,
    pub explosion_assets: &'a ExplosionAssets,
    pub gameplay_config: &'a GameplayConfig,
    pub collision_world: &'a CollisionWorld,
    pub map_layout: &'a MapLayout,
    pub carriers: &'a Carriers,
    pub carrier_entities: &'a CarrierEntities,
    pub blast_radii: &'a BlastRadii,
}

impl<'a> ExplosionSpawnCtx<'a> {
    fn surfaces(&self) -> ExplosionSurfaces<'a> {
        ExplosionSurfaces {
            collision_world: self.collision_world,
            map_layout: self.map_layout,
            carriers: self.carriers,
            carrier_entities: self.carrier_entities,
        }
    }
}

// What an explosion's surface effects land on: the geometry to probe, and
// the carrier roots a mark hangs under.
struct ExplosionSurfaces<'a> {
    collision_world: &'a CollisionWorld,
    map_layout: &'a MapLayout,
    carriers: &'a Carriers,
    carrier_entities: &'a CarrierEntities,
}

pub fn spawn_actor_explosion(commands: &mut Commands, ctx: &mut ExplosionSpawnCtx, actor_kind: &str, pos: Position) {
    let surfaces = ctx.surfaces();
    let actor_physics = ctx
        .gameplay_config
        .actor(actor_kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics();
    let blast_radius = ctx.blast_radii.actors.get(actor_kind).copied();
    let fireball_diameter = blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
        2.0 * radius * EXPLOSION_FIREBALL_BLAST_DIAMETER_FACTOR
    });
    spawn_explosion(
        commands,
        ctx.meshes,
        ctx.materials,
        ctx.budget,
        ctx.explosion_assets,
        ExplosionSpec {
            center: Vec3::new(pos.x, actor_physics.hitbox_center_y(pos.y), pos.z),
            ground_y: pos.y,
            fireball_diameter,
            blast_radius,
        },
        &surfaces,
    );
}

pub fn spawn_player_explosion(commands: &mut Commands, ctx: &mut ExplosionSpawnCtx, pos: Position) {
    let surfaces = ctx.surfaces();
    let player_physics = ctx.gameplay_config.player.physics();
    let blast_radius = (ctx.blast_radii.player > 0.0).then_some(ctx.blast_radii.player);
    spawn_explosion(
        commands,
        ctx.meshes,
        ctx.materials,
        ctx.budget,
        ctx.explosion_assets,
        ExplosionSpec {
            center: Vec3::new(pos.x, player_physics.hitbox_center_y(pos.y), pos.z),
            ground_y: pos.y,
            fireball_diameter: blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
                2.0 * radius * EXPLOSION_FIREBALL_BLAST_DIAMETER_FACTOR
            }),
            blast_radius,
        },
        &surfaces,
    );
}

// A missile detonation: the blast origin is the detonation point itself (no
// character body).
pub fn spawn_missile_explosion(commands: &mut Commands, ctx: &mut ExplosionSpawnCtx, pos: Position) {
    let surfaces = ctx.surfaces();
    let blast_radius = (ctx.blast_radii.missile > 0.0).then_some(ctx.blast_radii.missile);
    spawn_explosion(
        commands,
        ctx.meshes,
        ctx.materials,
        ctx.budget,
        ctx.explosion_assets,
        ExplosionSpec {
            center: Vec3::from(pos),
            ground_y: pos.y,
            fireball_diameter: blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
                2.0 * radius * EXPLOSION_FIREBALL_BLAST_DIAMETER_FACTOR
            }),
            blast_radius,
        },
        &surfaces,
    );
}

// Six layers: fireball flash, ground shockwave ring, scorch mark, debris
// shard burst, smoke, and a fading point light. `center` is the blast origin
// (hitbox center); `ground_y` anchors the ring at the victim's feet.
fn spawn_explosion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    spec: ExplosionSpec,
    surfaces: &ExplosionSurfaces<'_>,
) {
    let ExplosionSpec {
        center,
        ground_y,
        fireball_diameter,
        blast_radius,
    } = spec;
    let ExplosionSurfaces {
        collision_world,
        map_layout,
        carriers,
        carrier_entities,
    } = *surfaces;
    // `None` = cosmetic burst with no area damage (unknown-kind fallback):
    // shards and light size off the fireball, and no ring is spawned — a
    // ring always marks a real danger area.
    let reach_radius = blast_radius.unwrap_or(fireball_diameter * 0.5);
    let standing_distance = (center.y - ground_y).max(0.0) + SCORCH_SURFACE_OFFSET;
    let ground_surface = collision_world.ground_surface_below(center, reach_radius.max(standing_distance));

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
            lifetime: EXPLOSION_BASE_DURATION_SECS * EXPLOSION_FIREBALL_LIFETIME_FACTOR,
            max_scale: fireball_diameter,
            start_alpha: EXPLOSION_FIREBALL_START_ALPHA,
            base_emissive: explosion_assets.fireball_template.emissive,
            material: fireball_material,
        },
    ));

    if let (Some(blast_radius), Some(surface)) = (blast_radius, ground_surface) {
        let ring_material = materials.add(explosion_assets.ring_template.clone());
        let ring_mesh = meshes.add(shockwave_mesh(
            Some(collision_world),
            center,
            surface.normal,
            blast_radius * EXPLOSION_SHOCKWAVE_DIAMETER_FACTOR,
        ));
        let point = carriers.pose(surface.carrier).inverse_transform_point(surface.point);
        commands.spawn((
            Mesh3d(ring_mesh),
            MeshMaterial3d(ring_material.clone()),
            NotShadowCaster,
            ChildOf(carrier_entities.get(surface.carrier)),
            Transform {
                translation: point + surface.normal * SHOCKWAVE_SURFACE_OFFSET,
                rotation: Quat::from_rotation_arc(Vec3::Y, surface.normal),
                scale: Vec3::splat(0.01),
            },
            ExplosionPulse {
                elapsed: 0.0,
                lifetime: EXPLOSION_BASE_DURATION_SECS * EXPLOSION_SHOCKWAVE_LIFETIME_FACTOR,
                max_scale: 2.0 * blast_radius * EXPLOSION_SHOCKWAVE_DIAMETER_FACTOR,
                start_alpha: EXPLOSION_SHOCKWAVE_START_ALPHA,
                base_emissive: explosion_assets.ring_template.emissive,
                material: ring_material,
            },
        ));
    }

    let mut rng = rng();
    let scorch_diameter = 2.0 * reach_radius * EXPLOSION_SCORCH_BLAST_DIAMETER_FACTOR;
    let scorch_radius = scorch_diameter * 0.5;
    let scorch_style = ScorchStyle::random(explosion_assets.scorch_variants.len(), &mut rng);
    if let Some(surface) = ground_surface
        && let Some(diameter) = surface_cross_section_diameter(scorch_radius, center.distance(surface.point))
    {
        let contact = SurfaceContact {
            point: surface.point,
            normal: surface.normal,
            carrier: surface.carrier,
        };
        spawn_scorch_mark(
            commands,
            meshes,
            materials,
            budget,
            explosion_assets,
            carrier_entities,
            ground_scorch_placement(contact, map_layout, carriers, center, diameter, scorch_style),
            scorch_style,
            EXPLOSION_SCORCH_MAX_ACTIVE,
        );
    }
    for placement in wall_scorch_placements(
        map_layout,
        carriers,
        center,
        scorch_radius,
        EXPLOSION_SCORCH_WALL_REACH_FACTOR,
        scorch_style,
    ) {
        spawn_scorch_mark(
            commands,
            meshes,
            materials,
            budget,
            explosion_assets,
            carrier_entities,
            placement,
            scorch_style,
            EXPLOSION_SCORCH_MAX_ACTIVE,
        );
    }

    let ground_plane = ground_surface.map(|surface| SurfacePlane::from_hit(surface, center, reach_radius * 0.75));
    spawn_shard_cloud(
        commands,
        meshes,
        explosion_assets.shard_material.clone(),
        budget,
        Some(collision_world),
        ground_plane,
        center,
        reach_radius,
        shard_count(reach_radius),
        &mut rng,
    );
    spawn_smoke_cloud(
        commands,
        meshes,
        explosion_assets.smoke_material.clone(),
        budget,
        center,
        reach_radius,
        smoke_count(reach_radius),
        &mut rng,
    );

    // Own entity: the light fades over the full master lifetime, outliving
    // the shorter fireball flash.
    if budget.reserve_light(EXPLOSION_LIGHT_MAX_ACTIVE) {
        let range = (reach_radius * EXPLOSION_LIGHT_RANGE_PER_RADIUS).max(EXPLOSION_LIGHT_MIN_RANGE);
        let intensity = EXPLOSION_LIGHT_INTENSITY_LUMENS;
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
                lifetime: EXPLOSION_BASE_DURATION_SECS,
                intensity,
                range,
            },
        ));
    }
}

fn shard_count(reach_radius: f32) -> usize {
    scaled_particle_count(
        reach_radius,
        EXPLOSION_SHARDS_PER_RADIUS_METER,
        EXPLOSION_REFERENCE_SHARDS_PER_METER,
        EXPLOSION_SHARD_MIN_COUNT,
        EXPLOSION_SHARD_MAX_COUNT,
    )
}

fn smoke_count(reach_radius: f32) -> usize {
    scaled_particle_count(
        reach_radius,
        EXPLOSION_SMOKE_PER_RADIUS_METER,
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
#[path = "tests/spawn.rs"]
mod tests;
