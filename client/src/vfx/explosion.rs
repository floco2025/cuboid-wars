use std::{collections::HashMap, f32::consts::TAU};

use bevy::{
    asset::RenderAssetUsages, light::NotShadowCaster, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use rand::rng;

use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{MapLayout, Position},
};

use super::explosion_particles::{ExplosionVfxBudget, SurfacePlane, spawn_shard_cloud, spawn_smoke_cloud};
use super::scorch::{
    ScorchPlacement, ScorchStyle, scorch_mesh, surface_cross_section_diameter, wall_scorch_diameter,
    wall_scorch_placements,
};
use crate::{
    config::{ClientSettings, ExplosionVfxConfig},
    constants::*,
};

const SHOCKWAVE_RESOLUTION: u32 = 64;

// Blast radii from `SInit` (per actor kind + the player death blast). Starts
// empty (initialized at app build) and is replaced when `Init` arrives;
// death cues can't arrive earlier — the pre-bootstrap dispatcher drops them.
#[derive(Resource, Default)]
pub struct ExplosionRadii {
    pub actors: HashMap<String, f32>,
    pub player: f32,
}

#[must_use]
pub fn explosion_sound_speed(radius: f32) -> f32 {
    (1.08 - radius * 0.012).clamp(0.84, 1.04)
}

// Shared meshes plus material templates cloned for animated instances.
#[derive(Resource)]
pub struct ExplosionAssets {
    fireball_mesh: Handle<Mesh>,
    scorch_meshes: Vec<Handle<Mesh>>,
    shard_material: Handle<StandardMaterial>,
    smoke_material: Handle<StandardMaterial>,
    fireball_template: StandardMaterial,
    ring_template: StandardMaterial,
    scorch_template: StandardMaterial,
    config: ExplosionVfxConfig,
}

impl ExplosionAssets {
    // Public (rather than folded into `FromWorld`) so tests can build the
    // resource against plain `Assets` collections.
    pub fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        config: ExplosionVfxConfig,
    ) -> Self {
        let flash = config.fireball.emissive_brightness;
        let ring = config.shockwave.emissive_brightness;
        let shard = config.shards.emissive_brightness;
        Self {
            // Unit-diameter meshes: `Transform::scale` equals the layer's
            // world diameter in meters.
            fireball_mesh: meshes.add(with_white_vertex_colors(Mesh::from(Sphere::new(0.5)))),
            scorch_meshes: (0..EXPLOSION_SCORCH_MESH_VARIANT_COUNT)
                .map(|variant| meshes.add(scorch_mesh(variant as u64)))
                .collect(),
            shard_material: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.6, 0.25),
                emissive: LinearRgba::rgb(shard, shard * 0.45, shard * 0.12),
                ..default()
            }),
            smoke_material: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                ..default()
            }),
            fireball_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.85, 0.6, EXPLOSION_FIREBALL_START_ALPHA),
                emissive: LinearRgba::rgb(flash, flash * 0.45, flash * 0.12),
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            ring_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.3, EXPLOSION_SHOCKWAVE_START_ALPHA),
                emissive: LinearRgba::rgb(ring, ring * 0.45, ring * 0.12),
                alpha_mode: AlphaMode::Blend,
                // The ring must render when seen from below a ledge or ramp.
                cull_mode: None,
                ..default()
            },
            scorch_template: StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                unlit: true,
                depth_bias: 1.0,
                ..default()
            },
            config,
        }
    }
}

// Uniform white vertex colors: flips the mesh onto the vertex-color
// pipeline permutation. In this app the plain-mesh Blend permutation
// renders translucent materials wrong (invisible or unlit-white); the
// vertex-color path — which the scorch meshes use — renders correctly.
// White multiplies to identity, so visuals are otherwise unchanged.
fn with_white_vertex_colors(mut mesh: Mesh) -> Mesh {
    let count = mesh.count_vertices();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0, 1.0, 1.0, 1.0]; count]);
    mesh
}

