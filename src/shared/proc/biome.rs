use std::f32::consts::FRAC_2_PI;

use bevy::{color::palettes::css::GREEN, prelude::*};
use rand::Rng;

use crate::utils;

use super::{tree::Tree, utils::SnapToFloor, Terrain};

#[derive(Component)]
pub struct Biome {
    pub noise_map: Handle<Image>,
    trees_loaded: bool,
}

impl Biome {
    pub fn new(noise_map: Handle<Image>) -> Self {
        Self {
            noise_map,
            trees_loaded: false,
        }
    }
}

struct Params<'a, 'b, R: Rng> {
    width: usize,
    height: usize,
    noise: &'a [u8],
    min_radius: f64,
    rng: &'b mut R,
}

#[derive(Component)]
pub struct TreePosGizmo;

pub fn biome_system(
    mut commands: Commands,
    mut query: Query<(&mut Biome, Entity)>,
    mut gizmos: Gizmos,
    tree_pos_gizmos: Query<(&GlobalTransform, &Visibility), With<TreePosGizmo>>,
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

        let mut rng = rand::thread_rng();
        let positions = get_tree_positions(Params {
            width: terrain.chunk_size,
            height: terrain.chunk_size,
            min_radius: 3.0,
            noise: &noise_map.data,
            rng: &mut rng,
        });

        for position in positions {
            commands.entity(entity).with_children(|parent| {
                let transform =
                    Transform::from_translation(Vec3::new(position.x, 10.0, position.y))
                        .with_rotation(Quat::from_rotation_y(rng.gen_range(0.0..FRAC_2_PI)));
                let tree = parent
                    .spawn((
                        Tree::rand(coords_to_u64(position)),
                        SpatialBundle::from_transform(transform),
                    ))
                    .insert(Visibility::Hidden)
                    .id();
                snap_to_floor.send(SnapToFloor::new(tree).set_visible());
            });
        }
    }

    for (tree_pos, visibility) in tree_pos_gizmos.iter() {
        if visibility == Visibility::Visible {
            gizmos.sphere(tree_pos.translation(), Quat::IDENTITY, 1.0, GREEN);
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
        .filter(|point| params.noise[point[0] as usize + point[1] as usize * params.height] > 128)
        .map(|point| Vec2::new(point[0] as f32, point[1] as f32))
        .collect()
}

fn coords_to_u64(position: Vec2) -> u64 {
    let x = position.x;
    let y = position.y;
    let scale = 1_000_000.0; // Adjust scale for precision
    let ix = (x * scale) as i32;
    let iy = (y * scale) as i32;
    let ux = (ix as u64) + 0x8000_0000; // Offset to handle negatives
    let uy = (iy as u64) + 0x8000_0000;
    (ux << 32) | uy
}
