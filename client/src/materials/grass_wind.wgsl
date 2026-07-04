// Vertex-stage wind sway for grass blades. Fragment stages stay on the stock
// StandardMaterial PBR path, which multiplies vertex colors into base_color.
// UV0 carries (sway weight: 0 root / 1 tip, per-blade phase) — free because
// the material is untextured.

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

// xy = world wind direction (XZ), z = amplitude (m), w = speed (rad/s)
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> grass_wind: vec4<f32>;

fn wind_displacement(sway_weight: f32, phase01: f32) -> vec3<f32> {
    let t = globals.time * grass_wind.w;
    let phase = phase01 * 6.2831853;
    // primary gust + incommensurate ripple so blades don't move in lockstep
    let sway = sin(t + phase) + 0.4 * sin(t * 2.33 + phase * 1.71);
    // weight² keeps the lower blade stiff while the tip swings
    let bend = grass_wind.z * sway_weight * sway_weight * sway;
    return vec3<f32>(grass_wind.x * bend, 0.0, grass_wind.y * bend);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));

#ifdef PREPASS_PIPELINE
    // The depth-only prepass binds an empty material layout, so grass_wind
    // must stay unreferenced there; the deferred gbuffer pass binds the full
    // material layout.
#ifdef DEFERRED_PREPASS
#ifdef VERTEX_UVS_A
    world_position += vec4<f32>(wind_displacement(vertex.uv.x, vertex.uv.y), 0.0);
#endif
#endif
#else
#ifdef VERTEX_UVS_A
    world_position += vec4<f32>(wind_displacement(vertex.uv.x, vertex.uv.y), 0.0);
#endif
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
    // The sway contributes no motion vectors: previous = current.
    out.previous_world_position = world_position;
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

    return out;
}
