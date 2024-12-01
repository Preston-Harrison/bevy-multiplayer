use std::f32::consts::FRAC_2_PI;

use bevy::prelude::*;
use rand::Rng;

use crate::utils;

use super::{tree::Tree, utils::SnapToFloor, Terrain};

#[derive(Component)]
pub struct Biome {
    pub noise_map: Handle<Image>,
    pub biome_blend: BiomeBlend,
    trees_loaded: bool,
}

impl Biome {
    pub fn new(noise_map: Handle<Image>) -> Self {
        Self {
            noise_map,
            trees_loaded: false,
            biome_blend: BiomeBlend {
                grass_gte: 120,
                desert_lte: 110,
            },
        }
    }
}

/// Specifies how to blend between grass and desert.
/// [0, desert] = full desert
/// (desert, grass) = blend grass and desert
/// [grass, 255] = full desert
pub struct BiomeBlend {
    pub grass_gte: u8,
    pub desert_lte: u8,
}

impl BiomeBlend {
    fn tree_can_spawn(&self, value: u8) -> bool {
        value >= self.grass_gte
    }
}

struct Params<'a, R: Rng> {
    width: usize,
    height: usize,
    noise: &'a [u8],
    biome_blend: &'a BiomeBlend,
    min_radius: f64,
    rng: &'a mut R,
}

pub fn biome_system(
    mut commands: Commands,
    mut query: Query<(&mut Biome, Entity)>,
    images: Res<Assets<Image>>,
    terrain: Res<Terrain>,
    mut snap_to_floor: EventWriter<SnapToFloor>,
) {
    for (mut biome, entity) in query.iter_mut() {
        if biome.trees_loaded {
            continue;
        };
        let Some(noise_map) = images.get(&biome.noise_map) else {
            continue;
        };
        biome.trees_loaded = true;

        // Seed from first 32 pixels for deterministic tree positioning.
        let seed: [u8; 32] = noise_map.data[..32]
            .try_into()
            .expect("seed to be 32 bytes");
        let mut rng = utils::create_rng_from_seed(seed);
        let positions = get_tree_positions(Params {
            width: terrain.chunk_size,
            height: terrain.chunk_size,
            min_radius: 3.0,
            noise: &noise_map.data,
            biome_blend: &biome.biome_blend,
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
                snap_to_floor.send(SnapToFloor::new(tree).set_visible().with_offset(-1.5));
            });
        }
    }
}

fn get_tree_positions<R: Rng>(params: Params<R>) -> Vec<Vec2> {
    let points = utils::poisson_disk_sampling(
        params.width as f64,
        params.height as f64,
        params.min_radius,
        30,
        params.rng,
    );

    points
        .iter()
        .filter(|point| {
            let pixel_ix = point[0] as usize + point[1] as usize * params.height;
            params.biome_blend.tree_can_spawn(params.noise[pixel_ix])
        })
        .map(|point| Vec2::new(point[0] as f32, point[1] as f32))
        .collect()
}
