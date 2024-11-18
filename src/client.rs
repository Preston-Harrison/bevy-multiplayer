use bevy::color::palettes::tailwind;
use bevy::input::mouse::MouseMotion;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use bevy_renet::transport::NetcodeClientPlugin;
use bevy_renet::RenetClientPlugin;

use std::net::UdpSocket;
use std::time::SystemTime;

use crate::message::client::ReliableMessageFromClient;
use crate::shared::AppState;
use crate::{message, shared, shared::render};

pub fn run() {
    App::new()
        .add_plugins((DefaultPlugins, Client))
        .add_systems(
            Startup,
            (spawn_view_model, spawn_text, spawn_connect_button),
        )
        .add_systems(
            Update,
            (move_player, handle_connect_button, send_ready, load_local),
        )
        .insert_resource(LoadState::None)
        .insert_state(shared::AppState::MainMenu)
        .add_plugins(shared::Game)
        .add_plugins(message::client::ClientMessagePlugin)
        .run();
}

struct Client;

impl Plugin for Client {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenetClientPlugin);
        app.add_plugins(NetcodeClientPlugin);
    }
}

#[derive(Debug, Component)]
struct Player;

#[derive(Debug, Component)]
struct WorldModelCamera;

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
                RenderLayers::layer(render::VIEW_MODEL_RENDER_LAYER),
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
                RenderLayers::layer(render::VIEW_MODEL_RENDER_LAYER),
                // The arm is free-floating, so shadows would look weird.
                NotShadowCaster,
            ));
        });
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

#[derive(Component)]
struct ConnectButton;

fn spawn_connect_button(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            ..default()
        })
        .insert(ConnectButton)
        .with_children(|parent| {
            parent
                .spawn(ButtonBundle {
                    style: Style {
                        width: Val::Px(150.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        // horizontally center child text
                        justify_content: JustifyContent::Center,
                        // vertically center child text
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    border_color: BorderColor(Color::BLACK),
                    border_radius: BorderRadius::MAX,
                    background_color: Color::srgb(0.15, 0.15, 0.15).into(),
                    ..default()
                })
                .with_children(|parent| {
                    parent.spawn(TextBundle::from_section(
                        "Connect",
                        TextStyle {
                            font_size: 40.0,
                            color: Color::srgb(0.9, 0.9, 0.9),
                            ..Default::default()
                        },
                    ));
                });
        });
}

#[derive(Resource, PartialEq)]
enum LoadState {
    None,
    Connecting,
    LocalLoaded,
    RemoteLoading,
    RemoteLoaded,
}

fn handle_connect_button(
    mut commands: Commands,
    button: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
    parent: Query<Entity, With<ConnectButton>>,
    mut load_state: ResMut<LoadState>,
) {
    if *load_state != LoadState::None {
        return;
    }
    for interaction in button.iter() {
        match *interaction {
            Interaction::Pressed => {
                *load_state = LoadState::Connecting;
                commands.entity(parent.single()).despawn_recursive();

                let client = RenetClient::new(ConnectionConfig::default());
                commands.insert_resource(client);
                let client_id = rand::random();
                println!("client id: {client_id}");
                let authentication = ClientAuthentication::Unsecure {
                    server_addr: shared::SERVER_ADDR.parse().unwrap(),
                    client_id,
                    user_data: None,
                    protocol_id: 0,
                };
                let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
                let current_time = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap();
                let transport =
                    NetcodeClientTransport::new(current_time, authentication, socket).unwrap();
                commands.insert_resource(transport);
            }
            _ => {}
        }
    }
}

fn load_local(world: &mut World) {
    world.resource_scope(|world: &mut World, mut load_state: Mut<LoadState>| {
        if *load_state != LoadState::Connecting {
            return;
        }
        shared::scenes::setup_scene_1(world);
        *load_state = LoadState::LocalLoaded;
        let mut app_state = world.get_resource_mut::<NextState<AppState>>().unwrap();
        app_state.set(AppState::InGame);
        println!("in game");
    })
}

fn send_ready(mut load_state: ResMut<LoadState>, client: Option<ResMut<RenetClient>>) {
    let Some(mut client) = client else {
        return;
    };
    if *load_state == LoadState::LocalLoaded && client.is_connected() {
        let message = ReliableMessageFromClient::Ready;
        let bytes = bincode::serialize(&message).unwrap();
        client.send_message(DefaultChannel::ReliableUnordered, bytes);
        *load_state = LoadState::RemoteLoading;
    }
}
