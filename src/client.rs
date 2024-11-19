use bevy::prelude::*;
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use bevy_renet::transport::NetcodeClientPlugin;
use bevy_renet::RenetClientPlugin;

use std::net::UdpSocket;
use std::time::SystemTime;

use crate::message::client::{MessageReaderOnClient, ReliableMessageFromClient};
use crate::message::server::ReliableMessageFromServer;
use crate::message::MessagesAvailable;
use crate::shared::objects::player::{LocalPlayer, Player};
use crate::shared::objects::NetworkObject;
use crate::shared::AppState;
use crate::{message, shared};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum LoadState {
    Init,
    Connecting,
    LocalLoaded,
    RemoteLoading,
    WaitingForPlayerSpawn,
    Done,
}

pub fn run() {
    App::new()
        .add_plugins((DefaultPlugins, Client))
        .insert_state(LoadState::Init)
        .add_systems(Startup, spawn_connect_button)
        .add_systems(
            FixedUpdate,
            spawn_view_model.run_if(in_state(LoadState::WaitingForPlayerSpawn)),
        )
        .add_systems(OnEnter(LoadState::Connecting), load_local)
        .add_systems(
            FixedUpdate,
            (
                handle_connect_button.run_if(in_state(LoadState::Init)),
                send_ready.run_if(in_state(LoadState::LocalLoaded)),
                set_local_player.run_if(in_state(LoadState::RemoteLoading)),
            )
                .in_set(MessagesAvailable),
        )
        .insert_state(shared::AppState::MainMenu)
        .add_plugins(shared::Game)
        .add_plugins(message::client::ClientMessagePlugin {
            latency: Some(0.2),
            message_loss: Some(0.05),
        })
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
struct WorldModelCamera;

fn spawn_view_model(
    mut commands: Commands,
    players: Query<(Entity, &NetworkObject), Added<Player>>,
    cameras: Query<Entity, With<UICamera>>,
    local_player: Res<LocalPlayer>,
    mut load_state: ResMut<NextState<LoadState>>,
) {
    let mut entity = None;
    for (e, net_obj) in players.iter() {
        if *net_obj == local_player.0 {
            entity = Some(e);
        }
    }

    let Some(entity) = entity else {
        return;
    };
    load_state.set(LoadState::Done);

    for camera in cameras.iter() {
        commands.entity(camera).despawn_recursive();
    }

    println!("spawning player camera");
    commands.entity(entity).with_children(|parent| {
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
    });
}

#[derive(Component)]
struct ConnectButton;

#[derive(Component)]
struct UICamera;

fn spawn_connect_button(mut commands: Commands) {
    commands.spawn((UICamera, Camera3dBundle::default()));
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

fn handle_connect_button(
    mut commands: Commands,
    button: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
    parent: Query<Entity, With<ConnectButton>>,
    mut load_state: ResMut<NextState<LoadState>>,
) {
    for interaction in button.iter() {
        match *interaction {
            Interaction::Pressed => {
                load_state.set(LoadState::Connecting);
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
    world.resource_scope(
        |world: &mut World, mut load_state: Mut<NextState<LoadState>>| {
            shared::scenes::setup_scene_1(world);
            load_state.set(LoadState::LocalLoaded);
            println!("loaded local");
        },
    )
}

fn send_ready(mut load_state: ResMut<NextState<LoadState>>, client: Option<ResMut<RenetClient>>) {
    let Some(mut client) = client else {
        return;
    };
    if client.is_connected() {
        let message = ReliableMessageFromClient::Connected;
        println!("connected");
        let bytes = bincode::serialize(&message).unwrap();
        client.send_message(DefaultChannel::ReliableUnordered, bytes);
        load_state.set(LoadState::RemoteLoading);
    }
}

fn set_local_player(
    mut commands: Commands,
    reader: Res<MessageReaderOnClient>,
    client: Option<ResMut<RenetClient>>,
    mut app_state: ResMut<NextState<AppState>>,
    mut load_state: ResMut<NextState<LoadState>>,
) {
    let Some(mut client) = client else {
        return;
    };
    for msg in reader.reliable_messages() {
        if let ReliableMessageFromServer::SetPlayerNetworkObject(net_obj) = msg {
            println!("set local player");
            commands.insert_resource(LocalPlayer(net_obj.clone()));
            let message = ReliableMessageFromClient::ReadyForUpdates;
            let bytes = bincode::serialize(&message).unwrap();
            client.send_message(DefaultChannel::ReliableUnordered, bytes);
            app_state.set(AppState::InGame);
            load_state.set(LoadState::WaitingForPlayerSpawn);
        }
    }
}
