use std::f32::consts::PI;

use bevy::color::palettes::css::{BLUE, GREEN};
use bevy::color::palettes::tailwind::RED_500;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::input::mouse::MouseMotion;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, NotShadowCaster};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, Mesh, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
};
use bevy::render::view::ColorGrading;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_rapier3d::prelude::*;
use noise::{NoiseFn, Perlin};

struct TerrainChunk {
    position: IVec2,        // Position in chunk grid coordinates
    level_of_detail: usize, // LOD factor; higher values mean less detail
}

#[derive(Component)]
struct ChunkTag;

/// Represents a terrain chunk.
struct Terrain {
    chunk_size: usize, // Size of the chunk in units
    grid_spacing: usize,
}

impl Terrain {
    fn position_to_chunk(&self, position: Vec3) -> IVec2 {
        IVec2::new(
            position.x as i32 / self.chunk_size as i32,
            position.z as i32 / self.chunk_size as i32,
        )
    }

    /// Generates a terrain mesh for this chunk using layered noise maps.
    fn generate_mesh(
        &self,
        position: IVec2,
        level_of_detail: usize,
        noise_layers: &[(Perlin, f64, f64)], // (noise generator, amplitude, frequency)
    ) -> Mesh {
        let lod = level_of_detail;
        let grid_points = (self.chunk_size / (lod * self.grid_spacing)) + 1;
        let mut vertices = Vec::with_capacity(grid_points * grid_points);
        let mut indices = Vec::new();

        // Generate vertices and heights
        for z in 0..grid_points {
            for x in 0..grid_points {
                // Calculate world positions
                let world_x = (position.x * self.chunk_size as i32
                    + x as i32 * lod as i32 * self.grid_spacing as i32)
                    as f64;
                let world_z = (position.y * self.chunk_size as i32
                    + z as i32 * lod as i32 * self.grid_spacing as i32)
                    as f64;

                // Compute height using layered noise
                let mut height = 0.0f32;
                for (noise, amplitude, frequency) in noise_layers {
                    let sample_x = world_x * *frequency;
                    let sample_z = world_z * *frequency;
                    let noise_value = noise.get([sample_x, sample_z]) as f32;
                    height += noise_value * *amplitude as f32;
                }

                vertices.push([
                    x as f32 * lod as f32 * self.grid_spacing as f32,
                    height,
                    z as f32 * lod as f32 * self.grid_spacing as f32,
                ]);
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
        mesh.insert_indices(Indices::U32(indices));
        mesh.compute_normals();

        mesh
    }

    /// Renders the chunk into the Bevy world.
    fn render_chunk(
        &self,
        chunk: &TerrainChunk,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
        noise_layers: &[(Perlin, f64, f64)], // (noise generator, amplitude, frequency)
    ) {
        let mesh = self.generate_mesh(chunk.position, chunk.level_of_detail, noise_layers);
        let collider = Collider::from_bevy_mesh(&mesh, &ComputedColliderShape::TriMesh)
            .expect("collider to be constructed");
        let mesh_handle = meshes.add(mesh);

        // Position the chunk in world space
        let position = Vec3::new(
            (chunk.position.x * self.chunk_size as i32) as f32,
            0.0,
            (chunk.position.y * self.chunk_size as i32) as f32,
        );

        commands.spawn((
            PbrBundle {
                mesh: mesh_handle,
                material: materials.add(StandardMaterial {
                    base_color: GREEN.into(),
                    ..default()
                }),
                transform: Transform::from_translation(position),
                ..default()
            },
            collider,
            RigidBody::Fixed,
            ChunkTag,
        ));

        let mesh = Plane3d::new(Vec3::Y, Vec2::splat(self.chunk_size as f32 / 2.0)).into();
        let offset = Vec3::new(self.chunk_size as f32, 0.0, self.chunk_size as f32);
        let collider = Collider::from_bevy_mesh(&mesh, &ComputedColliderShape::TriMesh)
            .expect("plane to make mesh");
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(mesh),
                material: materials.add(StandardMaterial {
                    base_color: BLUE.into(),
                    ..Default::default()
                }),
                transform: Transform::from_translation(position + (offset / 2.0)),
                ..default()
            },
            NotShadowCaster,
            collider,
            RigidBody::Fixed,
            ChunkTag,
        ));
    }

    fn render_chunks(
        &self,
        active: IVec2,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
        noise_layers: &[(Perlin, f64, f64)],
    ) {
        const RADIUS: i32 = 1;

        for x in (-RADIUS)..=RADIUS {
            for y in (-RADIUS)..=RADIUS {
                let lod = 1;
                let chunk = TerrainChunk {
                    position: IVec2::new(active.x + x, active.y + y),
                    level_of_detail: lod,
                };
                self.render_chunk(&chunk, commands, meshes, materials, noise_layers);
            }
        }
    }

    fn unload_chunks(&self, commands: &mut Commands, query: Query<Entity, With<ChunkTag>>) {
        for entity in query.iter() {
            commands.entity(entity).despawn_recursive();
        }
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

        wasd_velocity = wasd_velocity.normalize_or_zero();
        wasd_velocity.y = 0.0;

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

#[derive(Component)]
struct FreeCamera {
    speed: f32,
    walk_speed: f32,
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, MaterialPlugin::<Water>::default()))
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

const CAMERA: f32 = 100.0;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let terrain = Terrain {
        chunk_size: 100,
        grid_spacing: 5,
    };
    let mut chunk = TerrainChunk {
        position: IVec2::ZERO,
        level_of_detail: 1,
    };
    let noise_layers = vec![
        (Perlin::new(0), 15.0, 0.005), // (noise generator, amplitude, frequency)
        (Perlin::new(1), 7.5, 0.01),
        (Perlin::new(2), 3.75, 0.02),
    ];
    for z in -3..3 {
        for x in -3..3 {
            chunk.position = IVec2::new(z, x);
            terrain.render_chunk(
                &chunk,
                &mut commands,
                &mut meshes,
                &mut materials,
                &noise_layers,
            );
        }
    }

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: light_consts::lux::FULL_DAYLIGHT,
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
                walk_speed: 10.0,
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

fn draw_gizmos(mut gizmos: Gizmos) {
    gizmos.sphere(Vec3::splat(CAMERA), Quat::default(), 1.0, RED_500);
}

fn cursor_grab(
    buttons: Res<ButtonInput<MouseButton>>,
    mut q_windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if buttons.just_pressed(MouseButton::Left) {
        let mut primary_window = q_windows.single_mut();

        // for a game that doesn't use the cursor (like a shooter):
        // use `Locked` mode to keep the cursor in one place
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
struct ActiveChunk(IVec2);

fn render_chunks(
    mut active: Local<ActiveChunk>,
    player: Query<&Transform, With<FreeCamera>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    chunks: Query<Entity, With<ChunkTag>>,
) {
    let Ok(player) = player.get_single() else {
        warn!("no player");
        return;
    };

    let terrain = Terrain {
        chunk_size: 100,
        grid_spacing: 5,
    };
    let noise_layers = vec![
        (Perlin::new(0), 15.0, 0.005), // (noise generator, amplitude, frequency)
        (Perlin::new(1), 7.5, 0.01),
        (Perlin::new(2), 3.75, 0.02),
    ];
    let position = terrain.position_to_chunk(player.translation);
    dbg!(position);
    dbg!(active.0);
    if position != active.0 {
        active.0 = position;
        info!("rendering around {:?}", position);
        terrain.unload_chunks(&mut commands, chunks);
        terrain.render_chunks(
            position,
            &mut commands,
            &mut meshes,
            &mut materials,
            &noise_layers,
        );
    }
}