fn shockwave_mesh(
    collision_world: Option<&CollisionWorld>,
    center: Vec3,
    surface_normal: Vec3,
    reach_radius: f32,
) -> Mesh {
    let rotation = Quat::from_rotation_arc(Vec3::Y, surface_normal);
    let mut positions = Vec::with_capacity(SHOCKWAVE_RESOLUTION as usize * 2);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut clear = Vec::with_capacity(SHOCKWAVE_RESOLUTION as usize);

    for segment in 0..SHOCKWAVE_RESOLUTION {
        let angle = segment as f32 / SHOCKWAVE_RESOLUTION as f32 * TAU;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        positions.push((radial * 0.5).to_array());
        positions.push((radial * (0.5 * (1.0 - EXPLOSION_SHOCKWAVE_THICKNESS_RATIO))).to_array());
        normals.extend([Vec3::Y.to_array(); 2]);
        colors.extend([[1.0, 1.0, 1.0, 1.0]; 2]);

        let world_direction = rotation * radial;
        let horizontal = Vec3::new(world_direction.x, 0.0, world_direction.z).normalize_or_zero();
        clear.push(
            horizontal == Vec3::ZERO
                || collision_world
                    .is_none_or(|world| world.wall_surface_along_ray(center, horizontal, reach_radius).is_none()),
        );
    }

    let mut indices = Vec::with_capacity(SHOCKWAVE_RESOLUTION as usize * 6);
    for segment in 0..SHOCKWAVE_RESOLUTION as usize {
        let next = (segment + 1) % SHOCKWAVE_RESOLUTION as usize;
        if !clear[segment] || !clear[next] {
            continue;
        }
        let outer = (segment * 2) as u32;
        let inner = outer + 1;
        let next_outer = (next * 2) as u32;
        let next_inner = next_outer + 1;
        indices.extend([outer, inner, next_outer, next_outer, inner, next_inner]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

impl FromWorld for ExplosionAssets {
    fn from_world(world: &mut World) -> Self {
        let config = world.resource::<ClientSettings>().vfx.explosions;
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            Self::new(&mut meshes, &mut materials, config)
        })
    }
}

// Fireball flash and shockwave ring share one animation: ease-out scale
// growth plus alpha + emissive fade on a per-instance material clone.
#[derive(Component)]
pub struct ExplosionPulse {
    elapsed: f32,
    lifetime: f32,
    max_scale: f32,
    start_alpha: f32,
    // Template emissive at spawn; the fade rescales from this each frame.
    base_emissive: LinearRgba,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct ExplosionLight {
    elapsed: f32,
    lifetime: f32,
    intensity: f32,
    range: f32,
}

#[derive(Component)]
pub struct ScorchMark {
    elapsed: f32,
    material: Handle<StandardMaterial>,
}

#[derive(Clone, Copy)]
struct ExplosionSpec {
    center: Vec3,
    ground_y: f32,
    fireball_diameter: f32,
    blast_radius: Option<f32>,
}

pub fn spawn_actor_explosion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    radii: &ExplosionRadii,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    map_layout: Option<&MapLayout>,
    actor_kind: &str,
    pos: Position,
) {
    let actor_physics = gameplay_config
        .actor(actor_kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics();
    let blast_radius = radii.actors.get(actor_kind).copied();
    let fireball_diameter = blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
        2.0 * radius * explosion_assets.config.fireball.blast_diameter_factor
    });
    spawn_explosion(
        commands,
        meshes,
        materials,
        budget,
        explosion_assets,
        ExplosionSpec {
            center: Vec3::new(pos.x, actor_physics.collider_center_y(pos.y), pos.z),
            ground_y: pos.y,
            fireball_diameter,
            blast_radius,
        },
        collision_world,
        map_layout,
    );
}

