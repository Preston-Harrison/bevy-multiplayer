use std::{net::UdpSocket, time::SystemTime};

use bevy::prelude::*;
use bevy_renet::{
    renet::{
        transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
        ConnectionConfig, DefaultChannel, RenetServer, ServerEvent,
    },
    transport::NetcodeServerPlugin,
    RenetServerPlugin,
};

use crate::{
    message::{
        self,
        client::ReliableMessageFromClient,
        server::{MessageReader, ReliableMessageFromServer},
        spawn::NetworkSpawn,
    },
    shared::{
        self,
        objects::{Ball, NetworkObject},
    },
};

pub fn run() {
    App::new()
        .add_plugins((DefaultPlugins, Server))
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_server_events, sync_game))
        .add_plugins((shared::Game, message::server::ServerMessagePlugin))
        .insert_state(shared::AppState::InGame)
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

fn sync_game(
    mut server: ResMut<RenetServer>,
    reader: Res<MessageReader>,
    ball_query: Query<(&NetworkObject, &Transform), With<Ball>>,
) {
    for (client_id, msg) in reader.reliable_messages() {
        if *msg == ReliableMessageFromClient::Ready {
            for (net_obj, transform) in ball_query.iter() {
                let spawn = NetworkSpawn::Ball(transform.clone());
                let message = ReliableMessageFromServer::Spawn(net_obj.clone(), spawn);
                let bytes = bincode::serialize(&message).unwrap();
                server.send_message(*client_id, DefaultChannel::ReliableUnordered, bytes);
            }
        }
    }
}
