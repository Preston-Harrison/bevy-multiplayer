use bevy::{prelude::*, utils::HashSet};
use chunk::{BiomeGenerator, Chunk, ChunkMetadata};
use grassy_desert::{gen_grassy_desert_trees, GrassyDesertBiomeData, GrassyDesertTerrain};
use shaders::GrassDesert;
use utils::{generate_chunks_around, ProcUtilsPlugin};

use self::tree::TreePlugin;

pub mod chunk;
pub mod grassy_desert;
pub mod shaders;
pub mod tree;
pub mod utils;

fn res_exists<T: Resource>(resource: Option<Res<T>>) -> bool {
    resource.is_some()
}

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (load_grassy_desert_chunks, gen_grassy_desert_trees)
                .run_if(res_exists::<GrassyDesertTerrain>),
        );
        app.add_plugins((TreePlugin, ProcUtilsPlugin));
        app.add_plugins(MaterialPlugin::<GrassDesert>::default());
    }
}

const SERVER_LOD: usize = 1;

#[derive(Component)]
pub struct LoadsChunks;

/// Loads and unloads chunks based on if an entity with `LoadsChunks` exists
/// there. Does this for all chunks nearby.
///
/// On clients, LoadsChunks should only be present on the client's player.
/// On the server, it should be on all players.
pub fn load_grassy_desert_chunks(
    loaders: Query<&Transform, With<LoadsChunks>>,
    chunks: Query<(Entity, &Chunk<GrassyDesertBiomeData>)>,
    mut commands: Commands,
    terrain: Res<GrassyDesertTerrain>,
    asset_server: Res<AssetServer>,
) {
    let mut chunks_with_loaders: HashSet<IVec2> = HashSet::new();
    for transform in loaders.iter() {
        let chunk = terrain.world_position_to_chunk(transform.translation);
        let loaded_chunks = generate_chunks_around(chunk, terrain.radius)
            .into_iter()
            .map(|(pos, _)| pos);
        chunks_with_loaders.extend(loaded_chunks);
    }

    let mut loaded_chunks: HashSet<IVec2> = HashSet::new();
    for (entity, chunk) in chunks.iter() {
        if chunks_with_loaders.contains(&chunk.meta.position) {
            loaded_chunks.insert(chunk.meta.position);
        } else {
            terrain.unload_chunk(entity, &mut commands);
        }
    }

    let chunks_to_load = chunks_with_loaders.difference(&loaded_chunks);
    for chunk_pos in chunks_to_load {
        let metadata = ChunkMetadata {
            position: *chunk_pos,
            lod: SERVER_LOD,
            size: terrain.chunk_size,
        };
        let chunk = Chunk {
            biome_data: terrain
                .biome_generator
                .generate_biome_data(&metadata, &asset_server),
            meta: metadata,
        };
        terrain.render_chunk(chunk, &mut commands, &asset_server);
    }
}
