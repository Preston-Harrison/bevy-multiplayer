#import bevy_pbr::{
    forward_io::VertexOutput,
    pbr_types::pbr_input_new,
    pbr_functions as fns,
}

@group(2) @binding(0) var<uniform> grass: vec4<f32>;
@group(2) @binding(1) var<uniform> desert: vec4<f32>;

@group(2) @binding(2) var noise_texture: texture_2d<f32>;
@group(2) @binding(3) var noise_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
	// Perlin noise is stored in red channel, (RGBA -> XYZW, R = X).
	let strength = textureSample(noise_texture, noise_sampler, mesh.uv).x;
	let color = mix(grass, desert, 1.0 - strength);

	var pbr_input = pbr_input_new();
    pbr_input.material.base_color = color;
    return fns::apply_pbr_lighting(pbr_input);
}
