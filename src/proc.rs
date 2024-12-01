use std::f32::consts::PI;

use bevy::color::palettes::css::BLUE;
use bevy::color::palettes::tailwind::RED_500;
use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin};
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_rapier3d::prelude::*;

use crate::shared::proc::{Chunk, LoadsChunks, Terrain, TerrainConfig, TerrainPlugin};
use crate::utils::toggle_cursor_grab_with_esc;

pub fn run() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            TerrainPlugin,
            FpsOverlayPlugin {
                config: FpsOverlayConfig {
                    text_config: TextStyle {
                        font_size: 20.0,
                        color: Color::srgb(0.0, 1.0, 0.0),
                        font: default(),
                    },
                },
            },
        ))
        .register_type::<TerrainConfig>()
        // FixedPostUpdate is necessary as game logic runs in FixedUpdate
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_schedule(FixedPostUpdate))
        .add_plugins(RapierDebugRenderPlugin::default())
        .init_resource::<DebugGizmos>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                free_camera_movement,
                mouse_look,
                toggle_cursor_grab_with_esc,
                draw_gizmos,
                sync_terrain_config,
                toggle_debug_ui,
            ),
        )
        .run();
}

#[derive(Component)]
struct FreeCamera {
    speed: f32,
    walk_speed: f32,
}

#[derive(Resource, Default)]
struct DebugGizmos(bool);

fn toggle_debug_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<DebugRenderContext>,
    mut debug_gizmos: ResMut<DebugGizmos>,
) {
    if keys.just_pressed(KeyCode::KeyO) {
        debug_gizmos.0 = true;
    }
    if keys.just_pressed(KeyCode::KeyI) {
        debug_gizmos.0 = false;
    }
    config.enabled = debug_gizmos.0;
}

fn setup(mut commands: Commands) {
    commands.insert_resource(TerrainConfig {
        terrain_frequency: vec![0.005, 0.01, 0.02],
        terrain_amplitude: vec![15.0, 5.0, 0.5],
        tree_frequency: 0.05,
        tree_spawn_threshold: 0.3,
    });
    let terrain = Terrain::new_desert();
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
    commands
        .spawn((
            Camera3dBundle {
                transform: Transform::from_translation(Vec3::new(0.0, 10.0, 20.0))
                    .looking_at(Vec3::ZERO, Vec3::Y),
                camera: Camera {
                    clear_color: ClearColorConfig::Custom(Color::srgb(0.3, 0.76, 1.0)),
                    ..default()
                },
                ..default()
            },
            FreeCamera {
                speed: 100.0,
                walk_speed: 10.0,
            },
            LoadsChunks,
        ))
        .insert(RigidBody::KinematicPositionBased)
        .insert(Collider::ball(0.5))
        .insert(SpatialBundle::default())
        .insert(KinematicCharacterController {
            ..KinematicCharacterController::default()
        });
}

fn draw_gizmos(
    mut gizmos: Gizmos,
    query: Query<&Chunk>,
    terrain: Res<Terrain>,
    debug: Res<DebugGizmos>,
) {
    if !debug.0 {
        return;
    }
    gizmos.arrow(Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0), BLUE);
    gizmos.arrow(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 20.0), RED_500);
    gizmos.sphere(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, 1.0, BLUE);

    for tag in query.iter() {
        terrain.draw_chunk_gizmo(&mut gizmos, tag.position);
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

fn sync_terrain_config(mut terrain: ResMut<Terrain>, config: Option<Res<TerrainConfig>>) {
    let Some(config) = config else {
        return;
    };
    terrain.update_config(&config);
}
