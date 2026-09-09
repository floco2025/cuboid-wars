use bevy::prelude::*;

use super::BumpFeedbackState;
use super::animation::{PlayerAnimationMotion, PlayerModel, player_animation_setup_system};
use crate::{
    cameras::LocalPlayerLabelMarker,
    characters::{PreviousTickPosition, load_character_model, model_transform, spawn_character_bounds},
    config::{AssetSet, ClientSettings},
    constants::{
        LABEL_PLAYER_BAR_WIDTH, LABEL_PLAYER_NAME_GAP, LABEL_PLAYER_TEXTURE_HEIGHT, LABEL_PLAYER_TEXTURE_WIDTH,
    },
    ui::floating_labels::{
        LABEL_RENDER_FRAMES, LabelCamera, setup_label_texture, spawn_floating_health_bar, spawn_floating_player_label,
    },
};
use common::{
    config::GameplayConfig,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{FaceYaw, Health, Player, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

// Marks the local-player entity (the player you control). Spawned by
// `spawn_player` when `is_local` is true; queried by input, camera, UI,
// and effect systems that should only act on the local player.
#[derive(Component)]
pub struct LocalPlayerMarker;

// Where a player's eye sits above its feet-based position.
#[must_use]
pub fn eye_position(feet: Position, eye_height: f32) -> Vec3 {
    Vec3::new(feet.x, feet.y + eye_height, feet.z)
}

// The asset stores and tuning a player spawn draws on.
pub struct PlayerSpawnContext<'a> {
    pub asset_server: &'a AssetServer,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
    pub asset_set: &'a AssetSet,
    pub client_settings: &'a ClientSettings,
    pub gameplay_config: &'a GameplayConfig,
    pub max_health: f32,
}

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
    face_direction: FaceYaw,
    transform: Transform,
    visibility: Visibility,
}

// ============================================================================
// Player Spawning
// ============================================================================

// Spawn a player model plus cosmetic children, returning the new entity id.
pub fn spawn_player(
    commands: &mut Commands,
    context: PlayerSpawnContext,
    id: PlayerId,
    player: &Player,
    is_local: bool,
) -> Entity {
    let PlayerSpawnContext {
        asset_server,
        meshes,
        materials,
        images,
        asset_set,
        client_settings,
        gameplay_config,
        max_health,
    } = context;
    let position = player.movement.pos;
    let face_yaw = player.movement.face_yaw;
    let player_model = asset_set.player_model();
    let player_physics = gameplay_config.player.physics();
    let entity = commands
        .spawn((
            PlayerBundle {
                player_id: id,
                player_marker: PlayerMarker,
                position,
                move_intent: player.movement.move_intent,
                motion: CharacterVerticalVelocity(player.movement.vertical_velocity),
                health: player.health,
                face_direction: FaceYaw(face_yaw),
                transform: Transform::from_xyz(position.x, position.y, position.z)
                    .with_rotation(Quat::from_rotation_y(face_yaw)),
                visibility: Visibility::Visible,
            },
            PreviousTickPosition(position),
            AirborneMomentum(Vec3::from_array(player.movement.airborne_momentum)),
            KnockbackVelocity(Vec3::from_array(player.movement.knockback)),
            PlayerAnimationMotion::default(),
        ))
        .id();

    if is_local {
        commands
            .entity(entity)
            .insert((LocalPlayerMarker, BumpFeedbackState::default()));
    }

    let mut children = vec![];

    children.push(spawn_character_bounds(commands, meshes, materials, player_physics));

    let model = commands
        .spawn((
            load_character_model(player_model, asset_server),
            model_transform(player_model),
            PlayerModel { owner: entity },
        ))
        .observe(player_animation_setup_system)
        .id();
    children.push(model);

    let health_bars = client_settings.hud.health_bars;
    let height_above = client_settings.hud.floating_labels.height_above;

    // Floating health bar: plain billboarded geometry (track + fill quads), the
    // same approach actors use — see `spawn_floating_health_bar`. Sits just
    // above the head.
    let bar_width = LABEL_PLAYER_BAR_WIDTH;
    let bar_height = bar_width * health_bars.player_aspect;
    let bar_y = player_physics.hitbox.top_y_offset() + height_above + bar_height / 2.0;
    let bar_entity = spawn_floating_health_bar(
        commands,
        meshes,
        materials,
        entity,
        bar_width,
        bar_height,
        bar_y,
        max_health,
        player.health.0,
    );
    if is_local {
        commands
            .entity(bar_entity)
            .insert((LocalPlayerLabelMarker, Visibility::Hidden));
    }
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
        &player.name,
        image_handle,
        text_camera,
        name_bottom_y,
        client_settings.hud.font_sizes.floating_label,
    );
    if is_local {
        commands
            .entity(name_mesh)
            .insert((LocalPlayerLabelMarker, Visibility::Hidden));
    }
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
