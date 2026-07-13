use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use bevy::{light::NotShadowCaster, prelude::*};
use rand::{RngExt, rng};

use common::{config::GameplayConfig, protocol::Position};

use crate::constants::{
    EXPLOSION_FALLBACK_SCALE, EXPLOSION_FIREBALL_DIAMETER_FACTOR, EXPLOSION_FLASH_BRIGHTNESS,
    EXPLOSION_FLASH_LIFETIME_FACTOR, EXPLOSION_FLASH_START_ALPHA, EXPLOSION_LIFETIME_SECS, EXPLOSION_LIGHT_COLOR,
    EXPLOSION_LIGHT_INTENSITY, EXPLOSION_LIGHT_MIN_RANGE, EXPLOSION_LIGHT_RANGE_PER_RADIUS, EXPLOSION_RING_BRIGHTNESS,
    EXPLOSION_RING_DIAMETER_FACTOR, EXPLOSION_RING_LIFETIME_FACTOR, EXPLOSION_RING_RESOLUTION,
    EXPLOSION_RING_START_ALPHA, EXPLOSION_RING_THICKNESS, EXPLOSION_RING_Y_OFFSET, EXPLOSION_SHARD_BOUNCE_DAMPING,
    EXPLOSION_SHARD_BRIGHTNESS, EXPLOSION_SHARD_FRICTION, EXPLOSION_SHARD_GRAVITY, EXPLOSION_SHARD_LIFETIME_FACTOR,
    EXPLOSION_SHARD_MAX_COUNT, EXPLOSION_SHARD_MIN_COUNT, EXPLOSION_SHARD_SIZE, EXPLOSION_SHARD_SPEED_FACTOR,
    EXPLOSION_SHARD_UP_BIAS, EXPLOSION_SHARDS_PER_METER,
};

// Blast radii from `SInit` (per actor kind + the player death blast). Starts
// empty (initialized at app build) and is replaced when `Init` arrives;
// death cues can't arrive earlier — the pre-bootstrap dispatcher drops them.
#[derive(Resource, Default)]
pub struct ExplosionRadii {
    pub actors: HashMap<String, f32>,
    pub player: f32,
}

// Shared meshes plus the two material templates that get cloned per
// explosion (their alpha is animated, so instances can't share a handle).
// Shards never fade — one shared material serves every explosion.
#[derive(Resource)]
pub struct ExplosionAssets {
    fireball_mesh: Handle<Mesh>,
    ring_mesh: Handle<Mesh>,
    shard_mesh: Handle<Mesh>,
    shard_material: Handle<StandardMaterial>,
    fireball_template: StandardMaterial,
    ring_template: StandardMaterial,
}

impl ExplosionAssets {
    // Public (rather than folded into `FromWorld`) so tests can build the
    // resource against plain `Assets` collections.
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let flash = EXPLOSION_FLASH_BRIGHTNESS;
        let ring = EXPLOSION_RING_BRIGHTNESS;
        let shard = EXPLOSION_SHARD_BRIGHTNESS;
        Self {
            // Unit-diameter meshes: `Transform::scale` equals the layer's
            // world diameter in meters.
            fireball_mesh: meshes.add(Sphere::new(0.5)),
            ring_mesh: meshes.add(
                Annulus::new(0.5 - EXPLOSION_RING_THICKNESS, 0.5)
                    .mesh()
                    .resolution(EXPLOSION_RING_RESOLUTION)
                    .build(),
            ),
            shard_mesh: meshes.add(Cuboid::new(
                EXPLOSION_SHARD_SIZE,
                EXPLOSION_SHARD_SIZE,
                EXPLOSION_SHARD_SIZE,
            )),
            shard_material: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.6, 0.25),
                emissive: LinearRgba::rgb(shard, shard * 0.45, shard * 0.12),
                ..default()
            }),
            fireball_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.85, 0.6, EXPLOSION_FLASH_START_ALPHA),
                emissive: LinearRgba::rgb(flash, flash * 0.45, flash * 0.12),
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            ring_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.3, EXPLOSION_RING_START_ALPHA),
                emissive: LinearRgba::rgb(ring, ring * 0.45, ring * 0.12),
                alpha_mode: AlphaMode::Blend,
                // The ring must render when seen from below a ledge or ramp.
                cull_mode: None,
                ..default()
            },
        }
    }
}

