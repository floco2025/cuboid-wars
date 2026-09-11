use super::{super::scorch::ScorchMark, *};
use crate::{map::GrassBurn, test_fixtures::WALL_HEIGHT};
use common::protocol::{Carrier, CarrierId, Floor, MapLayout, PlateState, Wall};

#[test]
fn density_scales_particle_count_and_zero_disables_it() {
    let full = scaled_particle_count(10.0, 4.0, 4.0, 2, 100);
    let half = scaled_particle_count(10.0, 2.0, 4.0, 2, 100);
    assert_eq!(half, full / 2);
    assert_eq!(scaled_particle_count(10.0, 0.0, 4.0, 2, 100), 0);
    assert_eq!(scaled_particle_count(0.1, 4.0, 4.0, 2, 100), 2);
    assert_eq!(scaled_particle_count(1000.0, 4.0, 4.0, 2, 100), 100);
}

// The marks an explosion at `center` leaves on `map_layout`, with one
// root entity per carrier, at the carriers' pose at tick 0.
fn explode(map_layout: &MapLayout, center: Vec3, blast_radius: f32) -> (World, Vec<Entity>, Assets<Mesh>) {
    let mut collision_world = CollisionWorld::from_map_layout(map_layout);
    let mut carriers = Carriers::from_layout(map_layout);
    carriers.advance(0, &PlateState::default());
    collision_world.set_carrier_poses(&carriers);
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let explosion_assets = ExplosionAssets::new(&mut meshes, &mut materials);
    let mut budget = ExplosionVfxBudget::default();
    let mut world = World::new();
    let roots: Vec<Entity> = (0..=map_layout.carriers.len())
        .map(|_| world.spawn_empty().id())
        .collect();
    let carrier_entities = CarrierEntities::new(roots.clone());
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
                center,
                ground_y: center.y - 1.0,
                fireball_diameter: blast_radius,
                blast_radius: Some(blast_radius),
            },
            &ExplosionSurfaces {
                collision_world: &collision_world,
                map_layout,
                carriers: &carriers,
                carrier_entities: &carrier_entities,
            },
        );
    }
    queue.apply(&mut world);
    (world, roots, meshes)
}

// Every mark's transform and its vertices in the carrier's frame.
fn mark_points(world: &mut World, meshes: &Assets<Mesh>) -> Vec<(Transform, Vec<Vec3>)> {
    let mut marks = world.query::<(&ScorchMark, &Transform, &Mesh3d)>();
    marks
        .iter(world)
        .map(|(_, transform, mesh)| {
            let positions = meshes
                .get(&mesh.0)
                .and_then(|mesh| mesh.attribute(Mesh::ATTRIBUTE_POSITION))
                .and_then(|values| values.as_float3())
                .expect("scorch mesh positions");
            let points = positions
                .iter()
                .map(|position| transform.transform_point(Vec3::from(*position)))
                .collect();
            (*transform, points)
        })
        .collect()
}

fn floor(carrier: CarrierId) -> Floor {
    Floor {
        x1: -10.0,
        z1: -10.0,
        x2: 10.0,
        z2: 10.0,
        y: 0.0,
        thickness: 1.0,
        level: 0,
        carrier,
    }
}

fn wall(carrier: CarrierId) -> Wall {
    Wall {
        x1: -10.0,
        z1: 1.0,
        x2: 10.0,
        z2: 1.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier,
    }
}

#[test]
fn large_grounded_explosion_spawns_one_sized_scorch_and_grass_burn() {
    let map_layout = MapLayout {
        floors: vec![floor(CarrierId::WORLD)],
        ..default()
    };
    let (mut world, _, _) = explode(&map_layout, Vec3::new(0.0, 1.0, 0.0), 15.0);

    let mut marks = world.query::<(&ScorchMark, &Transform)>();
    let marks: Vec<_> = marks.iter(&world).collect();
    assert_eq!(marks.len(), 1);
    let transform = marks[0].1;
    assert!((transform.translation.y - SCORCH_SURFACE_OFFSET).abs() < 0.001);
    let scorch_radius = 15.0 * EXPLOSION_SCORCH_BLAST_DIAMETER_FACTOR;
    let expected_diameter = 2.0 * scorch_radius.mul_add(scorch_radius, -1.0).sqrt();
    assert_eq!(transform.scale, Vec3::splat(expected_diameter));
    drop(marks);
    assert_eq!(world.query::<&GrassBurn>().iter(&world).count(), 1);
}

