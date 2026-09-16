// A pennant's motion and cloth. The vertex stage turns the cloth, modelled
// along +X from its hoist, downwind and ripples it with a wave that grows
// toward the tip, leaning harder and rippling faster while a gust passes
// and drooping in still air; its normal follows the wave so the folds shade.
// The fragment stage weaves the cloth, darkens its hem and hoist band, and
// tilts the normal with the threads.

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    mesh_functions,
    prepass_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_prepass_functions::{prepass_alpha_discard, calculate_motion_vector},
    view_transformations::position_world_to_clip,
}
#import bevy_render::globals::Globals
// The prepass view layout binds Globals at @binding(1); the forward-layout
// `mesh_view_bindings::globals` (@binding(11)) does not exist here.
@group(0) @binding(1) var<uniform> globals: Globals;
#ifdef DEFERRED_PREPASS
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_deferred_functions::deferred_output,
}
#endif
#else
#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::globals,
}
#endif
#import bevy_pbr::pbr_types::StandardMaterial
#import cuboid_wars::wind::gust_strength

// The vertex stage's output, laid out like Bevy's own for both pipelines so the
// fragment stage reads it as the stock `VertexOutput`, with the clip position
// invariant across both vertex pipelines so the depth and colour passes agree.
struct FlagVertexOutput {
    @builtin(position) @invariant position: vec4<f32>,
#ifdef PREPASS_PIPELINE
#ifdef VERTEX_UVS_A
    @location(0) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(1) uv_b: vec2<f32>,
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    @location(2) world_normal: vec3<f32>,
#endif
    @location(4) world_position: vec4<f32>,
#ifdef MOTION_VECTOR_PREPASS
    @location(5) previous_world_position: vec4<f32>,
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    @location(6) unclipped_depth: f32,
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(7) instance_index: u32,
#endif
#ifdef VERTEX_COLORS
    @location(8) color: vec4<f32>,
#endif
#ifdef VISIBILITY_RANGE_DITHER
    @location(9) @interpolate(flat) visibility_range_dither: i32,
#endif
#else
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
#ifdef VERTEX_UVS_A
    @location(2) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(3) uv_b: vec2<f32>,
#endif
#ifdef VERTEX_COLORS
    @location(5) color: vec4<f32>,
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(6) @interpolate(flat) instance_index: u32,
#endif
#ifdef VISIBILITY_RANGE_DITHER
    @location(7) @interpolate(flat) visibility_range_dither: i32,
#endif
#endif
}

// xy = world wind direction (XZ), z = flutter amplitude at the tip (m), w = speed (rad/s)
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> flag_wind: vec4<f32>;
// x = the length (m) over which the flutter ramps up, y = hem as a fraction
// of the local height, z = hoist band as a fraction of the length, w = weave contrast
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> flag_cloth: vec4<f32>;

const TAU: f32 = 6.2831853;
const UP: vec3<f32> = vec3<f32>(0.0, 1.0, 0.0);
// Waves per metre of cloth, and how much of the amplitude a gust adds.
const WAVE_FREQUENCY: f32 = 7.0;
const GUST_FLUTTER: f32 = 1.5;
// How far the tip sags in still air, as a fraction of the cloth length.
const CALM_DROOP: f32 = 0.18;
// Sideways ripple relative to the vertical one.
const CROSS_RIPPLE: f32 = 0.6;
// The step (m) along the cloth that samples the wave's slope for the normal.
const NORMAL_STEP: f32 = 0.02;
// Thread spacing (m), how dark the hem and hoist band are, and how far the
// threads tilt the normal.
const THREAD_PITCH: f32 = 0.004;
const TRIM_TINT: f32 = 0.55;
const THREAD_RELIEF: f32 = 0.3;

// The cloth vertex in the mesh's frame, turned downwind and displaced.
fn cloth_position(local: vec3<f32>, base_xz: vec2<f32>, time: f32) -> vec3<f32> {
    let along = flag_wind.xy;
    let across = vec2(-along.y, along.x);
    let reach = clamp(local.x / flag_cloth.x, 0.0, 1.0);
    let gust = gust_strength(base_xz, along, time);
    let phase = fract(dot(base_xz, vec2(0.37, 0.71))) * TAU;
    let t = time * flag_wind.w + phase + gust * 0.4;
    let ripple = flag_wind.z * reach * mix(1.0, 1.0 + GUST_FLUTTER, gust);
    let lift = ripple * sin(local.x * WAVE_FREQUENCY - t);
    let side = ripple * CROSS_RIPPLE * sin(local.x * WAVE_FREQUENCY * 0.7 - t * 1.3);
    let droop = CALM_DROOP * local.x * reach * (1.0 - gust);
    let xz = along * local.x + across * side;
    return vec3<f32>(xz.x, local.y + lift - droop, xz.y);
}

