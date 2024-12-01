use bevy::prelude::*;

#[derive(Debug)]
pub struct ChunkMetadata {
    pub position: IVec2,
    /// Level of Detail. This is only useful for rendering on the client.
    /// This is always 1 on the server. 1 means there is no loss in detail.
    pub lod: usize,

    // Width and height in world units.
    pub size: usize,
}

/// Describes a chunk parent entity. Everything local to the chunk (floor, trees,
/// but not entities that can walk across chunks) is a child of this.
#[derive(Component, Debug)]
pub struct Chunk<B: 'static + Send + Sync> {
    pub meta: ChunkMetadata,
    pub biome_data: B,
}

pub trait BiomeGenerator: Send + Sync + 'static {
    type BiomeData;

    fn generate_biome_data(
        &self,
        meta: &ChunkMetadata,
        asset_server: &AssetServer,
    ) -> Self::BiomeData;
}
