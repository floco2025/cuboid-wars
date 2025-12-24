use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use crate::{
    constants::*,
    markers::*,
    systems::animations::{AnimationToPlay, sentries_animation_system},
};
use common::{constants::*, markers::SentryMarker, protocol::*};

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct SentryBundle {
    sentry_id: SentryId,
    sentry_marker: SentryMarker,
    position: Position,
    velocity: Velocity,
    face_direction: FaceDirection,
    transform: Transform,
}

// ============================================================================
// Sentry Spawning
// ============================================================================

// Spawn a sentry cube
pub fn spawn_sentry(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    sentry_id: SentryId,
    position: &Position,
    velocity: &Velocity,
) -> Entity {
    // Create animation graph with walk animation
    let mut graph = AnimationGraph::new();
    let walk_clip = asset_server.load(GltfAssetLabel::Animation(SENTRY_WALK_ANIMATION_INDEX).from_asset(SENTRY_MODEL));
    let walk_index = graph.add_clip(walk_clip, 1.0, graph.root);

    let graph_handle = graphs.add(graph);

    let animation_to_play = AnimationToPlay {
        graph_handle,
        index: walk_index,
    };

    // Calculate initial face direction from velocity
    let face_dir = if velocity.x.abs() > 0.01 || velocity.z.abs() > 0.01 {
        velocity.x.atan2(velocity.z)
    } else {
        0.0 // Default facing direction when stopped
    };

    let entity = commands
        .spawn((
            SentryBundle {
                sentry_id,
                sentry_marker: SentryMarker,
                position: *position,
                velocity: *velocity,
                face_direction: FaceDirection(face_dir),
                transform: Transform::from_xyz(position.x, position.y + SENTRY_HEIGHT / 2.0, position.z)
                    .with_rotation(Quat::from_rotation_y(face_dir)),
            },
            animation_to_play.clone(),
        ))
        .id();

    let mut children = vec![];

    // Add transparent cuboid debug visualization if enabled
    if SENTRY_BOUNDING_BOX {
        let debug_box = commands
            .spawn((
                Mesh3d(meshes.add(Cuboid::new(SENTRY_WIDTH, SENTRY_HEIGHT, SENTRY_DEPTH))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(0.5, 0.5, 0.5, 0.15),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                })),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            ))
            .id();
        children.push(debug_box);
    }

    // Add the GLB sentry model with animation observer
    let base_y = SENTRY_MODEL_HEIGHT_OFFSET - SENTRY_HEIGHT / 2.0;
    let model = commands
        .spawn((
            SceneRoot(asset_server.load(SENTRY_MODEL)),
            Transform::from_scale(Vec3::splat(SENTRY_MODEL_SCALE))
                .with_rotation(Quat::from_rotation_x(std::f32::consts::PI))
                .with_translation(Vec3::new(0.0, base_y, SENTRY_MODEL_DEPTH_OFFSET)),
            animation_to_play,
            SentryModelMarker,
        ))
        .observe(sentries_animation_system)
        .id();
    children.push(model);

    commands.entity(entity).add_children(&children);

    entity
}
