use std::f32::consts::PI;

use bevy::{
    color::palettes::css::{BLUE, GREEN, RED},
    math::Affine2,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    },
    utils::HashSet,
};
use bevy_inspector_egui::InspectorOptions;
use bevy_rapier3d::prelude::*;
use noise::{NoiseFn, Perlin, Simplex};
use rock::{Rock, RockPlugin};
use shaders::GrassDesert;

use self::tree::{Tree, TreePlugin};

pub mod rock;
pub mod shaders;
pub mod tree;

pub struct TerrainPlugin {
    pub is_server: bool,
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (chunk_load_system, tree_spawn_system));
        app.add_plugins((TreePlugin, RockPlugin));
        app.add_plugins(MaterialPlugin::<GrassDesert>::default());
    }
}

/// Describes a chunk parent entity. Everything local to the chunk (floor, trees,
/// but not entities that can walk across chunks) is a child of this.
#[derive(Component, Clone, Debug)]
pub struct Chunk {
    pub position: IVec2,
    /// Level of Detail. This is only useful for rendering on the client.
    /// This is always 1 on the server. 1 means there is no loss in detail.
    pub lod: usize,
}

const SERVER_LOD: usize = 1;

#[derive(Reflect, Resource, Default, InspectorOptions)]
#[reflect(Resource)]
pub struct TerrainConfig {
    pub terrain_frequency: Vec<f64>,
    pub terrain_amplitude: Vec<f64>,

    pub tree_frequency: f64,
    pub tree_spawn_threshold: f64,
}

struct NoiseLayer {
    noise: Perlin,
    amplitude: f64,
    frequency: f64,
}

struct NoiseMap {
    noise: Perlin,
    frequency: f64,
}

struct BiomeGenerator {
    noise: Perlin,
    frequency: f64,
}

impl BiomeGenerator {
    fn get_noise(&self, chunk: IVec2, chunk_size: usize, x: usize, z: usize) -> f64 {
        let sample_x = (chunk.x as f64 * chunk_size as f64 + x as f64) * self.frequency;
        let sample_z = (chunk.y as f64 * chunk_size as f64 + z as f64) * self.frequency;
        self.noise.get([sample_x, sample_z])
    }

    fn get(&self, chunk: IVec2, chunk_size: usize, x: usize, z: usize) -> Biome {
        Biome::from_noise(self.get_noise(chunk, chunk_size, x, z))
    }
}

enum Biome {
    Desert(f32),
    Forest(f32),
}

impl Biome {
    fn from_noise(sample: f64) -> Self {
        assert!(sample >= -1.0 || sample <= 1.0);

        let mut normalized = sample;
        if normalized < 0.4 {
            normalized = 0.0;
        } else if normalized > 0.6 {
            normalized = 1.0;
        } else {
            normalized = (normalized - 0.4) * (1.0 / 0.2);
        }
        Self::Desert(normalized as f32)
    }

    fn get_vertex_color(&self) -> [f32; 4] {
        match self {
            Self::Desert(sample) => [*sample, 1.0 - sample, 0.0, 1.0],
            Self::Forest(sample) => [1.0 - sample, *sample, 0.0, 1.0],
        }
    }
}

/// Represents a terrain chunk.
#[derive(Resource)]
pub struct Terrain {
    chunk_size: usize,
    /// The radius around the player(s) to generate chunks.
    radius: i32,
    grid_spacing: usize,
    noise_layers: Vec<NoiseLayer>,
    tree_noise: NoiseMap,
    tree_spawn_threshold: f64,
    biome_generator: BiomeGenerator,
}

pub struct TerrainMaterials {
    pub sand_dune: Handle<GrassDesert>,
}