// The cloth's normal at a vertex: the wave's slope along the cloth against the
// hoist direction, on the side the mesh winding faces.
fn cloth_normal(local: vec3<f32>, cloth: vec3<f32>, base_xz: vec2<f32>, time: f32) -> vec3<f32> {
    let ahead = cloth_position(local + vec3<f32>(NORMAL_STEP, 0.0, 0.0), base_xz, time);
    return normalize(cross(ahead - cloth, UP));
}

// x = albedo and emission tint, y = roughness change, zw = thread relief in
// the (across, up) basis. The weave fades with the pixel footprint so distant
// cloth stays calm.
fn cloth_shade(uv: vec2<f32>, metres: vec2<f32>) -> vec4<f32> {
    let phase = metres / THREAD_PITCH * TAU;
    let footprint = fwidth(metres.x) / THREAD_PITCH;
    let visibility = 1.0 - smoothstep(0.3, 0.8, footprint);
    let weave = cos(phase.x) * cos(phase.y) * visibility;
    let hem = 1.0 - smoothstep(0.0, flag_cloth.y, min(uv.y, 1.0 - uv.y));
    let hoist = 1.0 - smoothstep(flag_cloth.z, flag_cloth.z * 1.3, uv.x);
    let symbol = (metres - vec2<f32>(flag_cloth.x * 0.43, -0.35)) / 0.20;
    let ring = abs(length(symbol) - 0.74) < 0.16 && !(symbol.x > 0.25 && symbol.y > 0.25);
    let arrow = symbol.x > 0.35 && symbol.x < 1.0 && abs(symbol.y - 0.42) < (symbol.x - 0.35) * 0.8;
    let emblem = select(1.0, 0.10, ring || arrow);
    let tint = emblem * (1.0 + flag_cloth.w * weave) * mix(1.0, TRIM_TINT, max(hem, hoist));
    let relief = vec2<f32>(-sin(phase.x) * cos(phase.y), -cos(phase.x) * sin(phase.y)) * visibility;
    return vec4<f32>(tint, -0.15 * weave, relief);
}

fn cloth_material(material: StandardMaterial, shade: vec4<f32>) -> StandardMaterial {
    var out = material;
    out.base_color = vec4<f32>(material.base_color.rgb * shade.x, material.base_color.a);
    out.emissive = vec4<f32>(material.emissive.rgb * shade.x, material.emissive.a);
    out.perceptual_roughness = clamp(material.perceptual_roughness + shade.y, 0.05, 1.0);
    return out;
}

fn thread_normal(n: vec3<f32>, relief: vec2<f32>) -> vec3<f32> {
    let across = normalize(cross(UP, n));
    return normalize(n + (across * relief.x + UP * relief.y) * THREAD_RELIEF);
}

@vertex
fn vertex(vertex: Vertex) -> FlagVertexOutput {
    var out: FlagVertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let base_xz = world_from_local[3].xz;

    let cloth = cloth_position(vertex.position, base_xz, globals.time);
    let normal = cloth_normal(vertex.position, cloth, base_xz, globals.time);
    var world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(cloth, 1.0));

    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(normal, vertex.instance_index);
#endif
#ifdef MOTION_VECTOR_PREPASS
    let previous_world_from_local = mesh_functions::get_previous_world_from_local(vertex.instance_index);
    let previous_cloth = cloth_position(vertex.position, base_xz, globals.time - globals.delta_time);
    out.previous_world_position =
        mesh_functions::mesh_position_local_to_world(previous_world_from_local, vec4<f32>(previous_cloth, 1.0));
#endif
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(normal, vertex.instance_index);
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
#endif

    return out;
}

#ifdef PREPASS_PIPELINE
#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var out: FragmentOutput;
#ifdef DEFERRED_PREPASS
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let shade = cloth_shade(in.uv, in.uv_b);
    pbr_input.material = cloth_material(pbr_input.material, shade);
    pbr_input.N = thread_normal(pbr_input.N, shade.zw);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    out = deferred_output(in, pbr_input);
#else
    prepass_alpha_discard(in);
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    prepass_alpha_discard(in);
}
#endif
#else
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let shade = cloth_shade(in.uv, in.uv_b);
    pbr_input.material = cloth_material(pbr_input.material, shade);
    pbr_input.N = thread_normal(pbr_input.N, shade.zw);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
#endif
