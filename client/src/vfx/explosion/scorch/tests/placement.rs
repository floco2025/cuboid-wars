use super::{
    super::variants::{ScorchStyle, scorch_variant},
    *,
};
use common::protocol::{Floor, PlateState, Ramp, Wall};
use rand::{SeedableRng, rngs::SmallRng};

const WALL_HEIGHT: f32 = 3.0;

fn floor(x1: f32, z1: f32, x2: f32, z2: f32) -> Floor {
    Floor {
        x1,
        z1,
        x2,
        z2,
        y: 0.0,
        thickness: 0.5,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
    Wall {
        x1,
        z1,
        x2,
        z2,
        width: 0.2,
        y: 0.0,
        height: WALL_HEIGHT,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn carriers(layout: &MapLayout) -> Carriers {
    let mut carriers = Carriers::from_layout(layout);
    carriers.advance(0, &PlateState::default());
    carriers
}

fn style() -> ScorchStyle {
    ScorchStyle::random(1, &mut SmallRng::seed_from_u64(7))
}

// The mark's vertices in its carrier's frame.
fn points(placement: &ScorchPlacement) -> Vec<Vec3> {
    let variant = placement.region.apply(&scorch_variant(0));
    assert!(!variant.triangles.is_empty(), "the mark was cut away entirely");
    variant
        .vertices
        .iter()
        .map(|vertex| {
            placement
                .transform
                .transform_point(Vec3::new(vertex.position.x, 0.0, vertex.position.y))
        })
        .collect()
}

fn ground(layout: &MapLayout, point: Vec3, normal: Vec3, diameter: f32) -> ScorchPlacement {
    let contact = SurfaceContact {
        point,
        normal,
        carrier: CarrierId::WORLD,
    };
    ground_scorch_placement(contact, layout, &carriers(layout), point + Vec3::Y, diameter, style())
}

#[test]
fn wall_cross_section_stops_at_reach_limit() {
    assert!(wall_scorch_diameter(2.0, 1.21, 0.6).is_none());
    assert!(wall_scorch_diameter(2.0, 1.20, 0.6).is_some());
}

#[test]
fn surface_cross_section_shrinks_with_distance() {
    assert_eq!(surface_cross_section_diameter(2.0, 0.0), Some(4.0));
    assert_eq!(surface_cross_section_diameter(2.0, 2.0), None);
    let diameter = surface_cross_section_diameter(2.0, 1.0).expect("surface intersects scorch volume");
    assert!((diameter - 2.0 * 3.0_f32.sqrt()).abs() < 0.001);
}

#[test]
fn ground_mark_ends_at_the_floor_edge() {
    let layout = MapLayout {
        floors: vec![floor(-10.0, -10.0, 10.0, 10.0)],
        ..default()
    };
    let placement = ground(&layout, Vec3::new(9.5, 0.0, 0.0), Vec3::Y, 4.0);
    let points = points(&placement);
    assert!(points.iter().all(|p| p.x <= 10.0 + 1e-3));
    assert!(points.iter().any(|p| p.x > 9.99));
    assert_eq!(placement.transform.scale, Vec3::splat(4.0));
}

#[test]
fn ground_mark_stops_at_a_wall_and_continues_past_its_end() {
    let layout = MapLayout {
        floors: vec![floor(-10.0, -10.0, 10.0, 10.0)],
        walls: vec![wall(-10.0, 1.0, 0.0, 1.0)],
        ..default()
    };
    let placement = ground(&layout, Vec3::new(-1.0, 0.0, 0.0), Vec3::Y, 6.0);
    let points = points(&placement);
    assert!(points.iter().all(|p| p.x > -1e-3 || p.z <= 0.9 + 1e-3));
    assert!(points.iter().any(|p| p.x > 0.2 && p.z > 1.0));
}

#[test]
fn ground_mark_on_a_ramp_ends_at_its_top() {
    let ramp = Ramp {
        x1: 0.0,
        y1: 0.0,
        z1: -2.0,
        x2: 4.0,
        y2: 2.0,
        z2: 2.0,
        carrier: CarrierId::WORLD,
    };
    let layout = MapLayout {
        floors: vec![floor(4.0, -2.0, 8.0, 2.0)],
        ramps: vec![ramp],
        ..default()
    };
    let normal = Vec3::new(-0.5, 1.0, 0.0).normalize();
    let placement = ground(&layout, Vec3::new(3.5, 1.75, 0.0), normal, 3.0);
    let points = points(&placement);
    assert!(points.iter().all(|p| p.x <= 4.0 + 1e-3 && p.x >= -1e-3));
    assert!(points.iter().any(|p| p.x > 3.99));
}

#[test]
fn wall_mark_is_cut_to_the_wall_rectangle() {
    let layout = MapLayout {
        walls: vec![wall(-2.0, 1.0, 2.0, 1.0)],
        ..default()
    };
    let placements = wall_scorch_placements(&layout, &carriers(&layout), Vec3::new(1.5, 2.5, 0.0), 2.0, 1.0, style());
    assert_eq!(placements.len(), 1);
    let placement = &placements[0];
    assert_eq!(placement.transform.scale, Vec3::splat(placement.transform.scale.x));
    let points = points(placement);
    assert!(points.iter().all(|p| {
        p.x >= -2.0 - 1e-3
            && p.x <= 2.0 + 1e-3
            && p.y >= -1e-3
            && p.y <= WALL_HEIGHT + 1e-3
            && (p.z - 0.885).abs() < 1e-3
    }));
    assert!(points.iter().any(|p| p.x > 1.99));
    assert!(points.iter().any(|p| p.y > WALL_HEIGHT - 0.01));
}

#[test]
fn a_nearer_wall_shadows_the_mark_on_the_wall_behind() {
    let layout = MapLayout {
        walls: vec![wall(-1.0, 1.0, 1.0, 1.0), wall(-10.0, 3.0, 10.0, 3.0)],
        ..default()
    };
    let placements = wall_scorch_placements(&layout, &carriers(&layout), Vec3::new(0.0, 1.0, 0.0), 5.0, 1.0, style());
    assert_eq!(placements.len(), 2);
    let behind = placements
        .iter()
        .find(|placement| placement.transform.translation.z > 2.0)
        .expect("mark on the far wall");
    let behind_points = points(behind);
    assert!(behind_points.iter().all(|p| p.x.abs() >= 1.0 - 1e-3));
    assert!(behind_points.iter().any(|p| p.x.abs() > 1.01));
    let near = placements
        .iter()
        .find(|placement| placement.transform.translation.z < 2.0)
        .expect("mark on the near wall");
    assert!(points(near).iter().any(|p| p.x.abs() < 0.5));
}
