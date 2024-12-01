use std::f32::consts::{FRAC_2_PI, PI};

use bevy::{
    color::palettes::css::BLUE,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
    },
};
use bevy_rapier3d::prelude::*;
use noise::{NoiseFn, Perlin, Simplex};
use rand::Rng;

use crate::utils;

use super::{
    chunk::{BiomeGenerator, Chunk, ChunkMetadata},
    shaders::GrassDesert,
    tree::Tree,
    utils::{NoiseLayer, SnapToFloor},
};

/// Represents a terrain chunk.
#[derive(Resource)]
pub struct GrassyDesertTerrain {
    pub chunk_size: usize,
    /// The radius around the player(s) to generate chunks.
    pub radius: i32,
    pub grid_spacing: usize,
    pub noise_layers: Vec<NoiseLayer<Perlin>>,
    pub biome_generator: GrassyDesertBiomeGenerator,
}

impl GrassyDesertTerrain {
    pub fn new() -> Self {
        let noise_layers = vec![
            NoiseLayer {
                noise: Perlin::new(0),
                amplitude: 15.0,
                frequency: 0.005,
            },
            NoiseLayer {
                noise: Perlin::new(1),
                amplitude: 5.0,
                frequency: 0.01,
            },
            NoiseLayer {
                noise: Perlin::new(2),
                amplitude: 0.5,
                frequency: 0.02,
            },
        ];
        Self {
            chunk_size: 100,
            radius: 2,
            grid_spacing: 5,
            noise_layers,
            biome_generator: GrassyDesertBiomeGenerator,
        }
    }

    pub fn draw_chunk_gizmo(&self, gizmos: &mut Gizmos, chunk_pos: IVec2) {
        gizmos.rect(
            self.chunk_to_world_position(chunk_pos, Vec3::ZERO) + self.mid_chunk_offset(),
            Quat::from_rotation_x(PI / 2.0),
            Vec2::splat(100.0),
            BLUE,
        );
    }

    pub fn world_position_to_chunk(&self, position: Vec3) -> IVec2 {
        IVec2::new(
            (position.x / self.chunk_size as f32).floor() as i32,
            (position.z / self.chunk_size as f32).floor() as i32,
        )
    }

    pub fn chunk_to_world_position(&self, chunk: IVec2, offset: Vec3) -> Vec3 {
        offset
            + Vec3::new(
                chunk.x as f32 * self.chunk_size as f32,
                0.0,
                chunk.y as f32 * self.chunk_size as f32,
            )
    }

    fn grid_point_to_world_position(&self, chunk: IVec2, offset: IVec2, lod: usize) -> (f64, f64) {
        let world_x = (chunk.x * self.chunk_size as i32
            + offset.x as i32 * lod as i32 * self.grid_spacing as i32) as f64;
        let world_z = (chunk.y * self.chunk_size as i32
            + offset.y as i32 * lod as i32 * self.grid_spacing as i32) as f64;
        (world_x, world_z)
    }

    /// Returns an offset to move from an output of `chunk_to_world_position` to
    /// the center of the chunk. Y is set to zero.
    pub fn mid_chunk_offset(&self) -> Vec3 {
        Vec3::new(self.chunk_size as f32, 0.0, self.chunk_size as f32) / 2.0
    }

    fn get_num_grid_points(&self, lod: usize) -> usize {
        (self.chunk_size / (lod * self.grid_spacing)) + 1
    }

    /// Generates a terrain mesh for this chunk using layered noise maps.
    fn generate_mesh(&self, chunk_pos: IVec2, lod: usize) -> Mesh {
        let grid_points = self.get_num_grid_points(lod);
        let mut vertices = Vec::with_capacity(grid_points * grid_points);
        let mut uvs = Vec::with_capacity(grid_points * grid_points);
        let mut indices = Vec::new();

        // Generate vertices and heights
        for z in 0..grid_points {
            for x in 0..grid_points {
                // Calculate world positions
                let (world_x, world_z) = self.grid_point_to_world_position(
                    chunk_pos,
                    IVec2::new(x as i32, z as i32),
                    lod,
                );

                // Compute height using layered noise
                let mut height = 0.0f32;
                for NoiseLayer {
                    noise,
                    amplitude,
                    frequency,
                } in self.noise_layers.iter()
                {
                    let sample_x = world_x * *frequency;
                    let sample_z = world_z * *frequency;
                    let noise_value = noise.get([sample_x, sample_z]) as f32;
                    height += noise_value * *amplitude as f32;
                }

                let x_pos = x as f32 * lod as f32 * self.grid_spacing as f32;
                let z_pos = z as f32 * lod as f32 * self.grid_spacing as f32;
                vertices.push([x_pos, height, z_pos]);

                // Compute UV coordinates
                let u = x_pos / (self.chunk_size as f32);
                let v = z_pos / (self.chunk_size as f32);
                uvs.push([u, v]);
            }
        }

        // Generate indices and normals
        for z in 0..(grid_points - 1) {
            for x in 0..(grid_points - 1) {
                let top_left = z * grid_points + x;
                let bottom_left = (z + 1) * grid_points + x;
                let top_right = top_left + 1;
                let bottom_right = bottom_left + 1;

                indices.extend_from_slice(&[
                    top_left as u32,
                    bottom_left as u32,
                    bottom_right as u32,
                    top_left as u32,
                    bottom_right as u32,
                    top_right as u32,
                ]);
            }
        }

        // Create the mesh
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh.compute_normals();
        mesh.generate_tangents().expect("tangents to be generated");

        mesh
    }

