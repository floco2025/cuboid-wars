use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use common::{health::health_ratio, protocol::Health};

use crate::constants::{
    HEALTH_BAR_FILL_COLOR, HEALTH_BAR_FILL_Z_OFFSET, HEALTH_BAR_TRACK_COLOR, LABEL_BACKGROUND_COLOR,
    LABEL_PLAYER_MESH_WIDTH, LABEL_TEXT_COLOR, LABEL_TEXT_PADDING_X, LABEL_TEXT_PADDING_Y,
};

// Marker for a billboarded label quad in world space — a player name's textured
// rectangle or a health bar's track quad. Queried by
// `floating_labels_billboard_system` to face the main camera each frame.
#[derive(Component)]
pub struct CharacterLabelMeshMarker;

// Markers for the name text and its padded background inside the label's
// render-target UI tree. `floating_label_scale_compensation_system` rewrites
// their font size / padding to cancel the global HUD `UiScale`, which would
// otherwise scale the fixed-size label texture's layout.
#[derive(Component)]
pub struct FloatingLabelTextMarker;

#[derive(Component)]
pub struct FloatingLabelPaddingMarker;

// Lives on a player entity. Holds the dedicated `camera` that renders the
// player's name into a texture, plus the `ui_root` of that render-target UI
// tree. Both are top-level entities, not children of the player, so the
// `on_remove` hook despawns them with the player — otherwise the camera (its
// live render pass + target image) and the UI tree leak on every logoff.
// (Health bars are plain geometry now — see `spawn_floating_health_bar` — so
// this is only the static name label.)
#[derive(Component, Clone, Copy)]
#[component(on_remove = despawn_label_render_target)]
pub struct LabelCamera {
    pub camera: Entity,
    pub ui_root: Entity,
    // Frames the `camera` should keep rendering, primed at spawn and counted
    // down to 0 (idle). The name never changes, so it only needs rendering
    // once; a multi-frame window (not a single frame) makes that render land
    // reliably, then the camera idles forever.
    pub render_ttl: u8,
}

fn despawn_label_render_target(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&LabelCamera { camera, ui_root, .. }) = world.get::<LabelCamera>(ctx.entity) else {
        return;
    };
    let mut commands = world.commands();
    commands.entity(camera).try_despawn();
    commands.entity(ui_root).try_despawn();
}

// Renders just the player's name into the render-target texture (the bar is
// separate geometry now). Returns the UI root and the billboarded name quad,
// the latter positioned with its bottom edge at `name_bottom_y` so the caller
// can stack it directly above the health bar.
pub fn spawn_floating_player_label(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    label: &str,
    image_handle: Handle<Image>,
    text_camera: Entity,
    name_bottom_y: f32,
    font_size: f32,
) -> (Entity, Entity) {
    use crate::constants::{LABEL_PLAYER_TEXTURE_HEIGHT, LABEL_PLAYER_TEXTURE_WIDTH};
    const LABEL_HEIGHT: f32 =
        LABEL_PLAYER_MESH_WIDTH * (LABEL_PLAYER_TEXTURE_HEIGHT as f32 / LABEL_PLAYER_TEXTURE_WIDTH as f32);

    let text_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTargetCamera(text_camera),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(LABEL_TEXT_PADDING_X), Val::Px(LABEL_TEXT_PADDING_Y)),
                        ..default()
                    },
                    BackgroundColor(LABEL_BACKGROUND_COLOR),
                    FloatingLabelPaddingMarker,
                ))
                .with_children(|label_background| {
                    label_background.spawn((
                        Text::new(label),
                        TextFont {
                            font_size: FontSize::Px(font_size),
                            ..default()
                        },
                        TextColor(LABEL_TEXT_COLOR),
                        TextLayout::no_wrap(),
                        FloatingLabelTextMarker,
                    ));
                });
        })
        .id();

    let mesh_entity = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(meshes.add(Rectangle::new(LABEL_PLAYER_MESH_WIDTH, LABEL_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(0.0, name_bottom_y + LABEL_HEIGHT / 2.0, 0.0),
        ))
        .id();

    (text_entity, mesh_entity)
}

// Drives the green fill quad of a character's world-space health bar.
// `floating_health_bar_fill_system` reads the tracked character's `Health` and
// rescales the quad; `full_width` is the track width so the fill stays
// anchored to the left edge as it shrinks.
#[derive(Component)]
pub struct FloatingHealthBarFill {
    pub tracked_entity: Entity,
    pub max_health: f32,
    pub full_width: f32,
}

// World-space health bar (players and actors), built from plain billboarded
// geometry: a dark track quad with a green fill quad anchored to its left edge,
// scaled on X by the health ratio. Unlike the player name (which needs a
// texture for text), a bar is just two rectangles, so it deliberately avoids
// the render-target-camera path: a per-character render-target camera both
// intermittently failed to redraw its texture (the bar updated only
// "sometimes") and cost a render pass per character every frame it was active.
// Direct geometry renders through the main camera, so the bar always reflects
// the latest `Health` and costs nothing beyond the two quads.
//
// `world_width` / `world_height` are the quad's size in meters and `y_center`
// its local height above the character. Returns the track entity (parented to
// the character); the fill is its child, so both despawn with the character via
// the recursive despawn.
pub fn spawn_floating_health_bar(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    character: Entity,
    world_width: f32,
    world_height: f32,
    y_center: f32,
    max_health: f32,
    current_health: f32,
) -> Entity {
    let bar_mesh = meshes.add(Rectangle::new(world_width, world_height));

    let ratio = health_ratio(Health(current_health), max_health);
    let fill = commands
        .spawn((
            FloatingHealthBarFill {
                tracked_entity: character,
                max_health,
                full_width: world_width,
            },
            Mesh3d(bar_mesh.clone()),
            // Opaque, not blended: the fill must layer over the translucent
            // track deterministically. As a character loses health the fill's
            // center shifts left, so a blended fill sorts unpredictably against
            // the track and the black tints the green dark from some angles. An
            // opaque fill depth-writes and always wins, giving pure green.
            MeshMaterial3d(materials.add(bar_material(HEALTH_BAR_FILL_COLOR, AlphaMode::Opaque))),
            floating_health_bar_fill_transform(ratio, world_width),
        ))
        .id();

    let track = commands
        .spawn((
            CharacterLabelMeshMarker,
            Mesh3d(bar_mesh),
            // Blended: the track is intentionally translucent so the scene
            // shows through the empty part of the bar.
            MeshMaterial3d(materials.add(bar_material(HEALTH_BAR_TRACK_COLOR, AlphaMode::Blend))),
            Transform::from_xyz(0.0, y_center, 0.0),
        ))
        .id();
    commands.entity(track).add_child(fill);
    track
}

// The fill quad is centered geometry; to anchor its left edge and shrink
// rightward, scale X by `ratio` and shift left by half the removed width. The
// small +Z offset puts it just in front of the track (local +Z faces the
// camera once billboarded) so it draws over it.
pub(super) fn floating_health_bar_fill_transform(ratio: f32, full_width: f32) -> Transform {
    let ratio = ratio.clamp(0.0, 1.0);
    Transform::from_xyz(-full_width * (1.0 - ratio) / 2.0, 0.0, HEALTH_BAR_FILL_Z_OFFSET)
        .with_scale(Vec3::new(ratio, 1.0, 1.0))
}

fn bar_material(color: Color, alpha_mode: AlphaMode) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        alpha_mode,
        unlit: true,
        ..default()
    }
}