pub fn spawn_player_explosion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    radii: &ExplosionRadii,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    map_layout: Option<&MapLayout>,
    pos: Position,
) {
    let player_physics = gameplay_config.player.physics();
    let blast_radius = (radii.player > 0.0).then_some(radii.player);
    spawn_explosion(
        commands,
        meshes,
        materials,
        budget,
        explosion_assets,
        ExplosionSpec {
            center: Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z),
            ground_y: pos.y,
            fireball_diameter: blast_radius.map_or(EXPLOSION_FALLBACK_FIREBALL_DIAMETER, |radius| {
                2.0 * radius * explosion_assets.config.fireball.blast_diameter_factor
            }),
            blast_radius,
        },
        collision_world,
        map_layout,
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

fn spawn_scorch_mark(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    placement: ScorchPlacement,
    style: ScorchStyle,
    max_active_marks: usize,
) {
    let material = materials.add(explosion_assets.scorch_template.clone());
    let scorch_mesh = explosion_assets.scorch_meshes[style.mesh_index].clone();
    let entity = commands
        .spawn((
            Mesh3d(scorch_mesh),
            MeshMaterial3d(material.clone()),
            NotShadowCaster,
            placement.transform,
            ScorchMark { elapsed: 0.0, material },
        ))
        .id();
    budget.register_scorch(commands, entity, max_active_marks);
}

fn scorch_alpha(elapsed: f32, lifetime: f32) -> f32 {
    let fade_secs = lifetime * EXPLOSION_SCORCH_FADE_FRACTION;
    ((lifetime - elapsed) / fade_secs).clamp(0.0, 1.0)
}

pub fn scorch_marks_system(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<ClientSettings>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut budget: ResMut<ExplosionVfxBudget>,
    mut marks: Query<(Entity, &mut ScorchMark)>,
) {
    let delta = time.delta_secs();
    let lifetime = settings.vfx.explosions.scorches.lifetime_secs;
    let fade_secs = lifetime * EXPLOSION_SCORCH_FADE_FRACTION;
    for (entity, mut mark) in &mut marks {
        mark.elapsed += delta;
        if mark.elapsed >= lifetime {
            budget.remove_scorch(entity);
            commands.entity(entity).despawn();
            continue;
        }
        if mark.elapsed >= lifetime - fade_secs
            && let Some(mut material) = materials.get_mut(&mark.material)
        {
            material.base_color.set_alpha(scorch_alpha(mark.elapsed, lifetime));
        }
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

pub fn explosion_pulse_system(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pulses: Query<(Entity, &mut ExplosionPulse, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut pulse, mut transform) in &mut pulses {
        pulse.elapsed += delta;
        // The per-instance material asset frees itself when the entity drops
        // the last handle to it.
        if pulse.elapsed >= pulse.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let progress = (pulse.elapsed / pulse.lifetime).clamp(0.0, 1.0);
        let grow = 1.0 - (1.0 - progress).powi(3);
        transform.scale = Vec3::splat(pulse.max_scale * grow);
        if let Some(mut material) = materials.get_mut(&pulse.material) {
            material.base_color.set_alpha(pulse.start_alpha * (1.0 - progress));
            // Emissive has no ceiling, so the alpha fade alone can't pull
            // extreme brightness values under the bloom threshold before
            // despawn — square-fade the emissive too so the glow dies
            // smoothly instead of blinking out.
            material.emissive = pulse.base_emissive * (1.0 - progress).powi(2);
        }
    }
}

pub fn explosion_lights_system(
    mut commands: Commands,
    time: Res<Time>,
    mut budget: ResMut<ExplosionVfxBudget>,
    mut lights: Query<(Entity, &mut ExplosionLight, &mut PointLight)>,
) {
    let delta = time.delta_secs();
    for (entity, mut state, mut light) in &mut lights {
        state.elapsed += delta;
        if state.elapsed >= state.lifetime {
            budget.release_light();
            commands.entity(entity).despawn();
            continue;
        }
        let progress = (state.elapsed / state.lifetime).clamp(0.0, 1.0);
        let fade = (1.0 - progress).powi(2);
        light.intensity = state.intensity * fade;
        light.range = state.range * fade.max(0.25);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn scorch_alpha_stays_opaque_then_fades_to_zero() {
        let lifetime = ExplosionVfxConfig::default().scorches.lifetime_secs;
        let fade_secs = lifetime * EXPLOSION_SCORCH_FADE_FRACTION;
        assert_eq!(scorch_alpha(0.0, lifetime), 1.0);
        assert_eq!(scorch_alpha(lifetime - fade_secs, lifetime), 1.0);
        assert_eq!(scorch_alpha(lifetime - fade_secs / 2.0, lifetime), 0.5);
        assert_eq!(scorch_alpha(lifetime, lifetime), 0.0);
    }

    #[test]
    fn larger_explosions_have_a_lower_sound_pitch() {
        assert!(explosion_sound_speed(6.0) > explosion_sound_speed(15.0));
        assert_eq!(explosion_sound_speed(100.0), 0.84);
    }

    #[test]
    fn grounded_explosion_spawns_one_sized_scorch_mark() {
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
        assert_eq!(marks.len(), 1);
        let transform = marks[0].1;
        assert!((transform.translation.y - EXPLOSION_SCORCH_SURFACE_OFFSET).abs() < 0.001);
        let scorch_radius = 6.0 * config.scorches.blast_diameter_factor;
        let expected_diameter = 2.0 * scorch_radius.mul_add(scorch_radius, -1.0).sqrt();
        assert_eq!(transform.scale, Vec3::splat(expected_diameter));
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
    }
}
