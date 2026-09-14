use bevy::prelude::*;

// Mirrors the cumulus layer in `materials/sky.wgsl` (`hash21`, `cloud_noise`,
// `cloud_fbm`, `cloud_layer_position`, `cumulus_density`, `sample_cumulus`)
// term for term, so the sun dims exactly where the sky shows a cloud in front
// of it. `fract` here is the shader's `x - floor(x)`, never Rust's.

fn fract(value: f32) -> f32 {
    value - value.floor()
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(p: Vec2) -> f32 {
    let mut p3 = Vec3::new(fract(p.x * 0.1031), fract(p.y * 0.1030), fract(p.x * 0.0973));
    p3 += Vec3::splat(p3.dot(Vec3::new(p3.y, p3.z, p3.x) + Vec3::splat(33.33)));
    fract((p3.x + p3.y) * p3.z)
}

fn noise(p: Vec2) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let bottom = hash(cell).lerp(hash(cell + Vec2::X), u.x);
    let top = hash(cell + Vec2::Y).lerp(hash(cell + Vec2::ONE), u.x);
    bottom.lerp(top, u.y)
}

fn next_octave(p: Vec2) -> Vec2 {
    Vec2::new(0.8 * p.x + 0.6 * p.y, -0.6 * p.x + 0.8 * p.y) * 2.02 + Vec2::new(17.3, 9.1)
}

fn fbm(mut p: Vec2, octaves: u32) -> f32 {
    let mut amplitude = 0.5;
    let mut total = 0.0;
    let mut range = 0.0;
    for _ in 0..octaves {
        total += noise(p) * amplitude;
        range += amplitude;
        p = next_octave(p);
        amplitude *= 0.5;
    }
    total / range
}

fn layer_position(direction: Vec3, height: f32, scale: f32) -> Vec2 {
    let planet_radius = 60.0;
    let b = planet_radius * direction.y;
    let distance = -b + (b * b + 2.0 * planet_radius * height + height * height).max(0.0).sqrt();
    Vec2::new(direction.x, direction.z) * distance * scale
}

fn density(p: Vec2, coverage: f32, detail_weight: f32) -> f32 {
    let warp = Vec2::new(
        fbm(p * 0.35 + Vec2::new(5.2, 1.3), 3),
        fbm(p * 0.35 + Vec2::new(9.7, 6.1), 3),
    ) - Vec2::splat(0.5);
    let q = p + warp * 1.2;
    let base = fbm(q, 5);
    let threshold = 0.70_f32.lerp(0.38, coverage);
    let shape = smoothstep(threshold, threshold + 0.12, base);
    let detail = fbm(q * 3.7 + Vec2::new(3.1, 8.4), 3);
    (shape - (1.0 - shape) * detail * 0.6 * detail_weight).clamp(0.0, 1.0)
}

// Cloud density on the line of sight toward `direction` at real time
// `seconds`, in the sky shader's units: `scale` and `wind_radians_per_sec`
// are the material's `clouds.z` and `clouds.w`.
#[must_use]
pub fn cumulus_toward(direction: Vec3, seconds: f32, coverage: f32, scale: f32, wind_radians_per_sec: f32) -> f32 {
    let wind = Vec2::new(1.0, 0.31).normalize() * seconds * wind_radians_per_sec;
    let position = layer_position(direction, 1.0, scale) + wind + Vec2::new(31.0, -17.0);
    let detail_weight = smoothstep(0.0, 0.25, direction.y);
    density(position, coverage, detail_weight)
}

#[cfg(test)]
#[path = "tests/clouds.rs"]
mod tests;
