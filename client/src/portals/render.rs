use std::collections::VecDeque;

use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass},
    image::ImageSampler,
    prelude::*,
    render::{render_resource::TextureFormat, view::ColorGrading},
};

use super::{
    PortalMap,
    projection::{PortalProjection, portal_camera_view},
    spawn::{PortalAssets, spawn_portal_replica},
};
use crate::{
    cameras::MainCameraMarker,
    config::ClientSettings,
    constants::{LOCAL_PLAYER_RENDER_LAYER, PORTAL_VIEW_TEXTURE_HEIGHT, PORTAL_VIEW_TEXTURE_WIDTH},
    players::local_player_camera_sync_system,
    schedule::ClientSet,
};
use common::{
    physics::PortalFrame,
    protocol::{PlayerId, Portal, PortalEnd},
};

const FIRST_RECURSIVE_RENDER_LAYER: usize = 3;
const MAX_PORTAL_VIEW_CAMERAS: usize = 64;
const MAX_PORTAL_REPLICAS: usize = 512;

type PortalKey = (PlayerId, PortalEnd);

#[derive(Component)]
struct PortalViewCamera {
    chain: Vec<PortalKey>,
    visibility_path: Vec<Entity>,
}

#[derive(Resource, Default)]
struct PortalRenderState {
    portals: Vec<Portal>,
    recursion_depth: Option<u8>,
    spawned: Vec<Entity>,
}

struct PendingView {
    chain: Vec<PortalKey>,
    visibility_path: Vec<Entity>,
    target_surface: Entity,
    recursion_remaining: u8,
}

pub fn portal_render_plugin(app: &mut App) {
    app.init_resource::<PortalRenderState>();
    app.add_systems(
        Update,
        rebuild_portal_views_system
            .after(ClientSet::Network)
            .before(ClientSet::Camera),
    );
    app.add_systems(
        Update,
        update_portal_view_cameras_system
            .in_set(ClientSet::Camera)
            .after(local_player_camera_sync_system),
    );
}

fn rebuild_portal_views_system(
    mut commands: Commands,
    portals: Res<PortalMap>,
    portal_assets: Res<PortalAssets>,
    client_settings: Res<ClientSettings>,
    mut state: ResMut<PortalRenderState>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wire_portals = portals.wire_portals();
    let recursion_depth = client_settings.rendering.portal_recursion_depth;
    if state.portals == wire_portals && state.recursion_depth == Some(recursion_depth) {
        return;
    }

    for entity in state.spawned.drain(..) {
        commands.entity(entity).despawn();
    }
    for portal in &wire_portals {
        if let Some(info) = portals.get(&(portal.owner, portal.end)) {
            commands
                .entity(info.entity)
                .insert(MeshMaterial3d(portal_assets.material(portal.end)));
        }
    }

    let complete_portals: Vec<_> = wire_portals
        .iter()
        .copied()
        .filter(|portal| paired_portal(&portals, portal).is_some())
        .collect();
    let mut pending = VecDeque::new();
    for portal in &complete_portals {
        let Some(info) = portals.get(&(portal.owner, portal.end)) else {
            continue;
        };
        pending.push_back(PendingView {
            chain: vec![(portal.owner, portal.end)],
            visibility_path: vec![info.entity],
            target_surface: info.entity,
            recursion_remaining: recursion_depth,
        });
    }

    let deferred = client_settings.rendering.opaque_renderer.is_deferred();
    let msaa = if deferred {
        Msaa::Off
    } else {
        Msaa::from_samples(client_settings.rendering.msaa_samples)
    };
    let mut camera_count = 0;
    let mut replica_count = 0;
    while let Some(view) = pending.pop_front() {
        if camera_count >= MAX_PORTAL_VIEW_CAMERAS {
            break;
        }

        let child_layer = FIRST_RECURSIVE_RENDER_LAYER + camera_count;
        let image = create_portal_view_image(&mut images);
        let material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(image.clone()),
            unlit: true,
            ..default()
        });
        commands.entity(view.target_surface).insert(MeshMaterial3d(material));

        let recursion_depth = view.chain.len() - 1;
        let mut camera = commands.spawn((
            PortalViewCamera {
                chain: view.chain.clone(),
                visibility_path: view.visibility_path.clone(),
            },
            Camera3d::default(),
            Camera {
                order: -(recursion_depth as isize) - 1,
                is_active: false,
                ..default()
            },
            RenderTarget::Image(image.into()),
            Projection::custom(PortalProjection::default()),
            ColorGrading::default(),
            RenderLayers::layer(0).with(LOCAL_PLAYER_RENDER_LAYER).with(child_layer),
            msaa,
            Transform::default(),
        ));
        if deferred {
            camera.insert((DepthPrepass, DeferredPrepass));
        }
        state.spawned.push(camera.id());
        camera_count += 1;

        for portal in &complete_portals {
            if replica_count >= MAX_PORTAL_REPLICAS {
                break;
            }
            let replica = spawn_portal_replica(&mut commands, &portal_assets, portal, child_layer);
            state.spawned.push(replica);
            replica_count += 1;

            if view.recursion_remaining > 0 && camera_count + pending.len() < MAX_PORTAL_VIEW_CAMERAS {
                let mut chain = view.chain.clone();
                chain.push((portal.owner, portal.end));
                let mut visibility_path = view.visibility_path.clone();
                visibility_path.push(replica);
                pending.push_back(PendingView {
                    chain,
                    visibility_path,
                    target_surface: replica,
                    recursion_remaining: view.recursion_remaining - 1,
                });
            }
        }
    }

    if !pending.is_empty() || replica_count >= MAX_PORTAL_REPLICAS {
        warn!(
            "portal recursion reached its render budget ({} cameras, {} surfaces)",
            camera_count, replica_count
        );
    }
    state.portals = wire_portals;
    state.recursion_depth = Some(recursion_depth);
}

