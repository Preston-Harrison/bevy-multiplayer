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
use netcode::{
    input::{InputBuffer, InputMapBuffer},
    read::{ClientMessages, ServerMessages},
    LocalPlayer,
};
use std::{
    env::args,
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime},
};

mod game;
mod netcode;
mod utils;

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
            title: "client".to_string(),
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
    app.insert_resource(netcode::tick::Tick::default());
    app.insert_resource(InputBuffer::default());

    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((TransformBundle::default(), LocalPlayer));
    });

    app.insert_state(ClientState::Connecting);
    app.add_systems(
        FixedUpdate,
        (
            netcode::read::recv_on_client,
            netcode::replace_prespawned_on_client,
            netcode::apply_transform_on_client,
            netcode::interpolate::<Transform>,
            netcode::input::read_input_on_client,
            game::move_on_client,
            netcode::tick::increment_tick,
        )
            .chain()
            .run_if(in_state(ClientState::Connecting)),
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

    app.add_systems(
        FixedUpdate,
        (
            netcode::read::recv_on_server,
            netcode::input::read_input_on_server,
            game::move_on_server,
            netcode::conn::handle_connect_on_server,
            netcode::tick::increment_tick,
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
