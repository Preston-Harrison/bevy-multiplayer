#import bevy_pbr::forward_io::VertexOutput


@group(2) @binding(0) var<uniform> tracer_start: vec4<f32>;
@group(2) @binding(1) var<uniform> tracer_end: vec4<f32>;

// @group(2) @binding(2) var<uniform> init_time: vec4<f32>;
// @group(2) @binding(3) var<uniform> tracer_end: vec4<f32>;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // Linear interpolation based on the y-coordinate in UV space
    let t = mesh.uv.y;
    return mix(tracer_start, tracer_end, t);
}
