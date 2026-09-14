#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

// x grass tile size, y grass relief, z soil relief, w soil tile size (metres)
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> surface: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var grass_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var soil_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var soil_sampler: sampler;
// Linear colour the meadow texture's mean is remapped to; the mean of the
// rendered grass, before lighting, so it can be matched by the blade meshes.
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<uniform> grass_color: vec4<f32>;

// Measured mean linear colours of the two albedo textures. Tinting divides
// them out, so the texture keeps every texel's relative hue and value while
// the configured colour decides what the meadow averages to.
const MEADOW_ALBEDO_MEAN: vec3<f32> = vec3(0.1273, 0.1777, 0.0403);
const SOIL_ALBEDO_MEAN: vec3<f32> = vec3(0.1041, 0.0647, 0.0416);
const LUMINANCE: vec3<f32> = vec3(0.2126, 0.7152, 0.0722);
// Dead grass is yellower and browner than live grass, not just lighter.
const DRY_GRASS_TINT: vec3<f32> = vec3(1.22, 1.0, 0.55);

fn relief_normal(position: vec3<f32>, normal: vec3<f32>, height: f32) -> vec3<f32> {
    let dx = dpdx(position);
    let dy = dpdy(position);
    let x = cross(dy, normal);
    let y = cross(normal, dx);
    let determinant = dot(dx, x);
    let gradient = sign(determinant) * (dpdx(height) * x + dpdy(height) * y);
    return normalize(max(abs(determinant), 0.00000001) * normal - gradient);
}

fn terrain_hash(cell: vec2<i32>) -> f32 {
    var value = bitcast<u32>(cell.x) * 374761393u + bitcast<u32>(cell.y) * 668265263u;
    value = (value ^ (value >> 13u)) * 1274126177u;
    return f32(value ^ (value >> 16u)) / 4294967295.0;
}

fn terrain_noise(position: vec2<f32>) -> f32 {
    let cell = vec2<i32>(floor(position));
    var t = fract(position);
    t = t * t * (vec2(3.0) - t * 2.0);
    let a = mix(terrain_hash(cell), terrain_hash(cell + vec2(1, 0)), t.x);
    let b = mix(terrain_hash(cell + vec2(0, 1)), terrain_hash(cell + vec2(1, 1)), t.x);
    return mix(a, b, t.y);
}

// Mirrors `TerrainCover::at` in `map/terrain_surface.rs`: x bare soil, y dry
// grass, z shade, w macro tint. The CPU side places blades from the same field.
fn terrain_cover(position: vec2<f32>) -> vec4<f32> {
    let warp = vec2(
        terrain_noise(position / 9.0 + vec2(3.0, 71.0)),
        terrain_noise(position / 9.0 + vec2(57.0, 13.0))
    ) * 6.0 - vec2(3.0);
    let warped = position + warp;
    let patch_noise = terrain_noise(warped / 13.0) * 0.55
        + terrain_noise(warped / 5.1 + vec2(31.0)) * 0.30
        + terrain_noise(warped / 2.1 + vec2(13.0, 47.0)) * 0.15;
    let soil = smoothstep(0.68, 0.80, patch_noise);
    let dry = smoothstep(0.28, 0.82, terrain_noise(position / 37.0 + vec2(71.0, 19.0)));
    let shade_noise = terrain_noise(position / 4.8 + vec2(5.0, 29.0)) * 0.65
        + terrain_noise(position / 2.3 + vec2(149.0, 11.0)) * 0.35;
    let region = terrain_noise(position / 19.0 + vec2(61.0, 173.0));
    let shade = (0.9 + smoothstep(0.28, 0.72, shade_noise) * 0.2) * (0.95 + region * 0.1);
    let macro_tint = terrain_noise(position / 4.6 + vec2(211.0, 43.0)) * 0.70
        + terrain_noise(position / 2.2 + vec2(17.0, 191.0)) * 0.30;
    return vec4(soil, dry, shade, macro_tint);
}

// Stochastic triangular tiling: every point blends three texture samples,
// each from a region with its own stable offset and quarter-turn, so the
// source image's repeat never lines up. Blending uncorrelated samples
// averages their contrast away; `restore_contrast` puts it back.
struct StochasticFrame {
    position: vec2<f32>,
    gradient_x: vec2<f32>,
    gradient_y: vec2<f32>,
    weights: vec3<f32>,
    id_0: vec2<i32>,
    id_1: vec2<i32>,
    id_2: vec2<i32>,
}

fn stochastic_frame(position: vec2<f32>) -> StochasticFrame {
    let skewed = vec2(position.x - position.y * 0.577350269, position.y * 1.154700538);
    let base = vec2<i32>(floor(skewed));
    let local = fract(skewed);
    let remainder = 1.0 - local.x - local.y;
    var frame: StochasticFrame;
    if remainder > 0.0 {
        frame.weights = vec3(remainder, local.y, local.x);
        frame.id_0 = base;
        frame.id_1 = base + vec2(0, 1);
        frame.id_2 = base + vec2(1, 0);
    } else {
        frame.weights = vec3(-remainder, 1.0 - local.y, 1.0 - local.x);
        frame.id_0 = base + vec2(1, 1);
        frame.id_1 = base + vec2(1, 0);
        frame.id_2 = base + vec2(0, 1);
    }
    frame.position = position;
    frame.gradient_x = dpdx(position);
    frame.gradient_y = dpdy(position);
    return frame;
}

fn variant_rotation(id: vec2<i32>, salt: vec2<i32>) -> vec2<f32> {
    let angle = floor(terrain_hash(id + salt) * 4.0) * 1.570796327;
    return vec2(cos(angle), sin(angle));
}

