use common::protocol::CarrierId;
use std::f32::consts::FRAC_PI_2;

use super::*;
use crate::portals::PortalInfo;
use common::constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH};

fn perspective() -> Projection {
    let mut projection = Projection::Perspective(PerspectiveProjection {
        fov: FRAC_PI_2,
        ..default()
    });
    projection.update(1000.0, 1000.0);
    projection
}

fn portal(pair: u32, end: PortalEnd, pos: Vec3, normal: Vec3) -> Portal {
    Portal {
        pair: PortalPairId(pair),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    }
}

fn portal_map(portals: &[Portal]) -> PortalMap {
    let mut map = PortalMap::default();
    for portal in portals {
        map.insert(
            (portal.pair, portal.end),
            PortalInfo {
                entity: Entity::PLACEHOLDER,
                portal: *portal,
            },
        );
    }
    map
}

// A facing pair 8 m apart on the z axis, the camera between them looking at A.
fn facing_pair() -> (PortalMap, PortalKey, PortalKey) {
    let a = portal(1, PortalEnd::A, Vec3::new(0.0, 1.0, -4.0), Vec3::Z);
    let b = portal(1, PortalEnd::B, Vec3::new(0.0, 1.0, 4.0), Vec3::NEG_Z);
    (portal_map(&[a, b]), (a.pair, a.end), (b.pair, b.end))
}

#[test]
fn projected_portal_footprint_shrinks_with_distance() {
    let camera = Transform::IDENTITY;
    let projection = perspective();
    let near = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -2.0), Vec3::Z, 0.0);
    let far = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -8.0), Vec3::Z, 0.0);

    let near_size = visible_aperture(&near, &camera, &projection, Vec2::splat(1000.0))
        .expect("near portal is on screen")
        .footprint;
    let far_size = visible_aperture(&far, &camera, &projection, Vec2::splat(1000.0))
        .expect("far portal is on screen")
        .footprint;

    assert!(near_size.x > far_size.x);
    assert!(near_size.y > far_size.y);
}

#[test]
fn portal_behind_camera_has_no_projected_footprint() {
    let projection = perspective();
    let portal = PortalFrame::from_surface(Vec3::new(0.0, 0.0, 2.0), Vec3::NEG_Z, 0.0);

    assert!(visible_aperture(&portal, &Transform::IDENTITY, &projection, Vec2::splat(1000.0)).is_none());
}

#[test]
fn distant_aperture_renders_whole_and_close_aperture_renders_only_the_visible_part() {
    let projection = perspective();
    let portal = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);

    let distant = visible_aperture(
        &portal,
        &Transform::from_xyz(0.0, 0.0, 5.0),
        &projection,
        Vec2::splat(1000.0),
    )
    .expect("distant portal is on screen");
    assert!(distant.footprint.y < 1000.0);
    assert!(
        (distant.rect.min - full_aperture().min).length() < 1e-3,
        "{:?}",
        distant.rect
    );
    assert!(
        (distant.rect.max - full_aperture().max).length() < 1e-3,
        "{:?}",
        distant.rect
    );

    // Half a metre out with a 90° lens sees ±0.5 m of the aperture.
    let close = visible_aperture(
        &portal,
        &Transform::from_xyz(0.0, 0.0, 0.5),
        &projection,
        Vec2::splat(1000.0),
    )
    .expect("close portal is on screen");
    assert_eq!(close.footprint, Vec2::splat(1000.0));
    assert!((close.rect.min - Vec2::splat(-0.5)).length() < 1e-3, "{:?}", close.rect);
    assert!((close.rect.max - Vec2::splat(0.5)).length() < 1e-3, "{:?}", close.rect);
}

#[test]
fn aperture_corner_behind_the_eye_is_clipped_not_abandoned() {
    let projection = perspective();
    let portal = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
    // Hugging the wall, turned along it: the near corner is behind the eye plane.
    let camera = Transform::from_xyz(0.5, 0.0, 0.15).looking_to(Vec3::new(-1.0, 0.0, -0.3).normalize(), Vec3::Y);

    let visible = visible_aperture(&portal, &camera, &projection, Vec2::splat(1000.0))
        .expect("far side of the aperture is on screen");
    assert!(visible.rect.max.x < PORTAL_HALF_WIDTH, "{:?}", visible.rect);
    assert!(visible.rect.min.x >= -PORTAL_HALF_WIDTH - 1e-4, "{:?}", visible.rect);
    assert!(
        visible.footprint.x > 0.0 && visible.footprint.x <= 1000.0,
        "{:?}",
        visible.footprint
    );
    assert!(
        visible.footprint.y > 0.0 && visible.footprint.y <= 1000.0,
        "{:?}",
        visible.footprint
    );
}

#[test]
fn uv_transform_maps_the_disc_onto_the_rendered_rect() {
    let identity = aperture_uv_transform(full_aperture());
    assert!((identity.transform_point2(Vec2::new(0.25, 0.75)) - Vec2::new(0.25, 0.75)).length() < 1e-6);

    // The upper-right quadrant of the aperture (disc UV u > 0.5, v < 0.5).
    let quadrant = aperture_uv_transform(Rect::new(0.0, 0.0, PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT));
    assert!((quadrant.transform_point2(Vec2::new(0.5, 0.5)) - Vec2::new(0.0, 1.0)).length() < 1e-6);
    assert!((quadrant.transform_point2(Vec2::new(1.0, 0.0)) - Vec2::new(1.0, 0.0)).length() < 1e-6);
}

