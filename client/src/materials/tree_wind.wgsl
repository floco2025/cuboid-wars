// Vertex-stage sway for trees: the crown bends with the wind and its gusts
// by height above the roots. Fragment stages stay on the stock
// StandardMaterial path (alpha-masked leaf texture, vertex colours).

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    mesh_functions,
    prepass_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}
#import bevy_render::globals::Globals
// The prepass view layout binds Globals at @binding(1); the forward-layout
// `mesh_view_bindings::globals` (@binding(11)) does not exist here.
@group(0) @binding(1) var<uniform> globals: Globals;
#else
#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::globals,
}
#endif
#import cuboid_wars::wind::gust_strength

// xy = world wind direction (XZ), z = crown amplitude (m), w = speed (rad/s)
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> tree_wind: vec4<f32>;

// Sway grows with height above the roots up to this many metres of the
// unscaled model.
const SWAY_HEIGHT: f32 = 10.0;
// Extra downwind lean, in crown amplitudes, while a gust passes.
const GUST_LEAN: f32 = 1.2;
// The branches' own swing on top of the trunk's: its size and rate relative
// to the trunk's, and how far its phase trails per metre out from the trunk
// axis, so a gust ripples through the crown instead of moving it as one slab.
const BRANCH_SWAY: f32 = 0.35;
const BRANCH_RATE: f32 = 1.8;
const BRANCH_LAG: f32 = 0.4;
// A little across-wind swing turns the pendulum into a loose ellipse.
const CROSS_SWAY: f32 = 0.25;
const CROSS_RATE: f32 = 0.83;
const TAU: f32 = 6.2831853;

// Every vertex at one height moves together (up to the slow crown ripple),
// so the tree leans and returns as a whole; anything varying per vertex
// stretches the leaf quads and reads as the wood deforming.
fn sway_displacement(local: vec3<f32>, base_xz: vec2<f32>, time: f32) -> vec3<f32> {
    let weight = clamp(local.y / SWAY_HEIGHT, 0.0, 1.0);
    // Each tree swings out of step with its neighbours.
    let phase = fract(dot(base_xz, vec2(0.37, 0.71))) * TAU;
    let t = time * tree_wind.w;
    let gust = gust_strength(base_xz, tree_wind.xy, time);
    let drive = mix(0.5, 1.0, gust);
    let trunk = weight * weight * (sin(t + phase) * drive + GUST_LEAN * gust);
    let reach = length(local.xz);
    let branches = weight * BRANCH_SWAY * drive * sin(t * BRANCH_RATE + phase - reach * BRANCH_LAG);
    let cross = weight * weight * CROSS_SWAY * sin(t * CROSS_RATE + phase * 1.7);
    let across = vec2(-tree_wind.y, tree_wind.x);
    let offset = tree_wind.z * (tree_wind.xy * (trunk + branches) + across * cross);
    return vec3<f32>(offset.x, 0.0, offset.y);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let base_xz = world_from_local[3].xz;

    // The wind uniform is bound in the main pass, in the deferred prepass,
    // and in the prepass of an alpha-masked material (the leaves); the
    // depth-only prepass of an opaque material binds no material at all.
#ifdef PREPASS_PIPELINE
#ifdef DEFERRED_PREPASS
    world_position += vec4<f32>(sway_displacement(vertex.position, base_xz, globals.time), 0.0);
#else
#ifdef MAY_DISCARD
    world_position += vec4<f32>(sway_displacement(vertex.position, base_xz, globals.time), 0.0);
#endif
#endif
#else
    world_position += vec4<f32>(sway_displacement(vertex.position, base_xz, globals.time), 0.0);
#endif

    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif

#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
#endif
#ifdef MOTION_VECTOR_PREPASS
    // Last frame's sway as well as last frame's transform, so temporal
    // anti-aliasing sees the crown's real motion instead of smearing it.
    let previous_world_from_local = mesh_functions::get_previous_world_from_local(vertex.instance_index);
    var previous_world_position =
        mesh_functions::mesh_position_local_to_world(previous_world_from_local, vec4<f32>(vertex.position, 1.0));
#ifdef DEFERRED_PREPASS
    previous_world_position +=
        vec4<f32>(sway_displacement(vertex.position, base_xz, globals.time - globals.delta_time), 0.0);
#else
#ifdef MAY_DISCARD
    previous_world_position +=
        vec4<f32>(sway_displacement(vertex.position, base_xz, globals.time - globals.delta_time), 0.0);
#endif
#endif
    out.previous_world_position = previous_world_position;
#endif
#else
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
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