impl FromWorld for ExplosionAssets {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            Self::new(&mut meshes, &mut materials)
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
pub struct ExplosionShard {
    velocity: Vec3,
    elapsed: f32,
    lifetime: f32,
    // The blast's floor plane — cosmetic bounce reference.
    floor_y: f32,
}

#[derive(Component)]
pub struct ExplosionLight {
    elapsed: f32,
    lifetime: f32,
    intensity: f32,
    range: f32,
}

pub fn spawn_actor_explosion(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    radii: &ExplosionRadii,
    gameplay_config: &GameplayConfig,
    actor_kind: &str,
    pos: Position,
) {
    let actor_physics = gameplay_config
        .actor(actor_kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics();
    let blast_radius = radii.actors.get(actor_kind).copied();
    let fireball_diameter = blast_radius.map_or(EXPLOSION_FALLBACK_SCALE, |radius| {
        2.0 * radius * EXPLOSION_FIREBALL_DIAMETER_FACTOR
    });
    spawn_explosion(
        commands,
        materials,
        explosion_assets,
        Vec3::new(pos.x, actor_physics.collider_center_y(pos.y), pos.z),
        pos.y,
        fireball_diameter,
        blast_radius,
    );
}

pub fn spawn_player_explosion(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    radii: &ExplosionRadii,
    gameplay_config: &GameplayConfig,
    pos: Position,
) {
    let player_physics = gameplay_config.player.physics();
    spawn_explosion(
        commands,
        materials,
        explosion_assets,
        Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z),
        pos.y,
        2.0 * radii.player * EXPLOSION_FIREBALL_DIAMETER_FACTOR,
        Some(radii.player),
    );
}

// Four layers: fireball flash, ground shockwave ring, debris shard burst,
// and a fading point light. `center` is the blast origin (collider center);
// `ground_y` anchors the ring at the victim's feet.
pub fn spawn_explosion(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    center: Vec3,
    ground_y: f32,
    fireball_diameter: f32,
    blast_radius: Option<f32>,
) {
    // `None` = cosmetic burst with no area damage (unknown-kind fallback):
    // shards and light size off the fireball, and no ring is spawned — a
    // ring always marks a real danger area.
    let reach_radius = blast_radius.unwrap_or(fireball_diameter * 0.5);

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
            lifetime: EXPLOSION_LIFETIME_SECS * EXPLOSION_FLASH_LIFETIME_FACTOR,
            max_scale: fireball_diameter,
            start_alpha: EXPLOSION_FLASH_START_ALPHA,
            base_emissive: explosion_assets.fireball_template.emissive,
            material: fireball_material,
        },
    ));

    if let Some(blast_radius) = blast_radius {
        let ring_material = materials.add(explosion_assets.ring_template.clone());
        commands.spawn((
            Mesh3d(explosion_assets.ring_mesh.clone()),
            MeshMaterial3d(ring_material.clone()),
            NotShadowCaster,
            Transform {
                translation: Vec3::new(center.x, ground_y + EXPLOSION_RING_Y_OFFSET, center.z),
                // The annulus meshes in the XY plane; lay it flat on XZ.
                rotation: Quat::from_rotation_x(-FRAC_PI_2),
                scale: Vec3::splat(0.01),
            },
            ExplosionPulse {
                elapsed: 0.0,
                lifetime: EXPLOSION_LIFETIME_SECS * EXPLOSION_RING_LIFETIME_FACTOR,
                max_scale: 2.0 * blast_radius * EXPLOSION_RING_DIAMETER_FACTOR,
                start_alpha: EXPLOSION_RING_START_ALPHA,
                base_emissive: explosion_assets.ring_template.emissive,
                material: ring_material,
            },
        ));
    }

    let shard_lifetime = EXPLOSION_LIFETIME_SECS * EXPLOSION_SHARD_LIFETIME_FACTOR;
    let mut rng = rng();
    for _ in 0..shard_count(reach_radius) {
        let direction = (Vec3::new(
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
        ) + Vec3::Y * EXPLOSION_SHARD_UP_BIAS)
            .normalize_or_zero();
        let direction = if direction == Vec3::ZERO { Vec3::Y } else { direction };
        let speed = reach_radius / shard_lifetime * EXPLOSION_SHARD_SPEED_FACTOR * rng.random_range(0.7..1.3);
        commands.spawn((
            Mesh3d(explosion_assets.shard_mesh.clone()),
            MeshMaterial3d(explosion_assets.shard_material.clone()),
            NotShadowCaster,
            Transform::from_translation(center),
            ExplosionShard {
                velocity: direction * speed,
                elapsed: 0.0,
                lifetime: shard_lifetime,
                floor_y: ground_y,
            },
        ));
    }

    // Own entity: the light fades over the full master lifetime, outliving
    // the shorter fireball flash.
    let range = (reach_radius * EXPLOSION_LIGHT_RANGE_PER_RADIUS).max(EXPLOSION_LIGHT_MIN_RANGE);
    commands.spawn((
        PointLight {
            color: EXPLOSION_LIGHT_COLOR,
            intensity: EXPLOSION_LIGHT_INTENSITY,
            range,
            radius: 1.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(center),
        ExplosionLight {
            elapsed: 0.0,
            lifetime: EXPLOSION_LIFETIME_SECS,
            intensity: EXPLOSION_LIGHT_INTENSITY,
            range,
        },
    ));
}