impl Terrain {
    pub fn new_desert(
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        grass_desert: &mut Assets<GrassDesert>,
    ) -> Self {
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
        let biome_generator = BiomeGenerator {
            noise: Perlin::new(3),
            frequency: 0.0109,
        };
        // let dune_texture = asset_server.load_with_settings("sand_dune_texture.png", |s: &mut _| {
        //     *s = ImageLoaderSettings {
        //         sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
        //             // rewriting mode to repeat image,
        //             address_mode_u: ImageAddressMode::Repeat,
        //             address_mode_v: ImageAddressMode::Repeat,
        //             ..default()
        //         }),
        //         ..default()
        //     }
        // });
        let tree_noise = NoiseMap {
            noise: Perlin::new(3),
            frequency: 0.01,
        };
        Self {
            chunk_size: 100,
            radius: 2,
            grid_spacing: 5,
            noise_layers,
            tree_noise,
            tree_spawn_threshold: 0.4,
            biome_generator,
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

    pub fn update_config(&mut self, config: &TerrainConfig) {
        for (ix, freq) in config.terrain_frequency.iter().enumerate() {
            if let Some(layer) = self.noise_layers.get_mut(ix) {
                layer.frequency = *freq;
            }
        }
        for (ix, freq) in config.terrain_amplitude.iter().enumerate() {
            if let Some(layer) = self.noise_layers.get_mut(ix) {
                layer.amplitude = *freq;
            }
        }
        self.tree_noise.frequency = config.tree_frequency;
        self.tree_spawn_threshold = config.tree_spawn_threshold;
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

    /// Returns (data, x_num, z_num). Access: `data[grid_x][grid_z] = Vec2(world_x, world_z);`
    fn generate_grid_points(
        &self,
        chunk_position: IVec2,
        lod: usize,
    ) -> (Vec<Vec<Vec2>>, usize, usize) {
        let grid_points = (self.chunk_size / (lod * self.grid_spacing)) + 1;
        let mut points = vec![vec![Vec2::ZERO; grid_points]; grid_points];
        for z in 0..grid_points {
            for x in 0..grid_points {
                // Calculate world positions
                let (world_x, world_z) = self.grid_point_to_world_position(
                    chunk_position,
                    IVec2::new(x as i32, z as i32),
                    lod,
                );
                points[x][z] = Vec2::new(world_x as f32, world_z as f32);
            }
        }
        return (points, grid_points, grid_points);
    }

    /// Generates a terrain mesh for this chunk using layered noise maps.
    fn generate_mesh(&self, chunk_pos: IVec2, level_of_detail: usize) -> Mesh {
        let lod = level_of_detail;
        let grid_points = (self.chunk_size / (lod * self.grid_spacing)) + 1;
        let mut vertices = Vec::with_capacity(grid_points * grid_points);
        let mut uvs = Vec::with_capacity(grid_points * grid_points);
        let mut colors: Vec<[f32; 4]> = Vec::with_capacity(grid_points * grid_points);
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

                // colors.push(
                //     self.biome_generator
                //         .get(chunk_pos, grid_points, x, z)
                //         .get_vertex_color(),
                // );

                colors.push([0.0, 0.0, 0.0, 1.0]);
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
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh.compute_normals();
        mesh.generate_tangents().expect("tangents to be generated");

        mesh
    }

    /// This requires the colldier mesh to already exist so the floor position
    /// can be found.
    /// TODO: have floor filter so raycasts only hit the floor.
    fn spawn_trees_and_rocks(
        &self,
        commands: &mut Commands,
        context: &RapierContext,
        chunk: &Chunk,
        chunk_entity: Entity,
    ) {
        if chunk.lod > 2 {
            return;
        }
        let chunk_world_position = self.chunk_to_world_position(chunk.position, Vec3::ZERO);
        let (grid, x_num, z_num) = self.generate_grid_points(chunk.position, chunk.lod);
        for x in 0..x_num {
            for z in 0..z_num {
                let sample_pos = grid[x][z];
                let sample_x = sample_pos.x as f64 * self.tree_noise.frequency;
                let sample_z = sample_pos.y as f64 * self.tree_noise.frequency;
                let noise = self.tree_noise.noise.get([sample_x, sample_z]);
                if noise > self.tree_spawn_threshold {
                    // Spawn tree here.
                    // TODO: better algo than random in grid.
                    match get_spawn_origin(context, grid[x][z]) {
                        Some(intersect) => {
                            commands.entity(chunk_entity).with_children(|parent| {
                                let local_position = intersect.point - chunk_world_position;
                                if x % 2 == 0 {
                                    parent.spawn((
                                        Tree::new(),
                                        SpatialBundle::from_transform(Transform::from_translation(
                                            local_position,
                                        )),
                                    ));
                                } else {
                                    parent.spawn((
                                        Rock::new(),
                                        SpatialBundle::from_transform(Transform::from_translation(
                                            local_position,
                                        )),
                                    ));
                                };
                            });
                        }
                        None => info!("no origin"),
                    }
                }
            }
        }
    }

    fn get_biome_noise(&self, chunk_pos: IVec2) -> Image {
        let perlin = Simplex::new(3);
        let mut data = vec![0; 10_000];

        for x in 0..100 {
            for z in 0..100 {
                let sample_x = (chunk_pos.x as f64 * 100.0 + x as f64) * 0.0159;
                let sample_z = (chunk_pos.y as f64 * 100.0 + z as f64) * 0.0159;
                data[x + 100 * z] = (perlin.get([sample_x, sample_z]) * 255.0)
                    .clamp(0.0, 255.0) as u8;
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

    /// Renders the chunk into the Bevy world.
    fn render_chunk(
        &self,
        chunk: &Chunk,
        chunk_entity: Entity,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        grass_desert: &mut Assets<GrassDesert>,
        images: &mut Assets<Image>,
    ) {
        let mesh = self.generate_mesh(chunk.position, chunk.lod);
        let collider = Collider::from_bevy_mesh(&mesh, &ComputedColliderShape::TriMesh)
            .expect("collider to be constructed");
        let mesh_handle = meshes.add(mesh);

        commands.entity(chunk_entity).with_children(|parent| {
            parent.spawn((
                MaterialMeshBundle {
                    mesh: mesh_handle,
                    material: grass_desert.add(GrassDesert {
                        grass: GREEN.into(),
                        desert: RED.into(),
                        noise_texture: images.add(self.get_biome_noise(chunk.position)),
                    }),
                    ..default()
                },
                collider,
                RigidBody::Fixed,
            ));
        });
    }

    /// Creates a chunk parent entity and returns it's entity ID.
    fn create_chunk(&self, chunk: &Chunk, commands: &mut Commands) -> Entity {
        let world_pos = self.chunk_to_world_position(chunk.position, Vec3::ZERO);
        let spatial = SpatialBundle::from_transform(Transform::from_translation(world_pos));
        commands.spawn((chunk.clone(), spatial)).id()
    }

    fn unload_chunk(&self, chunk_entity: Entity, commands: &mut Commands) {
        commands.entity(chunk_entity).despawn_recursive();
    }
}

/// Includes center position with distance = 0. Returns Vec<(position, distance)>.
fn generate_chunks_around(position: IVec2, radius: i32) -> Vec<(IVec2, i32)> {
    let mut result = Vec::new();

    // Iterate over the square area around the center position
    for x in -radius..=radius {
        for y in -radius..=radius {
            let offset = IVec2::new(x, y);
            let chunk_position = position + offset;

            // Calculate Chebyshev distance (diagonals count as 1)
            let distance = x.abs().max(y.abs());

            result.push((chunk_position, distance));
        }
    }

    result
}

fn get_spawn_origin(context: &RapierContext, position: Vec2) -> Option<RayIntersection> {
    let start = Vec3::new(position.x, 100.0, position.y);
    context
        .cast_ray_and_get_normal(start, -Vec3::Y, 150.0, false, QueryFilter::default())
        .map(|v| v.1)
}

/// FIXME: this panics sometimes for some reason.
/// Spawns trees in newly generated chunks.
pub fn tree_spawn_system(
    mut commands: Commands,
    context: Res<RapierContext>,
    chunks: Query<(Entity, &Chunk), Added<Chunk>>,
    terrain: Option<Res<Terrain>>,
) {
    let Some(terrain) = terrain else {
        return;
    };
    for (entity, chunk) in chunks.iter() {
        if commands.get_entity(entity).is_none() {
            // This can happen if the chunk is despawned in the same tick.
            // See:
            //  - https://github.com/bevyengine/bevy/issues/7118
            //  - https://github.com/bevyengine/bevy/pull/15929
            continue;
        }
        trace!("spawning trees for {:?}", chunk);
        terrain.spawn_trees_and_rocks(&mut commands, &context, chunk, entity);
    }
}

#[derive(Component)]
pub struct LoadsChunks;

/// Loads and unloads chunks based on if an entity with `LoadsChunks` exists
/// there. Does this for all chunks nearby.
///
/// On clients, LoadsChunks should only be present on the client's player.
/// On the server, it should be on all players.
pub fn chunk_load_system(
    loaders: Query<&Transform, With<LoadsChunks>>,
    chunks: Query<(Entity, &Chunk)>,
    mut commands: Commands,
    terrain: Option<Res<Terrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut grass_desert: ResMut<Assets<GrassDesert>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(terrain) = terrain else {
        return;
    };

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
        if chunks_with_loaders.contains(&chunk.position) {
            loaded_chunks.insert(chunk.position);
        } else {
            terrain.unload_chunk(entity, &mut commands);
        }
    }

    let chunks_to_load = chunks_with_loaders.difference(&loaded_chunks);
    for chunk_pos in chunks_to_load {
        let chunk = Chunk {
            position: *chunk_pos,
            // TODO: add different level of detail on clients.
            lod: SERVER_LOD,
        };
        let entity = terrain.create_chunk(&chunk, &mut commands);
        terrain.render_chunk(
            &chunk,
            entity,
            &mut commands,
            &mut meshes,
            &mut grass_desert,
            &mut images,
        );
        trace!("rendered chunk {:?}", chunk);
    }
}