#[test]
fn explosion_next_to_wall_marks_the_wall_and_stops_the_floor_mark_at_its_face() {
    let map_layout = MapLayout {
        walls: vec![wall(CarrierId::WORLD)],
        floors: vec![floor(CarrierId::WORLD)],
        ..default()
    };
    let (mut world, _, meshes) = explode(&map_layout, Vec3::new(0.0, 1.0, 0.0), 6.0);

    let marks = mark_points(&mut world, &meshes);
    assert_eq!(marks.len(), 2);
    let (wall_transform, wall_points) = marks
        .iter()
        .find(|(transform, _)| (transform.rotation * Vec3::Y).dot(Vec3::NEG_Z) > 0.999)
        .expect("expected wall-aligned scorch mark");
    assert_eq!(wall_transform.scale, Vec3::splat(wall_transform.scale.x));
    assert!(
        wall_points
            .iter()
            .all(|p| p.y >= -0.001 && p.y <= WALL_HEIGHT + 0.001 && (p.z - 0.885).abs() < 0.001)
    );
    assert!(wall_points.iter().any(|p| p.y < 0.001));
    let (_, floor_points) = marks
        .iter()
        .find(|(transform, _)| (transform.rotation * Vec3::Y).dot(Vec3::Y) > 0.999)
        .expect("floor mark");
    assert!(floor_points.iter().all(|p| p.z <= 0.9 + 0.001));
    assert!(floor_points.iter().any(|p| p.z > 0.899));
    assert_eq!(world.query::<&GrassBurn>().iter(&world).count(), 1);
}

#[test]
fn ground_mark_is_cut_at_the_floor_edge() {
    let map_layout = MapLayout {
        floors: vec![floor(CarrierId::WORLD)],
        ..default()
    };
    let (mut world, _, meshes) = explode(&map_layout, Vec3::new(9.5, 1.0, 0.0), 15.0);

    let marks = mark_points(&mut world, &meshes);
    assert_eq!(marks.len(), 1);
    let (transform, points) = &marks[0];
    let scorch_radius = 15.0 * EXPLOSION_SCORCH_BLAST_DIAMETER_FACTOR;
    let expected_diameter = 2.0 * scorch_radius.mul_add(scorch_radius, -1.0).sqrt();
    assert_eq!(transform.scale, Vec3::splat(expected_diameter));
    assert!(points.iter().all(|p| p.x <= 10.0 + 0.001));
    assert!(points.iter().any(|p| p.x > 9.99));
}

// A carrier resting at x = 30 carries the floor and wall; the blast at
// world x = 30 marks both in the carrier's frame, under its root.
#[test]
fn marks_on_a_carrier_hang_under_it_in_its_frame() {
    let rest = Position {
        x: 30.0,
        y: 0.0,
        z: 0.0,
    };
    let map_layout = MapLayout {
        walls: vec![wall(CarrierId(1))],
        floors: vec![floor(CarrierId(1))],
        carriers: vec![Carrier {
            switch_inverted: false,

            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: rest,
            to: rest,
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        }],
        ..default()
    };
    let (mut world, roots, _) = explode(&map_layout, Vec3::new(30.0, 1.0, 0.0), 6.0);

    let carrier_root = roots[1];
    let mut marks = world.query::<(&ScorchMark, &Transform, &ChildOf)>();
    let marks: Vec<_> = marks.iter(&world).collect();
    assert_eq!(marks.len(), 2);
    assert!(marks.iter().all(|(_, _, child_of)| child_of.parent() == carrier_root));
    let floor_mark = marks
        .iter()
        .find(|(_, transform, _)| (transform.rotation * Vec3::Y).dot(Vec3::Y) > 0.999)
        .expect("floor mark");
    assert!(
        floor_mark.1.translation.x.abs() < 0.001,
        "not in the carrier's frame: {:?}",
        floor_mark.1.translation
    );
    drop(marks);
    let burns: Vec<_> = world.query::<&GrassBurn>().iter(&world).copied().collect();
    assert_eq!(burns.len(), 1);
    assert_eq!(burns[0].carrier, CarrierId(1));
}