fn apply_rotation(value: vec2<f32>, axis: vec2<f32>) -> vec2<f32> {
    return vec2(axis.x * value.x - axis.y * value.y, axis.y * value.x + axis.x * value.y);
}

fn variant_offset(id: vec2<i32>, salt: vec2<i32>) -> vec2<f32> {
    return vec2(
        terrain_hash(id + salt + vec2(37, 101)),
        terrain_hash(id + salt + vec2(173, 59))
    );
}

// Scales the blend's deviation from the mean so its variance matches one
// sample's. Exact for uncorrelated samples with a shared mean, which the
// three regions of a seamless texture are.
fn restore_contrast(blend: vec3<f32>, mean: vec3<f32>, weights: vec3<f32>) -> vec3<f32> {
    let gain = inverseSqrt(max(dot(weights, weights), 0.0001));
    return max(mean + (blend - mean) * gain, vec3(0.0));
}

fn grass_variant(frame: StochasticFrame, id: vec2<i32>) -> vec3<f32> {
    let salt = vec2(11, 79);
    let axis = variant_rotation(id, salt);
    return textureSampleGrad(
        grass_texture,
        grass_sampler,
        apply_rotation(frame.position, axis) + variant_offset(id, salt),
        apply_rotation(frame.gradient_x, axis),
        apply_rotation(frame.gradient_y, axis)
    ).rgb;
}

fn stochastic_grass(position: vec2<f32>) -> vec3<f32> {
    let frame = stochastic_frame(position);
    let blend = grass_variant(frame, frame.id_0) * frame.weights.x
        + grass_variant(frame, frame.id_1) * frame.weights.y
        + grass_variant(frame, frame.id_2) * frame.weights.z;
    return restore_contrast(blend, MEADOW_ALBEDO_MEAN, frame.weights);
}

fn soil_variant(frame: StochasticFrame, id: vec2<i32>) -> vec3<f32> {
    let salt = vec2(193, 41);
    let axis = variant_rotation(id, salt);
    return textureSampleGrad(
        soil_texture,
        soil_sampler,
        apply_rotation(frame.position, axis) + variant_offset(id, salt),
        apply_rotation(frame.gradient_x, axis),
        apply_rotation(frame.gradient_y, axis)
    ).rgb;
}

fn stochastic_soil(position: vec2<f32>) -> vec3<f32> {
    let frame = stochastic_frame(position);
    let blend = soil_variant(frame, frame.id_0) * frame.weights.x
        + soil_variant(frame, frame.id_1) * frame.weights.y
        + soil_variant(frame, frame.id_2) * frame.weights.z;
    return restore_contrast(blend, SOIL_ALBEDO_MEAN, frame.weights);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    // UVs are surface-local metres, so a carried terrain surface keeps its texture in place.
    let uv = in.uv;
    let cover = terrain_cover(uv);

    // A slow coordinate warp bends the tiling regions so their straight
    // triangle edges never show as a lattice.
    let warp = vec2(
        terrain_noise(uv / 11.3 + vec2(17.0, 3.0)),
        terrain_noise(uv / 13.7 + vec2(5.0, 23.0))
    ) * 2.8;
    let meadow = stochastic_grass((uv + warp) / surface.x);
    let meadow_luminance = dot(meadow, LUMINANCE);
    let live_grass = meadow * (grass_color.rgb / MEADOW_ALBEDO_MEAN);
    let grass = mix(live_grass, live_grass * DRY_GRASS_TINT, cover.y * 0.35)
        * mix(0.92, 1.08, cover.w);

    // The meadow's own light and dark texels break up the patch boundary,
    // so grass thins into the soil instead of stopping at a contour. The
    // blades stop where the cover field alone says the soil is bare.
    let edge = cover.x + (meadow_luminance - dot(MEADOW_ALBEDO_MEAN, LUMINANCE)) * 0.6;
    var bare = smoothstep(0.05, 0.35, edge);
    // Far away the patches would read as flat blotches, so their contrast
    // fades with the pixel footprint: fewer metres per pixel means nearer.
    let footprint = max(length(dpdx(uv)), length(dpdy(uv)));
    bare *= mix(1.0, 0.4, smoothstep(0.08, 0.35, footprint));

    // Most of the meadow shows no soil at all, and its three taps are the
    // dearest part of the shader; explicit gradients make skipping them safe.
    var soil_color = vec3(0.0);
    var soil_height = 0.0;
    if bare > 0.005 {
        let soil_warp = vec2(
            terrain_noise(uv / 7.7 + vec2(89.0, 17.0)),
            terrain_noise(uv / 9.1 + vec2(23.0, 157.0))
        ) * 0.24 - vec2(0.12);
        let soil = stochastic_soil((uv + soil_warp) / surface.w);
        let soil_macro = terrain_noise(uv / 4.9 + vec2(181.0, 61.0)) * 0.65
            + terrain_noise(uv / 2.4 + vec2(73.0, 227.0)) * 0.35;
        soil_color = soil * mix(vec3(0.88, 0.9, 0.94), vec3(1.1, 1.05, 0.97), soil_macro);
        soil_height = dot(soil, LUMINANCE) * surface.z;
    }

    let color = mix(grass, soil_color, bare) * cover.z;
    pbr.material.base_color = vec4(color, 1.0);
    let height = mix(meadow_luminance * surface.y, soil_height, bare);
    pbr.N = relief_normal(in.world_position.xyz, normalize(in.world_normal), height);
    apply_decals(&pbr);
#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
#endif
}
