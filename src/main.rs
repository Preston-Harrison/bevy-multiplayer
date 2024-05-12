#![allow(irrefutable_let_patterns)]

use bevy::prelude::*;
use bevy_renet::{
    renet::{
        transport::{
            ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport,
            ServerAuthentication, ServerConfig,
        },
        ConnectionConfig, RenetClient, RenetServer,
    },
    transport::{NetcodeClientPlugin, NetcodeServerPlugin},
    RenetClientPlugin, RenetServerPlugin,
};
use game::GameLogic;
use netcode::{
    input::{InputBuffer, InputMapBuffer},
    read::{ClientMessages, ServerMessages},
    tick::{Tick, TickBroadcastTimer},
};
use std::{
    env::args,
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime},
};

mod game;
mod netcode;
mod utils;

const TICK_TIME: f64 = 1.0 / 20.0;

fn current_time() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
}

#[derive(States, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ClientState {
    Connecting,
    InGame,
}

fn client(server_addr: SocketAddr, socket: UdpSocket, client_id: u64) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("client {}", client_id),
            ..Default::default()
        }),
        ..Default::default()
    }));

    app.add_plugins(RenetClientPlugin);

    let client = RenetClient::new(ConnectionConfig::default());
    app.insert_resource(client);

    // Setup the transport layer
    app.add_plugins(NetcodeClientPlugin);

    let authentication = ClientAuthentication::Unsecure {
        server_addr,
        client_id,
        user_data: None,
        protocol_id: 0,
    };
    let transport = NetcodeClientTransport::new(current_time(), authentication, socket).unwrap();
    app.insert_resource(transport);
    app.insert_resource(netcode::ClientInfo { id: client_id });
    app.insert_resource(ClientMessages::default());
    app.insert_resource(InputBuffer::default());
    app.insert_resource(Time::<Fixed>::from_seconds(TICK_TIME));
    app.insert_resource(TickBroadcastTimer::default());

    app.insert_state(ClientState::Connecting);

    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Camera2dBundle::default());
    });
    app.add_systems(
        FixedUpdate,
        |ticks: Option<Res<Tick>>, mut next_state: ResMut<NextState<ClientState>>| {
            if ticks.is_some() {
                next_state.set(ClientState::InGame);
            }
        },
    );
    app.add_systems(FixedPreUpdate, netcode::read::recv_on_client);
    app.add_systems(
        FixedUpdate,
        netcode::tick::initialize_tick_on_client.run_if(in_state(ClientState::Connecting)),
    );

    app.add_systems(
        Update,
        netcode::interpolate::<Transform>.run_if(in_state(ClientState::InGame)),
    );
    app.add_systems(
        OnEnter(ClientState::InGame),
        netcode::tick::ask_for_game_updates_on_client,
    );

    app.add_systems(GameLogic, (game::move_on_client,).chain());

    app.add_systems(
        FixedUpdate,
        (
            netcode::input::read_input_on_client,
            netcode::tick::broadcast_tick_on_client,
            netcode::replace_prespawned_on_client,
            netcode::apply_transform_on_client,
            netcode::tick::set_adjustment_tick_on_client,
            game::spawn_joined_players_on_client,
            game::despawn_disconnected_players_on_client,
            game::run_game_logic_on_client,
        )
            .chain()
            .run_if(in_state(ClientState::InGame)),
    );

    app.run();
}

fn server(server_addr: SocketAddr) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "server".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    }));
    app.add_plugins(RenetServerPlugin);

    let server = RenetServer::new(ConnectionConfig::default());
    app.insert_resource(server);

    // Transport layer setup
    app.add_plugins(NetcodeServerPlugin);
    let socket = UdpSocket::bind(server_addr).unwrap();
    let server_config = ServerConfig {
        current_time: current_time(),
        max_clients: 64,
        protocol_id: 0,
        public_addresses: vec![server_addr],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
    app.insert_resource(transport);

    app.add_systems(Startup, |mut cmds: Commands| {
        cmds.spawn(Camera2dBundle::default());
    });

    app.insert_resource(ServerMessages::default());
    app.insert_resource(InputMapBuffer::default());
    app.insert_resource(netcode::tick::Tick::default());
    app.insert_resource(Time::<Fixed>::from_seconds(TICK_TIME));

    app.add_systems(GameLogic, (game::move_on_server,).chain());

    app.add_systems(
        FixedUpdate,
        (
            netcode::read::recv_on_server,
            netcode::tick::broadcast_adjustment_on_server,
            netcode::input::read_input_on_server,
            // Run simulation, send updates for current tick, then update tick.
            game::run_game_logic_on_server,
            netcode::broadcast_transforms,
            netcode::tick::increment_tick_on_server,
            netcode::conn::handle_connect_on_server,
            netcode::conn::send_join_messages_on_server,
        )
            .chain(),
    );

    app.run()
}

fn main() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let server_addr = "127.0.0.1:5000".parse().unwrap();

    match args().skip(1).next() {
        Some(v) if v == "client" => {
            let id: u64 = args()
                .skip(2)
                .next()
                .expect("must parse id")
                .parse()
                .expect("id must be number");
            client(server_addr, socket, id)
        }
        Some(v) if v == "server" => server(server_addr),
        _ => panic!("must provider 'client' or 'server' as first arg"),
    };
}