fn shard_count(reach_radius: f32) -> usize {
    ((reach_radius * EXPLOSION_SHARDS_PER_METER).ceil() as usize)
        .clamp(EXPLOSION_SHARD_MIN_COUNT, EXPLOSION_SHARD_MAX_COUNT)
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

// Ballistic fade-out, like bounce sparks but with per-explosion lifetimes.
pub fn explosion_shards_system(
    mut commands: Commands,
    time: Res<Time>,
    mut shards: Query<(Entity, &mut ExplosionShard, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut shard, mut transform) in &mut shards {
        shard.elapsed += delta;
        if shard.elapsed >= shard.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        shard.velocity.y -= EXPLOSION_SHARD_GRAVITY * delta;
        transform.translation += shard.velocity * delta;
        let floor = shard.floor_y + EXPLOSION_SHARD_SIZE * 0.5;
        if transform.translation.y < floor && shard.velocity.y < 0.0 {
            transform.translation.y = floor;
            shard.velocity.y = -shard.velocity.y * EXPLOSION_SHARD_BOUNCE_DAMPING;
            shard.velocity.x *= EXPLOSION_SHARD_FRICTION;
            shard.velocity.z *= EXPLOSION_SHARD_FRICTION;
        }
        transform.scale = Vec3::splat(1.0 - shard.elapsed / shard.lifetime);
    }
}

pub fn explosion_lights_system(
    mut commands: Commands,
    time: Res<Time>,
    mut lights: Query<(Entity, &mut ExplosionLight, &mut PointLight)>,
) {
    let delta = time.delta_secs();
    for (entity, mut state, mut light) in &mut lights {
        state.elapsed += delta;
        if state.elapsed >= state.lifetime {
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
    use crate::constants::{EXPLOSION_SHARD_MAX_COUNT, EXPLOSION_SHARD_MIN_COUNT};

    #[test]
    fn shard_count_clamps_to_bounds() {
        assert_eq!(shard_count(0.1), EXPLOSION_SHARD_MIN_COUNT);
        assert_eq!(shard_count(1000.0), EXPLOSION_SHARD_MAX_COUNT);
        let mid = shard_count(10.0);
        assert!(mid > EXPLOSION_SHARD_MIN_COUNT && mid < EXPLOSION_SHARD_MAX_COUNT);
    }

    #[test]
    fn shard_count_steps_up_with_blast_radius() {
        // The three shipped kinds must be visibly distinct.
        assert!(shard_count(6.0) < shard_count(10.0));
        assert!(shard_count(10.0) < shard_count(15.0));
    }
}
