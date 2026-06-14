use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use super::BumpFlashState;
use crate::{
    animations::{AnimationToPlay, character_animation_system},
    characters::{PreviousTickPosition, spawn_collider_box},
    config::{AssetSet, ClientSettings},
    constants::{
        LABEL_PLAYER_BAR_WIDTH, LABEL_PLAYER_NAME_GAP, LABEL_PLAYER_TEXTURE_HEIGHT, LABEL_PLAYER_TEXTURE_WIDTH,
        LABEL_RENDER_FRAMES,
    },
    ui::floating_labels::{LabelCamera, setup_label_texture, spawn_floating_health_bar, spawn_floating_player_label},
};
use common::{config::GameplayConfig, physics::CharacterVerticalVelocity, protocol::*};

// Marks the local-player entity (the player you control). Spawned by
// `spawn_player` when `is_local` is true; queried by input, camera, UI,
// and effect systems that should only act on the local player.
#[derive(Component)]
pub struct LocalPlayerMarker;

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct PlayerBundle {
    player_id: PlayerId,
    player_marker: PlayerMarker,
    position: Position,
    move_intent: PlayerMoveIntent,
    motion: CharacterVerticalVelocity,
    health: Health,
    face_direction: FaceDirection,
    transform: Transform,
    visibility: Visibility,
}

// ============================================================================
// Player Spawning
// ============================================================================

// Spawn a player model plus cosmetic children, returning the new entity id.
pub fn spawn_player(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    asset_set: &AssetSet,
    client_settings: &ClientSettings,
    gameplay_config: &GameplayConfig,
    player_id: u32,
    player_name: &str,
    position: &Position,
    move_intent: PlayerMoveIntent,
    health: Health,
    face_dir: f32,
    is_local: bool,
) -> Entity {
    let player_model = asset_set.player_model();
    let player_physics = gameplay_config.player.physics();
    let (graph, index) = AnimationGraph::from_clip(
        asset_server
            .load(GltfAssetLabel::Animation(player_model.animation_index).from_asset(player_model.scene.clone())),
    );
    let graph_handle = graphs.add(graph);
    let animation_to_play = AnimationToPlay {
        graph_handle,
        index,
        speed: player_model.animation_speed.unwrap_or(1.0),
    };

    let entity = commands
        .spawn((
            PlayerBundle {
                player_id: PlayerId(player_id),
                player_marker: PlayerMarker,
                position: *position,
                move_intent,
                motion: CharacterVerticalVelocity::default(),
                health,
                face_direction: FaceDirection(face_dir),
                transform: Transform::from_xyz(position.x, player_physics.collider_center_y(position.y), position.z)
                    .with_rotation(Quat::from_rotation_y(face_dir)),
                visibility: player_visibility(is_local),
            },
            PreviousTickPosition(*position),
            animation_to_play.clone(),
        ))
        .id();

    if is_local {
        commands
            .entity(entity)
            .insert((LocalPlayerMarker, BumpFlashState::default()));
    }

    let mut children = vec![];

    // Add transparent collider visualization if enabled.
    if client_settings.debug.collider_boxes {
        children.push(spawn_collider_box(commands, meshes, materials, player_physics));
    }

    let base_y = player_physics.model_y_offset_from_entity_center(player_model.y_offset);
    let model = commands
        .spawn((
            SceneRoot(asset_server.load(player_model.scene.clone())),
            Transform::from_scale(Vec3::splat(player_model.scale))
                .with_rotation(Quat::from_rotation_x(player_model.x_rotation_degrees.to_radians()))
                .with_translation(Vec3::new(player_model.x_offset, base_y, player_model.z_offset)),
            animation_to_play,
        ))
        .observe(character_animation_system)
        .id();
    children.push(model);

    let health_bars = client_settings.hud.health_bars;
    let height_above = client_settings.hud.floating_labels.height_above_character;

    // Floating health bar: plain billboarded geometry (track + fill quads), the
    // same approach actors use — see `spawn_floating_health_bar`. Sits just
    // above the head.
    let bar_width = LABEL_PLAYER_BAR_WIDTH;
    let bar_height = bar_width * (health_bars.floating_player_height / health_bars.floating_player_width);
    let bar_y = player_physics.collision_height() / 2.0 + height_above + bar_height / 2.0;
    let bar_entity = spawn_floating_health_bar(
        commands,
        meshes,
        materials,
        entity,
        bar_width,
        bar_height,
        bar_y,
        gameplay_config.player.health().max,
        health.0,
    );
    children.push(bar_entity);

    // Name label: rendered into its own texture (text needs one), stacked just
    // above the health bar.
    let (image_handle, text_camera) = setup_label_texture(
        commands,
        images,
        LABEL_PLAYER_TEXTURE_WIDTH,
        LABEL_PLAYER_TEXTURE_HEIGHT,
    );
    let name_bottom_y = bar_y + bar_height / 2.0 + LABEL_PLAYER_NAME_GAP;
    let (text_entity, name_mesh) = spawn_floating_player_label(
        commands,
        meshes,
        materials,
        player_name,
        image_handle,
        text_camera,
        name_bottom_y,
        client_settings.hud.font_sizes.floating_label,
    );
    children.push(name_mesh);

    // The visibility system uses `camera` to render the name texture for the
    // first frames after spawn; `ui_root` is tracked so both despawn with the
    // player (see the `LabelCamera` on_remove hook).
    commands.entity(entity).insert(LabelCamera {
        camera: text_camera,
        ui_root: text_entity,
        render_ttl: LABEL_RENDER_FRAMES,
    });
    commands.entity(entity).add_children(&children);

    entity
}

const fn player_visibility(is_local: bool) -> Visibility {
    if is_local {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}
