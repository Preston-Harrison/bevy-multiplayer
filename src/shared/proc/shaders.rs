use bevy::{
    color::palettes::css::GREEN,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

use super::biome::BiomeBlend;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GrassDesert {
    #[uniform(0)]
    pub grass: LinearRgba,
    #[uniform(1)]
    pub desert: LinearRgba,

    /// Grass is generated when sample is above this.
    #[uniform(2)]
    pub grass_gte: f32,
    /// Desert is generated when sample is below this.
    #[uniform(3)]
    pub desert_lte: f32,

    #[texture(4)]
    #[sampler(5)]
    pub noise_texture: Handle<Image>,
}

impl GrassDesert {
    pub fn from_biome(noise_texture: Handle<Image>, biome_blend: &BiomeBlend) -> Self {
        Self {
            grass: GREEN.into(),
            desert: Color::srgba_u8(237, 201, 175, 255).into(),
            noise_texture,
            grass_gte: biome_blend.grass_gte as f32 / 255.0,
            desert_lte: biome_blend.desert_lte as f32 / 255.0,
        }
    }
}

impl Material for GrassDesert {
    fn fragment_shader() -> ShaderRef {
        "shaders/grass_desert.wgsl".into()
    }
}
