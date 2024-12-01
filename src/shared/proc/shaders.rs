use bevy::{
    color::palettes::css::GREEN,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

use super::grassy_desert::GrassyDesertBiomeData;

/// A grassy desert biome.
/// Grass is drawn when the noise texture is sampled above grass_gte. Same
/// concept for desert_lte, but is drawn when less than. The base color of a
/// default pbr texture is used for grass and desert.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GrassDesert {
    #[uniform(0)]
    pub grass: LinearRgba,
    #[uniform(1)]
    pub desert: LinearRgba,

    #[uniform(2)]
    pub grass_gte: f32,
    #[uniform(3)]
    pub desert_lte: f32,

    #[texture(4)]
    #[sampler(5)]
    pub noise_texture: Handle<Image>,
}

impl GrassDesert {
    pub fn from_biome(biome: &GrassyDesertBiomeData) -> Self {
        Self {
            grass: GREEN.into(),
            desert: Color::srgba_u8(237, 201, 175, 255).into(),
            noise_texture: biome.noise_map.clone(),
            grass_gte: biome.grass_gte as f32 / 255.0,
            desert_lte: biome.desert_lte as f32 / 255.0,
        }
    }
}

impl Material for GrassDesert {
    fn fragment_shader() -> ShaderRef {
        "shaders/grass_desert.wgsl".into()
    }
}
