use super::{super::scorch::ScorchMark, *};
use crate::{map::GrassBurn, test_fixtures::WALL_HEIGHT};
use common::protocol::{BarrierKindTable, Carrier, CarrierId, Floor, MapLayout, Wall};

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

#[test]
fn density_scales_particle_count_and_zero_disables_it() {
    let full = scaled_particle_count(
        10.0,
        EXPLOSION_SHARDS_PER_RADIUS_METER,
        EXPLOSION_REFERENCE_SHARDS_PER_METER,
        EXPLOSION_SHARD_MIN_COUNT,
        EXPLOSION_SHARD_MAX_COUNT,
    );
    let half = scaled_particle_count(
        10.0,
        EXPLOSION_SHARDS_PER_RADIUS_METER / 2.0,
        EXPLOSION_REFERENCE_SHARDS_PER_METER,
        EXPLOSION_SHARD_MIN_COUNT,
        EXPLOSION_SHARD_MAX_COUNT,
    );
    assert_eq!(half, full / 2);
    let disabled = scaled_particle_count(
        10.0,
        0.0,
        EXPLOSION_REFERENCE_SHARDS_PER_METER,
        EXPLOSION_SHARD_MIN_COUNT,
        EXPLOSION_SHARD_MAX_COUNT,
    );
    assert_eq!(disabled, 0);
}

// The marks an explosion at `center` leaves on `map_layout`, with one
// root entity per carrier, at the carriers' pose at tick 0.
fn explode(map_layout: &MapLayout, center: Vec3, blast_radius: f32) -> (World, Vec<Entity>) {
    let mut collision_world = CollisionWorld::from_map_layout(map_layout, &BarrierKindTable::default());
    let mut carriers = Carriers::from_layout(map_layout);
    carriers.advance(0);
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
                collision_world: Some(&collision_world),
                map_layout: Some(map_layout),
                carriers: &carriers,
                carrier_entities: &carrier_entities,
            },
        );
    }
    queue.apply(&mut world);
    (world, roots)
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
    let (mut world, _) = explode(&map_layout, Vec3::new(0.0, 1.0, 0.0), 15.0);

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
fn explosion_next_to_wall_spawns_wall_scorch_mark() {
    let map_layout = MapLayout {
        walls: vec![wall(CarrierId::WORLD)],
        floors: vec![floor(CarrierId::WORLD)],
        ..default()
    };
    let (mut world, _) = explode(&map_layout, Vec3::new(0.0, 1.0, 0.0), 6.0);

    let mut marks = world.query::<(&ScorchMark, &Transform)>();
    let marks: Vec<_> = marks.iter(&world).collect();
    assert_eq!(marks.len(), 2);
    let wall_transform = marks
        .iter()
        .find_map(|(_, transform)| {
            let surface_normal = transform.rotation * Vec3::Y;
            (surface_normal.dot(Vec3::NEG_Z) > 0.999).then_some(*transform)
        })
        .expect("expected wall-aligned scorch mark");
    assert_eq!(wall_transform.scale.y, 1.0);
    assert!(wall_transform.scale.x > wall_transform.scale.z);
    assert!(wall_transform.translation.y - wall_transform.scale.z * 0.5 < 0.0);
    assert!((wall_transform.translation.y / wall_transform.scale.z).abs() < 0.35);
    assert!(wall_transform.translation.y + wall_transform.scale.z * 0.5 <= WALL_HEIGHT + 0.001);
    drop(marks);
    assert_eq!(world.query::<&GrassBurn>().iter(&world).count(), 1);
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
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: rest,
            to: rest,
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..default()
    };
    let (mut world, roots) = explode(&map_layout, Vec3::new(30.0, 1.0, 0.0), 6.0);

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
