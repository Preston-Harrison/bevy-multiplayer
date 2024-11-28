use std::f32::consts::PI;

use bevy::{ecs::system::RunSystemOnce, prelude::*};

use super::proc::Terrain;

pub fn setup_scene_1(world: &mut World) {
    world.run_system_once(spawn_world_model);
    world.run_system_once(spawn_lights);
}

/// TODO: static network objects for procedurally generated terrain
fn spawn_world_model(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let terrain = Terrain::new_desert(&asset_server, &mut materials);
    commands.insert_resource(terrain);
}

fn spawn_lights(mut commands: Commands) {
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform {
            translation: Vec3::new(0.0, 2.0, 0.0),
            rotation: Quat::from_rotation_x(-PI / 4.),
            ..default()
        },
        ..default()
    });
}
