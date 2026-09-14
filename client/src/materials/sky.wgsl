#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
    utils::coords_to_viewport_uv,
}

struct SkyUniform {
    sun_direction: vec4<f32>,
    moon_direction: vec4<f32>,
    pole_rotation: vec4<f32>,
    // x real time, y cloud cover, z moon illuminated fraction
    time_weather_phase: vec4<f32>,
    day_horizon: vec4<f32>,
    day_zenith: vec4<f32>,
    sunset: vec4<f32>,
    twilight_horizon: vec4<f32>,
    twilight_zenith: vec4<f32>,
    night_horizon: vec4<f32>,
    night_zenith: vec4<f32>,
    // angular radius, core luminance, halo radius, halo luminance
    sun: vec4<f32>,
    // angular radius, luminance, earthshine
    moon: vec4<f32>,
    moon_halo: vec4<f32>,
    // seed, density, min luminance, max luminance
    stars: vec4<f32>,
    // bright fraction, twinkle
    star_detail: vec4<f32>,
    // clear coverage, overcast coverage, scale, angular wind speed (radians/sec)
    clouds: vec4<f32>,
    cloud_color: vec4<f32>,
    overcast_color: vec4<f32>,
    cloud_shadow_color: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: SkyUniform;

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + vec3(33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn cloud_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return mix(
        mix(hash21(cell), hash21(cell + vec2(1.0, 0.0)), u.x),
        mix(hash21(cell + vec2(0.0, 1.0)), hash21(cell + vec2(1.0, 1.0)), u.x),
        u.y,
    );
}

// Each octave is rotated as well as scaled so the lattices never line up.
fn next_octave(p: vec2<f32>) -> vec2<f32> {
    return vec2(0.8 * p.x + 0.6 * p.y, -0.6 * p.x + 0.8 * p.y) * 2.02 + vec2(17.3, 9.1);
}

fn cloud_fbm(p_initial: vec2<f32>, octaves: i32) -> f32 {
    var p = p_initial;
    var amplitude = 0.5;
    var total = 0.0;
    var range = 0.0;
    for (var octave = 0; octave < octaves; octave += 1) {
        total += cloud_noise(p) * amplitude;
        range += amplitude;
        p = next_octave(p);
        amplitude *= 0.5;
    }
    return total / range;
}

// Where a view ray meets a cloud layer `height` above the ground on a planet
// sixty layer heights in radius: overhead formations keep their size while
// the horizon lies about eleven heights away, so distant clouds flatten and
// crowd together without stretching to infinity.
fn cloud_layer_position(direction: vec3<f32>, height: f32) -> vec2<f32> {
    let planet_radius = 60.0;
    let b = planet_radius * direction.y;
    let distance = -b + sqrt(max(0.0, b * b + 2.0 * planet_radius * height + height * height));
    return direction.xz * distance * sky.clouds.z;
}

fn cloud_wind() -> vec2<f32> {
    return normalize(vec2(1.0, 0.31)) * sky.time_weather_phase.x * sky.clouds.w;
}

fn cumulus_density(p: vec2<f32>, coverage: f32, detail_weight: f32) -> f32 {
    let warp = vec2(
        cloud_fbm(p * 0.35 + vec2(5.2, 1.3), 3),
        cloud_fbm(p * 0.35 + vec2(9.7, 6.1), 3)
    ) - vec2(0.5);
    let q = p + warp * 1.2;
    let base = cloud_fbm(q, 5);
    let threshold = mix(0.70, 0.38, coverage);
    let shape = smoothstep(threshold, threshold + 0.12, base);
    let detail = cloud_fbm(q * 3.7 + vec2(3.1, 8.4), 3);
    // Eroding the thin parts with fine detail turns rounded blobs into
    // ragged edges while the thick core stays solid.
    return clamp(shape - (1.0 - shape) * detail * 0.6 * detail_weight, 0.0, 1.0);
}

struct CloudSample {
    opacity: f32,
    // 1 on the sun-facing rim of a thin part, falling toward 0 on the far
    // side and under the thick core.
    lit: f32,
}

fn sample_cumulus(direction: vec3<f32>, coverage: f32, rain: f32) -> CloudSample {
    let position = cloud_layer_position(direction, 1.0) + cloud_wind() + vec2(31.0, -17.0);
    // Fine detail only aliases where the layer compresses toward the horizon.
    let detail_weight = smoothstep(0.0, 0.25, direction.y);
    let density = cumulus_density(position, coverage, detail_weight);
    // A second sample a little toward the sun says which side of the cloud
    // this is: density falling toward the sun is the lit rim, rising is the
    // far side. The thick core is its own shadowed underside.
    let sun_dir = normalize(sky.sun_direction.xyz);
    let toward_sun = normalize(sun_dir.xz + vec2(0.0001, 0.0)) * 0.09 * sky.clouds.z;
    let sunward = cumulus_density(position + toward_sun, coverage, detail_weight);
    let rim = clamp(0.5 + (density - sunward) * 2.5, 0.0, 1.0);
    let core = smoothstep(0.2, 0.95, density);
    let lit = mix(0.35, 1.0, rim) * mix(1.0, 0.55, core);

    var opacity = density;
    // The layer closes into an unbroken deck as overcast arrives, reaching
    // full cover before the separate precipitation envelope starts.
    opacity = mix(opacity, 1.0, smoothstep(0.55, 0.95, rain));
    opacity *= smoothstep(0.0, 0.05, direction.y);
    return CloudSample(opacity, lit);
}

// A faint broken veil far above the cumulus, fair weather and high sky
// only. It is barely anisotropic: any long streak on the layer converges
// in perspective and reads as spokes.
fn sample_cirrus(direction: vec3<f32>, rain: f32) -> f32 {
    let p = cloud_layer_position(direction, 2.4) * 0.5 + cloud_wind() * 0.6 + vec2(-23.0, 41.0);
    let bend = cloud_fbm(p * 0.4 + vec2(7.7, 2.2), 3) - 0.5;
    let veil = cloud_fbm(vec2(p.x * 0.8 + bend * 0.8, p.y * 0.8 + bend * 1.2), 4);
    let cover = smoothstep(0.55, 0.85, veil) * smoothstep(0.25, 0.5, direction.y);
    return cover * 0.16 * (1.0 - rain);
}

// `sunset` is how close the sun is to the horizon: a low sun lights the
// clouds from the side, so their lit faces go golden and their undersides
// pick up the sunset colour across the whole sky.
fn cloud_shading(lit: f32, twilight: f32, daylight: f32, rain: f32, sun_height: f32, sunset: f32) -> vec3<f32> {
    let night = mix(sky.night_horizon.rgb, sky.night_zenith.rgb, 0.45) * sky.night_horizon.w * 1.75;
    let dusk = mix(sky.twilight_horizon.rgb, sky.twilight_zenith.rgb, 0.25) * sky.twilight_horizon.w * 1.15;
    let golden = mix(vec3(1.0), vec3(1.0, 0.78, 0.55), sunset * 0.7);
    let day_lit = sky.cloud_color.rgb * golden * sky.day_horizon.w * mix(0.7, 1.1, sun_height);
    let underside = sky.sunset.rgb * sky.day_horizon.w * 0.8;
    let day_shadow = mix(sky.cloud_shadow_color.rgb * sky.day_horizon.w, underside, sunset * 0.6);
    let fair = mix(day_shadow, day_lit, lit);
    let storm = sky.overcast_color.rgb * sky.day_horizon.w * mix(0.8, 1.2, lit);
    let dark = mix(night, dusk, twilight) * mix(0.7, 1.0, lit);
    return mix(dark, mix(fair, storm, rain), daylight);
}

// Reconstructing the ray from the fragment coordinate avoids interpolating
// directions across the sky mesh. Procedural detail therefore stays smooth
// across triangle boundaries and remains stable at large world positions.
fn camera_ray(position: vec2<f32>) -> vec3<f32> {
    let view_position = view.view_from_clip * vec4(
        coords_to_viewport_uv(position, view.viewport) * vec2(2.0, -2.0) + vec2(-1.0, 1.0),
        1.0,
        1.0,
    );
    let view_direction = view_position.xyz / view_position.w;
    return normalize((view.world_from_view * vec4(view_direction, 0.0)).xyz);
}

fn rotate_about_axis(v: vec3<f32>, axis: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return v * c + cross(axis, v) * s + axis * dot(axis, v) * (1.0 - c);
}

// A star's natural core is a fraction of a pixel at any playable resolution,
// so sampling it point-wise makes stars pop as the view moves. The drawn
// radius is held near one pixel and the light it carries scaled down to the
// natural core's, so a star is stable under motion and no brighter for being
// enlarged. Centres fall anywhere in their cell and every sample checks the
// eight cells nearest to it, so an enlarged star is drawn whole wherever it
// sits: confining centres to the cell's middle leaves star-free bands along
// the lattice planes, which cut the sky sphere in visible circles.
const STAR_CORE_RADIUS: f32 = 0.075;
const STAR_PIXEL_RADIUS: f32 = 0.9;
const STAR_MAX_RADIUS: f32 = 0.25;
// Only centres this close to the sphere are stars, and they are drawn where
// they project onto it, so the visible set never depends on the drawn radius.
const STAR_SHELL: f32 = 0.25;
// Cells per pixel beyond which a one-pixel star no longer fits its cell.
const STAR_LOD_FOOTPRINT: f32 = 0.3;

fn star_hash(cell: vec3<i32>, salt: u32) -> f32 {
    var value = bitcast<u32>(cell.x) * 374761393u
        + bitcast<u32>(cell.y) * 668265263u
        + bitcast<u32>(cell.z) * 2246822519u
        + salt * 3266489917u;
    value = (value ^ (value >> 13u)) * 1274126177u;
    return f32(value ^ (value >> 16u)) / 4294967295.0;
}

// `footprint` is the sky angle one pixel covers, in radians.
fn star_layer(direction: vec3<f32>, scale: f32, seed: u32, footprint: f32) -> vec3<f32> {
    let p = direction * scale;
    let base = floor(p);
    let radius = clamp(footprint * scale * STAR_PIXEL_RADIUS, STAR_CORE_RADIUS, STAR_MAX_RADIUS);
    let energy = (STAR_CORE_RADIUS * STAR_CORE_RADIUS) / (radius * radius);
    let side = select(vec3(-1.0), vec3(1.0), p - base >= vec3(0.5));
    var total = vec3(0.0);
    for (var i = 0; i < 8; i += 1) {
        let corner = base + vec3(f32(i & 1), f32((i >> 1) & 1), f32((i >> 2) & 1)) * side;
        let cell = vec3<i32>(corner);
        if star_hash(cell, seed) < 1.0 - sky.stars.y {
            continue;
        }
        let center = corner + vec3(star_hash(cell, seed + 1u), star_hash(cell, seed + 2u), star_hash(cell, seed + 3u));
        let depth = length(center) - scale;
        if abs(depth) > STAR_SHELL {
            continue;
        }
        let core = 1.0 - smoothstep(radius * 0.1, radius, length(p - center * (scale / length(center))));
        if core <= 0.0 {
            continue;
        }
        let value = star_hash(cell, seed + 4u);
        var luminance = mix(sky.stars.z, sky.stars.w, value * value);
        luminance *= mix(1.0, 2.8, step(1.0 - sky.star_detail.x, value));
        let twinkle = 1.0 + sky.star_detail.y * sin(sky.time_weather_phase.x * (0.7 + value * 1.8) + value * 31.0);
        let tint = mix(vec3(0.68, 0.79, 1.0), vec3(1.0, 0.88, 0.68), star_hash(cell, seed + 5u));
        total += tint * core * luminance * twinkle;
    }
    return total * energy;
}

// Coarser cells where a pixel spans too much sky for one-pixel stars, as in
// a small portal view target, cross-faded between power-of-two
// steps so no seam shows where the footprint crosses one.
fn star_lod(direction: vec3<f32>, scale: f32, seed: u32, footprint: f32) -> vec3<f32> {
    let lod = max(0.0, log2(footprint * scale / STAR_LOD_FOOTPRINT));
    let lower = floor(lod);
    let blend = lod - lower;
    let lower_scale = scale / exp2(lower);
    let stars = star_layer(direction, lower_scale, seed, footprint);
    if blend < 0.001 {
        return stars;
    }
    return mix(stars, star_layer(direction, lower_scale * 0.5, seed, footprint), blend);
}

fn star_field(ray: vec3<f32>, night: f32) -> vec3<f32> {
    let rotated = rotate_about_axis(ray, sky.pole_rotation.xyz, -sky.pole_rotation.w);
    let footprint = length(fwidth(ray));
    let seed = u32(sky.stars.x) * 8u;
    let field = star_lod(rotated, 185.0, seed, footprint) + star_lod(rotated, 240.0, seed + 16u, footprint) * 0.7;
    return field * night * smoothstep(-0.08, 0.12, ray.y);
}

fn moon_color(ray: vec3<f32>) -> vec3<f32> {
    let center = normalize(sky.moon_direction.xyz);
    let separation = acos(clamp(dot(ray, center), -1.0, 1.0));
    let halo = exp(-pow(separation / max(sky.moon_halo.x, 0.0001), 1.45)) * sky.moon_halo.y
        * sky.time_weather_phase.z;
    var result = vec3(0.72, 0.82, 1.0) * halo;
    if separation >= sky.moon.x {
        return result;
    }

    var right = cross(vec3(0.0, 1.0, 0.0), center);
    if length(right) < 0.01 {
        right = vec3(1.0, 0.0, 0.0);
    } else {
        right = normalize(right);
    }
    let up = normalize(cross(center, right));
    let radius = sin(sky.moon.x);
    let x = dot(ray, right) / radius;
    let y = dot(ray, up) / radius;
    let z = sqrt(max(0.0, 1.0 - x * x - y * y));
    let surface_normal = normalize(right * x + up * y - center * z);
    let direct = smoothstep(-0.018, 0.018, dot(surface_normal, normalize(sky.sun_direction.xyz)));
    let earthshine = sky.moon.z * (0.45 + 0.55 * max(0.0, -surface_normal.y));
    let surface_light = max(earthshine, direct);
    let limb = smoothstep(0.0, 0.12, z);
    result += vec3(0.86, 0.91, 1.0) * sky.moon.y * surface_light * limb;
    return result;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ray = camera_ray(in.position.xy);
    let elevation = clamp(ray.y, 0.0, 1.0);
    let gradient = pow(elevation, 0.42);
    let sun_dir = normalize(sky.sun_direction.xyz);
    let sun_altitude = asin(clamp(sun_dir.y, -1.0, 1.0));
    // The same altitude edges as `celestial_sky_system` uses for ambient,
    // fog, and grading: daylight from -4 to 10 degrees, twilight from -12
    // to -3, so every part of the scene turns over together.
    let daylight = smoothstep(-0.0698, 0.1745, sun_altitude);
    let twilight = smoothstep(-0.2094, -0.0524, sun_altitude);
    let night = 1.0 - twilight;

    let night_sky = mix(sky.night_horizon.rgb, sky.night_zenith.rgb, gradient) * sky.night_horizon.w;
    let twilight_sky = mix(sky.twilight_horizon.rgb, sky.twilight_zenith.rgb, gradient) * sky.twilight_horizon.w;
    let day_sky = mix(sky.day_horizon.rgb, sky.day_zenith.rgb, gradient) * sky.day_horizon.w;
    var color = mix(night_sky, twilight_sky, twilight);
    color = mix(color, day_sky, daylight);
    let sky_gradient = color;

    var toward_sun = 0.0;
    if length(ray.xz) > 0.0001 && length(sun_dir.xz) > 0.0001 {
        toward_sun = max(0.0, dot(normalize(ray.xz), normalize(sun_dir.xz)));
    }
    let sunset = 1.0 - smoothstep(0.02, 0.30, abs(sun_altitude));
    let sunset_band = exp(-abs(ray.y) * 8.0) * pow(toward_sun, 7.0) * sunset;
    color += sky.sunset.rgb * sunset_band;
    // The star field is the costliest part of the sky and invisible by day.
    if night > 0.001 {
        color += star_field(ray, night);
    }

    let sun_separation = acos(clamp(dot(ray, sun_dir), -1.0, 1.0));
    let sun_halo = exp(-pow(sun_separation / max(sky.sun.z, 0.0001), 1.35)) * sky.sun.w;
    color += vec3(1.0, 0.62, 0.30) * sun_halo;
    if sun_separation < sky.sun.x {
        color += vec3(1.0, 0.91, 0.68) * sky.sun.y;
    }
    color += moon_color(ray);

    let rain = clamp(sky.time_weather_phase.y, 0.0, 1.0);
    let coverage = mix(sky.clouds.x, sky.clouds.y, rain);
    let sun_height = smoothstep(-0.08, 0.43, sun_altitude);
    let cirrus = sample_cirrus(ray, rain);
    // A veil this thin shows the sky through it.
    let cirrus_rgb = mix(cloud_shading(0.85, twilight, daylight, rain, sun_height, sunset), sky_gradient, 0.3);
    color = mix(color, cirrus_rgb, cirrus);

    let cloud = sample_cumulus(ray, coverage, rain);
    var cloud_rgb = cloud_shading(cloud.lit, twilight, daylight, rain, sun_height, sunset);
    let sun_facing = pow(max(0.0, dot(ray, sun_dir)), 10.0);
    let silver_lining = sun_facing * (1.0 - cloud.opacity) * cloud.opacity * daylight * 2.2;
    cloud_rgb += vec3(1.0, 0.72, 0.44) * silver_lining;
    cloud_rgb += sky.sunset.rgb * sunset_band * cloud.opacity * 0.32;
    // Distant clouds sink into the same haze that pales the horizon sky;
    // under an overcast deck that haze is grey, not the blue behind it.
    let haze = 1.0 - smoothstep(0.0, 0.3, ray.y);
    let haze_rgb = mix(sky_gradient, sky.overcast_color.rgb * sky.day_horizon.w * 1.6, rain * daylight);
    cloud_rgb = mix(cloud_rgb, haze_rgb, haze * 0.7);
    color = mix(color, cloud_rgb, cloud.opacity);
    return vec4(max(color, vec3(0.0)), 1.0);
}
