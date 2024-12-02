#import bevy_pbr::{
    mesh_view_bindings::globals,
    forward_io::VertexOutput,
}

@group(2) @binding(0) var<uniform> tracer_start: vec4<f32>;
@group(2) @binding(1) var<uniform> tracer_end: vec4<f32>;

@group(2) @binding(2) var<uniform> time_spawned: f32;
@group(2) @binding(3) var<uniform> time_alive: f32;
@group(2) @binding(4) var<uniform> tracer_length: f32;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // Compute the elapsed time and lifetime fraction
    let elapsed_time = globals.time - time_spawned;
    let lifetime_fraction = clamp(elapsed_time / time_alive, 0.0, 1.0);

    // Calculate the moving visible segment
	// The visible segment starts at the lifetime fraction
    let start = lifetime_fraction - tracer_length;
	// Moves to the end over time
    let end = lifetime_fraction + tracer_length;

    // Check if the current fragment is within the visible range
    if (mesh.uv.y < start || mesh.uv.y > end) {
        discard;
    }

    // Normalize t within the visible range for color interpolation
    let t = (mesh.uv.y - start) / (end - start);
    let color = mix(tracer_start, tracer_end, clamp(t, 0.0, 1.0));

    return color;
}

