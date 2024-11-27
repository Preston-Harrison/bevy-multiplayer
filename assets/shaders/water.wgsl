#import bevy_pbr::mesh_types
#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::prepass_utils::prepass_depth

@fragment
fn fragment(
    @builtin(position) frag_coord: vec4<f32>,
    @builtin(sample_index) sample_index: u32,
) -> @location(0) vec4<f32> {
	let depth = prepass_depth(frag_coord, sample_index);
	var intersection = 1.0 - ((frag_coord.z - depth) * 100.0) - 0.82;
    intersection = smoothstep(0.0, 1.0, intersection);
	intersection = intersection + 0.1;
    return vec4(0, 0, intersection, 1.0);
}

