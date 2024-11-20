use bevy::{
    color::palettes::tailwind, ecs::system::RunSystemOnce, prelude::*, render::view::RenderLayers,
};
use bevy_rapier3d::prelude::*;

use super::render::{DEFAULT_RENDER_LAYER, VIEW_MODEL_RENDER_LAYER};

pub fn setup_scene_1(world: &mut World) {
    world.run_system_once(spawn_world_model);
    world.run_system_once(spawn_lights);
}

fn spawn_world_model(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let floor = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(10.0)));
    let cube = meshes.add(Cuboid::new(2.0, 0.5, 1.0));
    let material = materials.add(Color::WHITE);

    // The world model camera will render the floor and the cubes spawned in this system.
    // Assigning no `RenderLayers` component defaults to layer 0.

    commands.spawn(MaterialMeshBundle {
        mesh: floor,
        material: material.clone(),
        ..default()
    });

    commands
        .spawn(MaterialMeshBundle {
            mesh: cube.clone(),
            material: material.clone(),
            transform: Transform::from_xyz(0.0, 0.25, -3.0),
            ..default()
        })
        .insert((RigidBody::Fixed, Collider::cuboid(1.0, 0.5, 0.5)));

    commands.spawn(MaterialMeshBundle {
        mesh: cube,
        material,
        transform: Transform::from_xyz(0.75, 1.75, 0.0),
        ..default()
    });
}

fn spawn_lights(mut commands: Commands) {
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::from(tailwind::ROSE_300),
                shadows_enabled: true,
                ..default()
            },
            transform: Transform::from_xyz(-2.0, 4.0, -0.75),
            ..default()
        },
        // The light source illuminates both the world model and the view model.
        RenderLayers::from_layers(&[DEFAULT_RENDER_LAYER, VIEW_MODEL_RENDER_LAYER]),
    ));
}
