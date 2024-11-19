use std::{net::UdpSocket, time::SystemTime};

use bevy::{prelude::*, utils::HashMap};
use bevy_renet::{
    renet::{
        transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
        ClientId, ConnectionConfig, DefaultChannel, RenetServer, ServerEvent,
    },
    transport::NetcodeServerPlugin,
    RenetServerPlugin,
};

use crate::{
    message::{
        self,
        client::ReliableMessageFromClient,
        server::{MessageReaderOnServer, ReliableMessageFromServer},
        spawn::NetworkSpawn,
    },
    shared::{
        self, despawn_recursive_and_broadcast,
        objects::{player::Player, Ball, NetworkObject},
        tick::{get_unix_millis, Tick},
        GameLogic,
    },
};

pub fn run() {
    App::new()
        .add_plugins((DefaultPlugins, Server))
        .add_systems(Startup, setup)
        .add_systems(
            FixedUpdate,
            (handle_server_events, handle_ready_game).in_set(GameLogic::Sync),
        )
        .add_plugins((
            shared::Game,
            message::server::ServerMessagePlugin,
            shared::tick::TickPlugin { is_server: true },
        ))
        .insert_state(shared::AppState::InGame)
        .run();
}

struct Server;

impl Plugin for Server {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenetServerPlugin);
        app.insert_resource(ClientNetworkObjectMap::default());

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

#[derive(Resource, Default)]
pub struct ClientNetworkObjectMap(pub HashMap<ClientId, NetworkObject>);

fn handle_server_events(
    mut server_events: EventReader<ServerEvent>,
    mut client_map: ResMut<ClientNetworkObjectMap>,
    query: Query<(Entity, &NetworkObject), With<Player>>,
    mut server: ResMut<RenetServer>,
    mut commands: Commands,
) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                println!("Client {} connected", client_id);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                println!("Client {} disconnected: {:?}", client_id, reason);
                if let Some(net_obj) = client_map.0.remove(client_id) {
                    for (entity, obj) in query.iter() {
                        if obj.id == net_obj.id {
                            despawn_recursive_and_broadcast(
                                &mut server,
                                &mut commands,
                                entity,
                                net_obj.clone(),
                            );
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn handle_ready_game(
    mut server: ResMut<RenetServer>,
    reader: Res<MessageReaderOnServer>,
    ball_query: Query<(&NetworkObject, &Transform), With<Ball>>,
    player_query: Query<(&NetworkObject, &Transform), With<Player>>,
    mut client_map: ResMut<ClientNetworkObjectMap>,
    mut commands: Commands,
    tick: Res<Tick>,
) {
    for (client_id, msg) in reader.reliable_messages() {
        if *msg == ReliableMessageFromClient::Connected {
            if client_map.0.contains_key(client_id) {
                println!("connected called twice");
                continue;
            }
            println!("sending player network object");
            let net_obj = NetworkObject::rand();
            let message = ReliableMessageFromServer::SetPlayerNetworkObject(net_obj.clone());
            let bytes = bincode::serialize(&message).unwrap();
            server.send_message(*client_id, DefaultChannel::ReliableUnordered, bytes);
            client_map.0.insert(*client_id, net_obj.clone());

            let message = ReliableMessageFromServer::Tick {
                tick: tick.get(),
                unix_millis: get_unix_millis(),
            };
            let bytes = bincode::serialize(&message).unwrap();
            server.send_message(*client_id, DefaultChannel::ReliableUnordered, bytes);
        }
        if *msg == ReliableMessageFromClient::ReadyForUpdates {
            let Some(net_obj) = client_map.0.get(client_id) else {
                println!("ready called twice");
                continue;
            };
            println!("spawning player and syncing game objects");
            commands.spawn((Player, Transform::from_xyz(0.0, 1.0, 0.0), net_obj.clone()));

            for (net_obj, transform) in ball_query.iter() {
                let spawn = NetworkSpawn::Ball(transform.clone());
                let message = ReliableMessageFromServer::Spawn(net_obj.clone(), spawn);
                let bytes = bincode::serialize(&message).unwrap();
                server.send_message(*client_id, DefaultChannel::ReliableUnordered, bytes);
            }

            // Won't include player just spawned.
            for (net_obj, transform) in player_query.iter() {
                let spawn = NetworkSpawn::Player(transform.clone());
                let message = ReliableMessageFromServer::Spawn(net_obj.clone(), spawn);
                let bytes = bincode::serialize(&message).unwrap();
                server.send_message(*client_id, DefaultChannel::ReliableUnordered, bytes);
            }
        }
    }
}
