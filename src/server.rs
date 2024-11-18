use std::{net::UdpSocket, time::SystemTime};

use bevy::prelude::*;
use bevy_renet::{
    renet::{
        transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
        ConnectionConfig, RenetServer, ServerEvent,
    },
    transport::NetcodeServerPlugin,
    RenetServerPlugin,
};

use crate::shared;

pub fn run() {
    App::new()
        .add_plugins((DefaultPlugins, Server))
        .add_systems(Startup, setup)
        .add_systems(Update, handle_server_events)
        .add_systems(Update, server_broadcast)
        .run();
}

struct Server;

impl Plugin for Server {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenetServerPlugin);

        let server = RenetServer::new(ConnectionConfig::default());
        app.insert_resource(server);

        app.add_plugins(NetcodeServerPlugin);
        let server_addr = shared::SERVER_ADDR.parse().unwrap();
        let socket = UdpSocket::bind(server_addr).unwrap();
        let server_config = ServerConfig {
            current_time: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
            max_clients: 64,
            protocol_id: 0,
            public_addresses: vec![server_addr],
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
        app.insert_resource(transport);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera3dBundle::default());
}

fn handle_server_events(mut server: EventReader<ServerEvent>) {
    for event in server.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                println!("Client {} connected", client_id);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                println!("Client {} disconnected: {:?}", client_id, reason);
            }
        }
    }
}

fn server_broadcast(mut server: ResMut<RenetServer>) {
    for client_id in server.clients_id() {
        let message = b"Hello from the server!";
        server.send_message(client_id, 0, message.to_vec());
    }
}