    /// Renders the chunk into the Bevy world.
    pub fn render_chunk(
        &self,
        chunk: Chunk<GrassyDesertBiomeData>,
        commands: &mut Commands,
        asset_server: &AssetServer,
    ) {
        let mesh = self.generate_mesh(chunk.meta.position, chunk.meta.lod);
        let collider = Collider::from_bevy_mesh(&mesh, &ComputedColliderShape::TriMesh)
            .expect("collider to be constructed");
        let mesh_handle = asset_server.add(mesh);
        let grass_desert = asset_server.add(GrassDesert::from_biome(&chunk.biome_data));

        let world_pos = self.chunk_to_world_position(chunk.meta.position, Vec3::ZERO);
        let spatial = SpatialBundle::from_transform(Transform::from_translation(world_pos));

        commands.spawn((chunk, spatial)).with_children(|parent| {
            parent.spawn((
                MaterialMeshBundle {
                    mesh: mesh_handle,
                    material: grass_desert,
                    ..default()
                },
                collider,
                RigidBody::Fixed,
            ));
        });
    }

    pub fn unload_chunk(&self, chunk_entity: Entity, commands: &mut Commands) {
        commands.entity(chunk_entity).despawn_recursive();
    }
}

pub struct GrassyDesertBiomeGenerator;

impl GrassyDesertBiomeGenerator {
    fn get_biome_noise(&self, meta: &ChunkMetadata) -> Image {
        let noise_fn = Simplex::new(3);
        let mut data = vec![0; 10_000];

        for x in 0..(meta.size as usize) {
            for z in 0..100 {
                let sample_x = (meta.position.x as f64 * meta.size as f64 + x as f64) * 0.00659;
                let sample_z = (meta.position.y as f64 * meta.size as f64 + z as f64) * 0.00659;
                data[x + 100 * z] =
                    (noise_fn.get([sample_x, sample_z]) * 255.0).clamp(0.0, 255.0) as u8;
            }
        }

        Image::new(
            Extent3d {
                width: 100,
                height: 100,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::R8Unorm,
            RenderAssetUsages::default(),
        )
    }
}

impl BiomeGenerator for GrassyDesertBiomeGenerator {
    type BiomeData = GrassyDesertBiomeData;

    fn generate_biome_data(
        &self,
        meta: &ChunkMetadata,
        asset_server: &AssetServer,
    ) -> Self::BiomeData {
        GrassyDesertBiomeData {
            noise_map: asset_server.add(self.get_biome_noise(meta)),
            trees_loaded: false,
            grass_gte: 120,
            desert_lte: 110,
        }
    }
}

/// Specifies how to blend between grass and desert.
/// [0, desert] = full desert
/// (desert, grass) = blend grass and desert
/// [grass, 255] = full desert
pub struct GrassyDesertBiomeData {
    pub noise_map: Handle<Image>,
    pub trees_loaded: bool,

    pub grass_gte: u8,
    pub desert_lte: u8,
}

impl GrassyDesertBiomeData {
    fn tree_can_spawn(&self, value: u8) -> bool {
        value >= self.grass_gte
    }
}

struct Params<'a, R: Rng> {
    chunk: &'a Chunk<GrassyDesertBiomeData>,
    noise: &'a [u8],
    min_radius: f64,
    rng: &'a mut R,
}

pub fn gen_grassy_desert_trees(
    mut commands: Commands,
    mut query: Query<(&mut Chunk<GrassyDesertBiomeData>, Entity)>,
    images: Res<Assets<Image>>,
    mut snap_to_floor: EventWriter<SnapToFloor>,
) {
    for (mut chunk, entity) in query.iter_mut() {
        if chunk.biome_data.trees_loaded {
            continue;
        };
        let Some(noise_map) = images.get(&chunk.biome_data.noise_map) else {
            continue;
        };
        chunk.biome_data.trees_loaded = true;

        // Seed from first 32 pixels for deterministic tree positioning.
        let seed: [u8; 32] = noise_map.data[..32]
            .try_into()
            .expect("seed to be 32 bytes");
        let mut rng = utils::create_rng_from_seed(seed);
        let positions = get_tree_positions(Params {
            chunk: &chunk,
            min_radius: 3.0,
            noise: &noise_map.data,
            rng: &mut rng,
        });

        for position in positions {
            commands.entity(entity).with_children(|parent| {
                // Y axis is set to zero and set to proper value using snap_to_floor.
                let translation = Vec3::new(position.x, 0.0, position.y);
                let rotation = Quat::from_rotation_y(rng.gen_range(0.0..FRAC_2_PI));
                let transform = Transform::default()
                    .with_translation(translation)
                    .with_rotation(rotation);
                let tree = parent
                    .spawn((
                        Tree::rand(utils::coords_to_u64(position)),
                        SpatialBundle {
                            transform,
                            visibility: Visibility::Hidden,
                            ..default()
                        },
                    ))
                    .id();
                snap_to_floor.send(SnapToFloor::new(tree).set_visible().with_offset(-1.0));
            });
        }
    }
}

fn get_tree_positions<R: Rng>(params: Params<R>) -> Vec<Vec2> {
    let points = utils::poisson_disk_sampling(
        params.chunk.meta.size as f64,
        params.chunk.meta.size as f64,
        params.min_radius,
        30,
        params.rng,
    );

    points
        .iter()
        .filter(|point| {
            let pixel_ix = point[0] as usize + point[1] as usize * params.chunk.meta.size;
            params
                .chunk
                .biome_data
                .tree_can_spawn(params.noise[pixel_ix])
        })
        .map(|point| Vec2::new(point[0] as f32, point[1] as f32))
        .collect()
}
