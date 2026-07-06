use bevy::{light::NotShadowCaster, prelude::*, world_serialization::WorldInstanceReady};
use rand::{RngExt, rng};

use crate::constants::{
    BEAM_IN_LIGHT_INTENSITY_PER_M3, BEAM_IN_LIGHT_MIN_INTENSITY, BEAM_IN_SPARKLE_BRIGHTNESS,
    BEAM_IN_SPARKLE_INTERVAL_SECS, BEAM_IN_SPARKLE_LIFETIME_SECS, BEAM_IN_SPARKLE_RISE_SPEED, BEAM_IN_SPARKLE_SIZE,
    BEAM_IN_SPARKLES_PER_M3,
};

// Shared mesh + material for beam-in sparkles. Separate from `SparkAssets`,
// whose material brightness is baked from the projectiles config.
#[derive(Resource)]
pub struct BeamAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

impl FromWorld for BeamAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
            BEAM_IN_SPARKLE_SIZE,
            BEAM_IN_SPARKLE_SIZE,
            BEAM_IN_SPARKLE_SIZE,
        ));
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.3),
            emissive: LinearRgba::rgb(
                BEAM_IN_SPARKLE_BRIGHTNESS,
                BEAM_IN_SPARKLE_BRIGHTNESS * 0.8,
                BEAM_IN_SPARKLE_BRIGHTNESS * 0.1,
            ),
            ..default()
        });
        Self { mesh, material }
    }
}

// Root marker of a beam-in ghost: a purely visual stand-in shown during an
// actor's spawn warning window. The fade runs off the server's window
// (`remaining_secs` / `warning_secs`): ticked locally every frame, resynced
// from each snapshot.
#[derive(Component)]
pub struct BeamInGhost {
    pub remaining_secs: f32,
    pub warning_secs: f32,
    pub sparkle_timer: f32,
    // Collider half-extents of the incoming actor — the sparkle volume.
    pub half_extents: Vec3,
}

impl BeamInGhost {
    // 0 at the start of the warning window, 1 at materialization.
    fn fade_progress(&self) -> f32 {
        if self.warning_secs <= 0.0 {
            1.0
        } else {
            (1.0 - self.remaining_secs / self.warning_secs).clamp(0.0, 1.0)
        }
    }

    // Collider volume of the incoming actor — scales the sparkle count and
    // the glow intensity so every kind gets the same effect density.
    fn volume(&self) -> f32 {
        8.0 * self.half_extents.x * self.half_extents.y * self.half_extents.z
    }
}

// Per-ghost clones of the model's materials, switched to alpha-blend so the
// fade can drive their opacity without touching the shared GLTF materials
// that live actors of the same kind use.
#[derive(Component)]
pub struct GhostFadeMaterials(Vec<Handle<StandardMaterial>>);

#[derive(Component)]
pub struct BeamSparkle {
    elapsed: f32,
}

// Observer on the ghost's model entity: once the GLTF scene hierarchy is
// instantiated, swap every mesh's shared material for a private translucent
// clone (starting fully invisible) and stop the half-materialized model from
// casting a full-strength shadow.
pub fn ghost_fade_setup_system(
    scene_ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut handles = Vec::new();
    for child in children.iter_descendants(scene_ready.entity) {
        let Ok(mesh_material) = mesh_materials.get(child) else {
            continue;
        };
        let Some(source) = materials.get(&mesh_material.0) else {
            continue;
        };
        let mut faded = source.clone();
        faded.alpha_mode = AlphaMode::Blend;
        faded.base_color.set_alpha(0.0);
        let handle = materials.add(faded);
        commands
            .entity(child)
            .insert((MeshMaterial3d(handle.clone()), NotShadowCaster));
        handles.push(handle);
    }
    commands.entity(scene_ready.entity).insert(GhostFadeMaterials(handles));
}

// Tick each ghost's window down and drive the fade: model opacity and glow
// intensity both follow the window's progress.
pub fn beam_ghost_fade_system(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ghosts: Query<(&mut BeamInGhost, &mut PointLight, &Children)>,
    faders: Query<&GhostFadeMaterials>,
) {
    let delta = time.delta_secs();
    for (mut ghost, mut light, children) in &mut ghosts {
        ghost.remaining_secs = (ghost.remaining_secs - delta).max(0.0);
        let progress = ghost.fade_progress();
        let full_intensity = (BEAM_IN_LIGHT_INTENSITY_PER_M3 * ghost.volume()).max(BEAM_IN_LIGHT_MIN_INTENSITY);
        light.intensity = full_intensity * progress;
        for child in children {
            let Ok(fade) = faders.get(*child) else {
                continue;
            };
            for handle in &fade.0 {
                if let Some(mut material) = materials.get_mut(handle) {
                    material.base_color.set_alpha(progress);
                }
            }
        }
    }
}

// Continuously emit sparkles at random points inside each ghost's volume.
// Sparkles are world entities, not children — they outlive the ghost and
// fade out on their own after it despawns.
pub fn beam_ghost_sparkle_system(
    mut commands: Commands,
    time: Res<Time>,
    beam_assets: Res<BeamAssets>,
    mut ghosts: Query<(&Transform, &mut BeamInGhost)>,
) {
    let delta = time.delta_secs();
    let mut rng = rng();
    for (transform, mut ghost) in &mut ghosts {
        let sparkles_per_emit = ((ghost.volume() * BEAM_IN_SPARKLES_PER_M3).ceil() as usize).max(1);
        ghost.sparkle_timer -= delta;
        while ghost.sparkle_timer <= 0.0 {
            ghost.sparkle_timer += BEAM_IN_SPARKLE_INTERVAL_SECS;
            for _ in 0..sparkles_per_emit {
                let offset = Vec3::new(
                    rng.random_range(-1.0..1.0) * ghost.half_extents.x,
                    rng.random_range(-1.0..1.0) * ghost.half_extents.y,
                    rng.random_range(-1.0..1.0) * ghost.half_extents.z,
                );
                commands.spawn((
                    Mesh3d(beam_assets.mesh.clone()),
                    MeshMaterial3d(beam_assets.material.clone()),
                    NotShadowCaster,
                    Transform::from_translation(transform.translation + offset),
                    BeamSparkle { elapsed: 0.0 },
                ));
            }
        }
    }
}

// Rising fade-out: sparkles drift up and shrink to nothing.
pub fn beam_sparkles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut sparkles: Query<(Entity, &mut BeamSparkle, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut sparkle, mut transform) in &mut sparkles {
        sparkle.elapsed += delta;
        if sparkle.elapsed >= BEAM_IN_SPARKLE_LIFETIME_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.y += BEAM_IN_SPARKLE_RISE_SPEED * delta;
        transform.scale = Vec3::splat(1.0 - sparkle.elapsed / BEAM_IN_SPARKLE_LIFETIME_SECS);
    }
}
