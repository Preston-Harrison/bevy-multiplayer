use bevy::color::palettes::tailwind;
use bevy::input::mouse::MouseMotion;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::renet::{ConnectionConfig, RenetClient};
use bevy_renet::transport::NetcodeClientPlugin;
use bevy_renet::RenetClientPlugin;

use std::net::UdpSocket;
use std::time::SystemTime;

use crate::{message, shared};

pub fn run() {
    App::new()
        .add_plugins((DefaultPlugins, Client))
        .add_systems(
            Startup,
            (
                spawn_view_model,
                spawn_world_model,
                spawn_lights,
                spawn_text,
            ),
        )
        .add_systems(Update, (move_player, change_fov))
        .add_plugins(shared::Game)
        .add_plugins(message::client::ClientMessagePlugin)
        .run();
}

struct Client;

impl Plugin for Client {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenetClientPlugin);

        let client = RenetClient::new(ConnectionConfig::default());
        app.insert_resource(client);

        app.add_plugins(NetcodeClientPlugin);

        let authentication = ClientAuthentication::Unsecure {
            server_addr: shared::SERVER_ADDR.parse().unwrap(),
            client_id: 0,
            user_data: None,
            protocol_id: 0,
        };
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();
        app.insert_resource(transport);
    }
}

#[derive(Debug, Component)]
struct Player;

#[derive(Debug, Component)]
struct WorldModelCamera;

/// Used implicitly by all entities without a `RenderLayers` component.
/// Our world model camera and all objects other than the player are on this layer.
/// The light source belongs to both layers.
const DEFAULT_RENDER_LAYER: usize = 0;

/// Used by the view model camera and the player's arm.
/// The light source belongs to both layers.
const VIEW_MODEL_RENDER_LAYER: usize = 1;

fn spawn_view_model(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let arm = meshes.add(Cuboid::new(0.1, 0.1, 0.5));
    let arm_material = materials.add(Color::from(tailwind::TEAL_200));

    commands
        .spawn((
            Player,
            SpatialBundle {
                transform: Transform::from_xyz(0.0, 1.0, 0.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                WorldModelCamera,
                Camera3dBundle {
                    projection: PerspectiveProjection {
                        fov: 90.0_f32.to_radians(),
                        ..default()
                    }
                    .into(),
                    ..default()
                },
            ));

            // Spawn view model camera.
            parent.spawn((
                Camera3dBundle {
                    camera: Camera {
                        // Bump the order to render on top of the world model.
                        order: 1,
                        ..default()
                    },
                    projection: PerspectiveProjection {
                        fov: 70.0_f32.to_radians(),
                        ..default()
                    }
                    .into(),
                    ..default()
                },
                // Only render objects belonging to the view model.
                RenderLayers::layer(VIEW_MODEL_RENDER_LAYER),
            ));

            // Spawn the player's right arm.
            parent.spawn((
                MaterialMeshBundle {
                    mesh: arm,
                    material: arm_material,
                    transform: Transform::from_xyz(0.2, -0.1, -0.25),
                    ..default()
                },
                // Ensure the arm is only rendered by the view model camera.
                RenderLayers::layer(VIEW_MODEL_RENDER_LAYER),
                // The arm is free-floating, so shadows would look weird.
                NotShadowCaster,
            ));
        });
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

    commands.spawn(MaterialMeshBundle {
        mesh: cube.clone(),
        material: material.clone(),
        transform: Transform::from_xyz(0.0, 0.25, -3.0),
        ..default()
    });

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

fn spawn_text(mut commands: Commands) {
    commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(12.0),
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            parent.spawn(TextBundle::from_section(
                concat!(
                    "Move the camera with your mouse.\n",
                    "Press arrow up to decrease the FOV of the world model.\n",
                    "Press arrow down to increase the FOV of the world model."
                ),
                TextStyle {
                    font_size: 25.0,
                    ..default()
                },
            ));
        });
}

fn move_player(
    mut mouse_motion: EventReader<MouseMotion>,
    mut player: Query<&mut Transform, With<Player>>,
) {
    let mut transform = player.single_mut();
    for motion in mouse_motion.read() {
        let yaw = -motion.delta.x * 0.003;
        let pitch = -motion.delta.y * 0.002;
        // Order of rotations is important, see <https://gamedev.stackexchange.com/a/136175/103059>
        transform.rotate_y(yaw);
        transform.rotate_local_x(pitch);
    }
}

fn change_fov(
    input: Res<ButtonInput<KeyCode>>,
    mut world_model_projection: Query<&mut Projection, With<WorldModelCamera>>,
) {
    let mut projection = world_model_projection.single_mut();
    let Projection::Perspective(ref mut perspective) = projection.as_mut() else {
        unreachable!(
            "The `Projection` component was explicitly built with `Projection::Perspective`"
        );
    };

    if input.pressed(KeyCode::ArrowUp) {
        perspective.fov -= 1.0_f32.to_radians();
        perspective.fov = perspective.fov.max(20.0_f32.to_radians());
    }
    if input.pressed(KeyCode::ArrowDown) {
        perspective.fov += 1.0_f32.to_radians();
        perspective.fov = perspective.fov.min(160.0_f32.to_radians());
    }
}
