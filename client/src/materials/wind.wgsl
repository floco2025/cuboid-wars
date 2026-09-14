#define_import_path cuboid_wars::wind

// Gusts are a slow noise field travelling downwind: where one passes, blades
// and crowns lean over together and swing harder, so the motion rolls across
// the ground instead of every plant vibrating in place.
const GUST_CELL_METRES: f32 = 16.0;
const GUST_SPEED_METRES_PER_SEC: f32 = 5.5;

fn gust_hash(cell: vec2<f32>) -> f32 {
    var p3 = fract(vec3(cell.xyx) * vec3(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + vec3(33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn gust_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(gust_hash(cell), gust_hash(cell + vec2(1.0, 0.0)), u.x),
        mix(gust_hash(cell + vec2(0.0, 1.0)), gust_hash(cell + vec2(1.0, 1.0)), u.x),
        u.y,
    );
}

// 0 in still air, 1 in the heart of a gust, at a world XZ position.
fn gust_strength(world_xz: vec2<f32>, wind_direction: vec2<f32>, time: f32) -> f32 {
    let travel = wind_direction * time * (GUST_SPEED_METRES_PER_SEC / GUST_CELL_METRES);
    let p = world_xz / GUST_CELL_METRES - travel;
    let field = gust_noise(p) * 0.65 + gust_noise(p * 2.3 + vec2(7.1, 3.7)) * 0.35;
    return smoothstep(0.4, 0.75, field);
}
