use bevy::{
    pbr::MaterialExtension,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GrassDesert {
    #[uniform(0)]
    pub grass: LinearRgba,
    #[uniform(1)]
    pub desert: LinearRgba,

    #[texture(2)]
    #[sampler(3)]
    pub noise_texture: Handle<Image>,
}

impl Material for GrassDesert {
    fn fragment_shader() -> ShaderRef {
        "shaders/grass_desert.wgsl".into()
    }
}