fn create_portal_view_image(images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new_target_texture(
        PORTAL_VIEW_TEXTURE_WIDTH,
        PORTAL_VIEW_TEXTURE_HEIGHT,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    );
    image.sampler = ImageSampler::linear();
    images.add(image)
}

fn update_portal_view_cameras_system(
    main_camera: Query<(&Transform, &Projection), (With<Camera3d>, With<MainCameraMarker>, Without<PortalViewCamera>)>,
    portals: Res<PortalMap>,
    visibility: Query<&ViewVisibility>,
    mut view_cameras: Query<
        (&PortalViewCamera, &mut Camera, &mut Transform, &mut Projection),
        Without<MainCameraMarker>,
    >,
) {
    let Ok((main_transform, main_projection)) = main_camera.single() else {
        return;
    };

    for (view, mut camera, mut transform, mut projection) in &mut view_cameras {
        let mut eye = main_transform.translation;
        let mut next_view = None;
        for key in &view.chain {
            let Some(entry) = portals.get(key).map(|info| &info.portal) else {
                next_view = None;
                break;
            };
            let Some(exit) = paired_portal(&portals, entry) else {
                next_view = None;
                break;
            };
            let entry_frame = PortalFrame::from_portal(entry);
            let exit_frame = PortalFrame::from_portal(exit);
            next_view = portal_camera_view(eye, &entry_frame, &exit_frame, main_projection.far());
            let Some((next_transform, _)) = &next_view else {
                break;
            };
            eye = next_transform.translation;
        }

        camera.is_active = view
            .visibility_path
            .iter()
            .all(|entity| visibility.get(*entity).is_ok_and(|visible| visible.get()))
            && next_view.is_some();
        if camera.is_active
            && let Some((next_transform, next_projection)) = next_view
        {
            *transform = next_transform;
            *projection = Projection::custom(next_projection);
        }
    }
}

fn paired_portal<'a>(portals: &'a PortalMap, portal: &Portal) -> Option<&'a Portal> {
    let paired_end = match portal.end {
        PortalEnd::A => PortalEnd::B,
        PortalEnd::B => PortalEnd::A,
    };
    portals.get(&(portal.owner, paired_end)).map(|info| &info.portal)
}