#[test]
fn view_is_active_only_while_the_aperture_is_on_screen() {
    let (portals, key_a, _) = facing_pair();
    let projection = perspective();
    let looking_at_a = Transform::from_xyz(0.0, 1.0, 0.0);
    let looking_aside = Transform::from_xyz(-2.0, 1.0, 0.0).looking_to(Vec3::X, Vec3::Y);

    let carriers = Carriers::default();
    assert!(
        view_through_chain(
            &portals,
            &[key_a],
            &carriers,
            0.0,
            &looking_at_a,
            &projection,
            UVec2::splat(1000)
        )
        .is_some()
    );
    assert!(
        view_through_chain(
            &portals,
            &[key_a],
            &carriers,
            0.0,
            &looking_aside,
            &projection,
            UVec2::splat(1000)
        )
        .is_none()
    );
}

#[test]
fn nested_view_continues_through_the_far_portal_but_never_its_own_exit() {
    let (portals, key_a, key_b) = facing_pair();
    let projection = perspective();
    let camera = Transform::from_xyz(0.0, 1.0, 0.0);

    let carriers = Carriers::default();
    let (mapped, _, _, _) = view_through_chain(
        &portals,
        &[key_a, key_a],
        &carriers,
        0.0,
        &camera,
        &projection,
        UVec2::splat(1000),
    )
    .expect("second look through A is in view");
    assert!(
        mapped.translation.distance(Vec3::new(0.0, 1.0, 16.0)) < 1e-4,
        "{mapped:?}"
    );
    assert!(
        view_through_chain(
            &portals,
            &[key_a, key_b],
            &carriers,
            0.0,
            &camera,
            &projection,
            UVec2::splat(1000)
        )
        .is_none()
    );
}

#[test]
fn texture_size_follows_each_axis_of_the_footprint() {
    assert_eq!(
        adaptive_portal_resolution(Vec2::new(1000.0, 300.0), UVec2::splat(64)),
        UVec2::new(1024, 512)
    );
    assert_eq!(
        adaptive_portal_resolution(Vec2::ZERO, UVec2::splat(2048)),
        UVec2::splat(64)
    );
    assert_eq!(
        adaptive_portal_resolution(Vec2::splat(4000.0), UVec2::splat(64)),
        UVec2::splat(2048)
    );
}

fn mapped(chain: &[PortalKey], footprint: Vec2) -> MappedView {
    MappedView {
        entity: Entity::PLACEHOLDER,
        presenter: Entity::PLACEHOLDER,
        chain: chain.to_vec(),
        transform: Transform::IDENTITY,
        projection: Projection::default(),
        footprint,
        rect: full_aperture(),
    }
}

#[test]
fn budget_admits_the_largest_views_first() {
    let views = [
        mapped(&[(PortalPairId(1), PortalEnd::A)], Vec2::new(50.0, 100.0)),
        mapped(&[(PortalPairId(2), PortalEnd::A)], Vec2::new(200.0, 400.0)),
        mapped(&[(PortalPairId(3), PortalEnd::A)], Vec2::new(100.0, 200.0)),
    ];
    assert_eq!(admit_views(&views, 2), vec![1, 2]);
    assert_eq!(admit_views(&views, 0), Vec::<usize>::new());
}

#[test]
fn nested_view_is_admitted_only_under_its_parent() {
    let a = (PortalPairId(1), PortalEnd::A);
    let b = (PortalPairId(2), PortalEnd::A);
    let views = [
        mapped(&[a], Vec2::new(10.0, 20.0)),
        mapped(&[a, a], Vec2::new(10.0, 20.0)),
        mapped(&[b], Vec2::new(100.0, 200.0)),
    ];
    assert_eq!(admit_views(&views, 1), vec![2]);
    assert_eq!(admit_views(&views, 2), vec![2, 0]);
    assert_eq!(admit_views(&views, 3), vec![2, 0, 1]);
}

#[test]
fn orphaned_nested_view_is_skipped() {
    let a = (PortalPairId(1), PortalEnd::A);
    let b = (PortalPairId(2), PortalEnd::A);
    let views = [
        mapped(&[a, a], Vec2::new(300.0, 600.0)),
        mapped(&[b], Vec2::new(10.0, 20.0)),
    ];
    assert_eq!(admit_views(&views, 2), vec![1]);
}

#[test]
fn texture_size_shrink_has_hysteresis() {
    assert_eq!(axis_size(450.0, 1024), 1024);
    assert_eq!(axis_size(400.0, 1024), 512);
    assert_eq!(axis_size(300.0, 128), 512);
}

#[test]
fn root_selection_uses_visible_size_not_portal_pair_order() {
    let portals = [
        portal(1, PortalEnd::A, Vec3::new(0.0, 0.0, -8.0), Vec3::Z),
        portal(1, PortalEnd::B, Vec3::new(0.0, 0.0, 20.0), Vec3::NEG_Z),
        portal(2, PortalEnd::A, Vec3::new(0.0, 0.0, -2.0), Vec3::Z),
        portal(2, PortalEnd::B, Vec3::new(0.0, 0.0, 24.0), Vec3::NEG_Z),
    ];
    let map = portal_map(&portals);

    assert_eq!(
        largest_visible_roots(
            &map,
            &portals,
            &Carriers::default(),
            0.0,
            &Transform::IDENTITY,
            &perspective(),
            UVec2::splat(1000),
            1,
        ),
        vec![(PortalPairId(2), PortalEnd::A)]
    );
}

#[test]
fn recursive_camera_layers_stay_within_render_layer_capacity() {
    assert_eq!(RENDER_LAYER_PORTAL_VIEW_START + MAX_PORTAL_VIEW_CAMERAS - 1, 63);
}
