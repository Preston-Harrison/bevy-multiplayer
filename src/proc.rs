use std::f32::consts::PI;

use bevy::color::palettes::css::BLUE;
use bevy::color::palettes::tailwind::RED_500;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin};
use bevy::input::mouse::MouseMotion;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::{Mesh, MeshVertexBufferLayoutRef};
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
};
use bevy::render::view::ColorGrading;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_rapier3d::prelude::*;
use noise::Perlin;

use crate::shared::proc::tree::{render_tree, Params, TreeSet};
use crate::shared::proc::{
    ChunkTag, NoiseLayer, NoiseMap, Terrain, TerrainMaterials, TerrainPlugin,
};

pub fn run() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            TerrainPlugin,
            FpsOverlayPlugin {
                config: FpsOverlayConfig {
                    text_config: TextStyle {
                        // Here we define size of our overlay
                        font_size: 50.0,
                        // We can also change color of the overlay
                        color: Color::srgb(0.0, 1.0, 0.0),
                        // If we want, we can use a custom font
                        font: default(),
                    },
                },
            },
        ))
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        // .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                free_camera_movement,
                mouse_look,
                cursor_grab,
                toggle_cursor_grab,
                draw_gizmos,
                render_chunks,
            ),
        )
        .run();
}

#[derive(Component)]
struct FreeCamera {
    speed: f32,
    walk_speed: f32,
}

fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    asset_server: Res<AssetServer>,
) {
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
    let terrain_materials = TerrainMaterials {
        grass: materials.add(StandardMaterial {
            base_color: LinearRgba::new(1.0, 0.37, 0.1, 1.0).into(),
            normal_map_texture: Some(asset_server.load("sand_dune_texture.png")),
            ..default()
        }),
    };
    let tree_noise = NoiseMap {
        noise: Perlin::new(3),
        frequency: 0.01,
    };
    let params = Params::new_desert_tree();
    let tree_set = TreeSet::new(&[params], &mut meshes, &mut materials);
    let terrain = Terrain::new(
        100,
        5,
        noise_layers,
        tree_set,
        tree_noise,
        terrain_materials,
    );
    commands.insert_resource(terrain);

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

    // Add a free camera
    let mut color_grading = ColorGrading::default();
    color_grading.global.tint = -1.0;
    commands
        .spawn((
            Camera3dBundle {
                transform: Transform::from_translation(Vec3::new(0.0, 10.0, 20.0))
                    .looking_at(Vec3::ZERO, Vec3::Y),
                camera: Camera {
                    clear_color: ClearColorConfig::Custom(Color::srgb(0.3, 0.76, 1.0)),
                    ..default()
                },
                color_grading,
                ..default()
            },
            FreeCamera {
                speed: 100.0,
                walk_speed: 30.0,
            },
            DepthPrepass,
        ))
        .insert(RigidBody::KinematicPositionBased)
        .insert(Collider::ball(0.5))
        .insert(SpatialBundle::default())
        .insert(KinematicCharacterController {
            ..KinematicCharacterController::default()
        });
}

fn draw_gizmos(mut gizmos: Gizmos, query: Query<&ChunkTag>, terrain: Res<Terrain>) {
    gizmos.arrow(Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0), BLUE);
    gizmos.arrow(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 20.0), RED_500);
    gizmos.sphere(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, 1.0, BLUE);

    for tag in query.iter() {
        gizmos.rect(
            terrain.chunk_to_world_position(tag.position, Vec3::ZERO) + terrain.mid_chunk_offset(),
            Quat::from_rotation_x(PI / 2.0),
            Vec2::splat(100.0),
            BLUE,
        );
    }
}

fn cursor_grab(
    buttons: Res<ButtonInput<MouseButton>>,
    mut q_windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if buttons.just_pressed(MouseButton::Left) {
        let mut primary_window = q_windows.single_mut();
        primary_window.cursor.grab_mode = CursorGrabMode::Locked;
        primary_window.cursor.visible = false;
    }
}

fn toggle_cursor_grab(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        let mut primary_window = q_windows.single_mut();

        primary_window.cursor.grab_mode = CursorGrabMode::None;
        primary_window.cursor.visible = true;
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct Water {}

impl Material for Water {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Default)]
struct ActiveChunk(Option<IVec2>);

fn render_chunks(
    mut active: Local<ActiveChunk>,
    player: Query<&Transform, With<FreeCamera>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain: Res<Terrain>,
    chunks: Query<(Entity, &ChunkTag)>,
) {
    let Ok(player) = player.get_single() else {
        warn!("no player");
        return;
    };

    let position = terrain.world_position_to_chunk(player.translation);
    if Some(position) != active.0 {
        info!("reloading chunks");
        terrain.reload_chunks(active.0, position, &mut commands, &chunks, &mut meshes);
        active.0 = Some(position);
    }
}

#[derive(Default)]
struct PhysicsEnabled(bool);

// Free camera system
fn free_camera_movement(
    mut physics_enabled: Local<PhysicsEnabled>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<(
        &mut Transform,
        &mut KinematicCharacterController,
        &mut FreeCamera,
    )>,
) {
    for (mut transform, mut controller, camera) in query.iter_mut() {
        if keys.just_pressed(KeyCode::KeyP) {
            physics_enabled.0 = !physics_enabled.0;
        }

        let forward = transform.rotation.mul_vec3(Vec3::new(0.0, 0.0, -1.0));
        let right = transform.rotation.mul_vec3(Vec3::new(1.0, 0.0, 0.0));

        let mut wasd_velocity = Vec3::ZERO;
        let mut vertical = 0.0;

        if keys.pressed(KeyCode::KeyW) {
            wasd_velocity += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            wasd_velocity -= forward;
        }
        if keys.pressed(KeyCode::KeyA) {
            wasd_velocity -= right;
        }
        if keys.pressed(KeyCode::KeyD) {
            wasd_velocity += right;
        }
        if keys.pressed(KeyCode::Space) {
            vertical += 1.0;
        }
        if keys.pressed(KeyCode::ShiftLeft) {
            vertical -= 1.0;
        }

        wasd_velocity.y = 0.0;
        wasd_velocity = wasd_velocity.normalize_or_zero();

        if physics_enabled.0 {
            controller.translation =
                Some((wasd_velocity * camera.walk_speed + Vec3::Y * -9.8) * time.delta_seconds());
        } else {
            wasd_velocity.y = vertical;
            transform.translation += wasd_velocity * time.delta_seconds() * camera.speed;
        }
    }
}

/// Rotates the player based on mouse movement.
fn mouse_look(
    mut mouse_motion: EventReader<MouseMotion>,
    mut camera: Query<&mut Transform, With<FreeCamera>>,
    q_windows: Query<&Window, With<PrimaryWindow>>,
) {
    let primary_window = q_windows.single();
    if primary_window.cursor.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok(mut camera) = camera.get_single_mut() else {
        return;
    };
    for motion in mouse_motion.read() {
        let yaw = -motion.delta.x * 0.003;
        let pitch = -motion.delta.y * 0.002;
        camera.rotate_y(yaw);
        camera.rotate_local_x(pitch);
    }
}
