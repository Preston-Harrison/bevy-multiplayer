use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
    },
};
use bevy_rapier3d::prelude::*;
use noise::{NoiseFn, Perlin};

use self::tree::{render_tree, Params, TreeSet};

pub mod shaders;
pub mod tree;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tree_spawn_system);
    }
}

#[derive(Component, Clone)]
pub struct ChunkTag {
    pub position: IVec2,
    pub lod: usize,
}

#[derive(Component)]
pub struct IsTree;

pub struct NoiseLayer {
    pub noise: Perlin,
    pub amplitude: f64,
    pub frequency: f64,
}

pub struct NoiseMap {
    pub noise: Perlin,
    pub frequency: f64,
}

/// Represents a terrain chunk.
#[derive(Resource)]
pub struct Terrain {
    chunk_size: usize,
    radius: i32,
    grid_spacing: usize,
    noise_layers: Vec<NoiseLayer>,
    tree_set: TreeSet,
    tree_noise: NoiseMap,
    materials: TerrainMaterials<StandardMaterial>,
}

pub struct TerrainMaterials<G: Material> {
    pub grass: Handle<G>,
}

impl Terrain {
    pub fn new(
        chunk_size: usize,
        grid_spacing: usize,
        noise_layers: Vec<NoiseLayer>,
        tree_set: TreeSet,
        tree_noise: NoiseMap,
        materials: TerrainMaterials<StandardMaterial>,
    ) -> Self {
        Self {
            chunk_size,
            radius: 10,
            grid_spacing,
            noise_layers,
            tree_noise,
            materials,
            tree_set,
        }
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
    fn generate_mesh(&self, position: IVec2, level_of_detail: usize) -> Mesh {
        let lod = level_of_detail;
        let grid_points = (self.chunk_size / (lod * self.grid_spacing)) + 1;
        let mut vertices = Vec::with_capacity(grid_points * grid_points);
        let mut uvs = Vec::with_capacity(grid_points * grid_points);
        let mut indices = Vec::new();

        // Generate vertices and heights
        for z in 0..grid_points {
            for x in 0..grid_points {
                // Calculate world positions
                let (world_x, world_z) = self.grid_point_to_world_position(
                    position,
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
                uvs.push([u, v]); // Add this line
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

    /// This requires the colldier mesh to already exist so the floor position
    /// can be found.
    /// TODO: have floor filter so raycasts only hit the floor.
    fn spawn_trees(&self, commands: &mut Commands, context: &RapierContext, chunk_tag: &ChunkTag) {
        if chunk_tag.lod > 1 {
            return;
        }
        let mut n = 0;
        let (grid, x_num, z_num) = self.generate_grid_points(chunk_tag.position, chunk_tag.lod);
        for x in 0..x_num {
            for z in 0..z_num {
                let sample_x = x as f64 * self.tree_noise.frequency;
                let sample_z = z as f64 * self.tree_noise.frequency;
                let noise = self.tree_noise.noise.get([sample_x, sample_z]);
                if noise > 0.4 && n < 10 {
                    // Spawn tree here.
                    // TODO: better algo for declustering
                    match get_spawn_origin(context, grid[x][z]) {
                        Some(intersect) => {
                            n += 1;
                            let mut params = Params::new_desert_tree();
                            params.root_transform.translation = intersect.point;
                            let handle = &self.tree_set.handles[0];
                            let tree = render_tree(commands, &handle, &params);
                            commands.entity(tree).insert((chunk_tag.clone(), IsTree));
                        }
                        None => info!("no origin"),
                    }
                }
            }
        }
    }

    /// Renders the chunk into the Bevy world.
    fn render_chunk(
        &self,
        chunk_position: IVec2,
        lod: usize,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
    ) {
        let mesh = self.generate_mesh(chunk_position, lod);
        let collider = Collider::from_bevy_mesh(&mesh, &ComputedColliderShape::TriMesh)
            .expect("collider to be constructed");
        let mesh_handle = meshes.add(mesh);

        let position = self.chunk_to_world_position(chunk_position, Vec3::ZERO);

        commands.spawn((
            MaterialMeshBundle {
                mesh: mesh_handle,
                material: self.materials.grass.clone(),
                transform: Transform::from_translation(position),
                ..default()
            },
            collider,
            RigidBody::Fixed,
            ChunkTag {
                position: chunk_position,
                lod,
            },
        ));
    }

    pub fn reload_chunks(
        &self,
        old: Option<IVec2>,
        new: IVec2,
        commands: &mut Commands,
        query: &Query<(Entity, &ChunkTag)>,
        meshes: &mut ResMut<Assets<Mesh>>,
    ) {
        let old_chunks = match old {
            Some(old) => generate_chunks_around(old, self.radius),
            None => vec![],
        };
        let new_chunks = generate_chunks_around(new, self.radius);
        let old_chunks: Vec<(IVec2, usize)> = old_chunks
            .into_iter()
            .map(|(chunk, distance)| (chunk, distance_to_lod(distance)))
            .collect();
        let new_chunks: Vec<(IVec2, usize)> = new_chunks
            .into_iter()
            .map(|(chunk, distance)| (chunk, distance_to_lod(distance)))
            .collect();
        let mut despawn = vec![];
        let mut spawn = vec![];

        for new_chunk in new_chunks.iter() {
            if !old_chunks.contains(new_chunk) {
                spawn.push(new_chunk);
            }
        }
        for old_chunk in old_chunks.iter() {
            if !new_chunks.contains(old_chunk) {
                despawn.push(old_chunk);
            }
        }

        for (entity, chunk_tag) in query.iter() {
            if despawn
                .iter()
                .map(|(pos, _)| pos)
                .find(|pos| **pos == chunk_tag.position)
                .is_some()
            {
                commands.entity(entity).despawn_recursive();
            }
        }

        for (position, lod) in spawn {
            self.render_chunk(*position, *lod, commands, meshes);
        }
    }
}

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

fn distance_to_lod(distance: i32) -> usize {
    if distance <= 1 {
        return 1;
    }
    if distance <= 6 {
        return 2;
    }
    return 4;
}

fn get_spawn_origin(context: &RapierContext, position: Vec2) -> Option<RayIntersection> {
    let start = Vec3::new(position.x, 100.0, position.y);
    context
        .cast_ray_and_get_normal(start, -Vec3::Y, 150.0, false, QueryFilter::default())
        .map(|v| v.1)
}

pub fn tree_spawn_system(
    mut commands: Commands,
    context: Res<RapierContext>,
    chunks: Query<&ChunkTag, (Added<ChunkTag>, Without<IsTree>)>,
    terrain: Option<Res<Terrain>>,
) {
    let Some(terrain) = terrain else {
        return;
    };
    for chunk in chunks.iter() {
        info!("running tree system on chunk");
        terrain.spawn_trees(&mut commands, &context, chunk);
    }
}
