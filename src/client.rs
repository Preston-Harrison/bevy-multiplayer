use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use bevy_renet::transport::NetcodeClientPlugin;
use bevy_renet::RenetClientPlugin;

use std::net::UdpSocket;
use std::time::SystemTime;

use crate::message::client::{MessageReaderOnClient, ReliableMessageFromClient};
use crate::message::server::ReliableMessageFromServer;
use crate::message::MessagesAvailable;
use crate::shared::objects::player::LocalPlayer;
use crate::shared::tick::get_client_tick;
use crate::shared::AppState;
use crate::shared::SpawnMode;
use crate::{message, shared};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum LoadState {
    Init,
    Connecting,
    LocalLoaded,
    RemoteLoading,
    Done,
}

pub fn run() {
    let is_server = false;
    App::new()
        .add_plugins((DefaultPlugins, Client, WorldInspectorPlugin::new()))
        .insert_state(LoadState::Init)
        .add_systems(Startup, spawn_connect_button)
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
        .add_systems(
            Update,
            (cursor_grab, toggle_cursor_grab).run_if(in_state(LoadState::Done)),
        )
        .insert_state(shared::AppState::MainMenu)
        .add_plugins((
            shared::Game { is_server },
            shared::tick::TickPlugin { is_server },
        ))
        .add_plugins(message::client::ClientMessagePlugin {
            latency: Some(0.2),
            message_loss: None,
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

#[derive(Resource, Default)]
pub struct ServerInfoReceived {
    set_player_obj: bool,
    tick: bool,
}

impl ServerInfoReceived {
    fn all(&self) -> bool {
        self.set_player_obj && self.tick
    }
}

fn set_local_player(
    mut commands: Commands,
    reader: Res<MessageReaderOnClient>,
    client: Option<ResMut<RenetClient>>,
    mut app_state: ResMut<NextState<AppState>>,
    mut load_state: ResMut<NextState<LoadState>>,
    mut server_info: Local<ServerInfoReceived>,
    ui_camera: Query<Entity, With<UICamera>>,
) {
    let Some(mut client) = client else {
        return;
    };
    for msg in reader.reliable_messages() {
        match msg {
            ReliableMessageFromServer::InitPlayer(player_info) => {
                server_info.set_player_obj = true;
                commands.insert_resource(LocalPlayer(player_info.net_obj.clone()));
                shared::objects::player::spawn_player(
                    SpawnMode::Client(player_info.tick.clone()),
                    &mut commands,
                    player_info.transform,
                    player_info.net_obj.clone(),
                );
            }
            ReliableMessageFromServer::TickSync(sync) => {
                let tick = get_client_tick(sync.tick, sync.unix_millis);
                commands.insert_resource(tick);
                server_info.tick = true;
            }
            _ => {}
        }
    }

    if server_info.all() {
        let message = ReliableMessageFromClient::ReadyForUpdates;
        let bytes = bincode::serialize(&message).unwrap();
        client.send_message(DefaultChannel::ReliableUnordered, bytes);
        app_state.set(AppState::InGame);
        load_state.set(LoadState::Done);
        commands.entity(ui_camera.single()).despawn_recursive();
    }
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
