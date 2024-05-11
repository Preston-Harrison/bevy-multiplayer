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
    RenetClientPlugin,
};
use std::{
    env::args,
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime},
};

mod netcode;
mod utils;

fn current_time() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
}

fn client(server_addr: SocketAddr, socket: UdpSocket) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);

    app.add_plugins(RenetClientPlugin);

    let client = RenetClient::new(ConnectionConfig::default());
    app.insert_resource(client);

    // Setup the transport layer
    app.add_plugins(NetcodeClientPlugin);

    let authentication = ClientAuthentication::Unsecure {
        server_addr,
        client_id: 1,
        user_data: None,
        protocol_id: 0,
    };
    let transport = NetcodeClientTransport::new(current_time(), authentication, socket).unwrap();
    app.insert_resource(transport);

    app.run();
}

fn server(server_addr: SocketAddr) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);

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

    app.run()
}

fn main() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let server_addr = "127.0.0.1:5000".parse().unwrap();

    match args().skip(1).next() {
        Some(v) if v == "client" => client(server_addr, socket),
        Some(v) if v == "server" => server(server_addr),
        _ => panic!("must provider 'client' or 'server' as first arg"),
    };
}
