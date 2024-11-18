use crate::message::{
    client::MessageReader,
    server::ReliableMessageFromServer,
    spawn::{CanNetworkSpawn, NetworkSpawn},
};
use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{ClientOnly, GameLogic, ServerOnly};

// #[derive(Component)]
// pub struct Player;
//
// impl CanNetworkSpawn for Player {
//     fn network_spawn(&self) -> NetworkSpawn {
//         return NetworkSpawn::Player;
//     }
//
//     fn add_spawn_system(app: &mut App) {
//         app.add_systems(Update, spawn_players.run_if(run_if_is_client));
//     }
// }
//
// fn spawn_players(mut commands: Commands, reader: Res<MessageReader>) {
//     for msg in reader.messages() {
//         let ReliableMessageFromServer::Spawn(network_obj, network_spawn) = msg else {
//             continue;
//         };
//         if let NetworkSpawn::Player = network_spawn {
//             commands.spawn(Player).insert(network_obj.clone());
//         }
//     }
// }

#[derive(Serialize, Deserialize, Component, Clone, Debug)]
pub struct NetworkObject {
    pub id: u64,
    pub authority: Authority,
}

impl NetworkObject {
    pub fn rand() -> Self {
        let mut rng = rand::thread_rng();
        let random_number: u64 = rng.gen();
        Self {
            id: random_number,
            authority: Authority::Server,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Authority {
    Server,
    Client(u64),
}

#[derive(Component)]
pub struct Ball;

impl CanNetworkSpawn for Ball {
    fn add_send_spawn_system(app: &mut App) {
        app.add_systems(Update, broadcast_ball_spawns.in_set(ServerOnly));
    }

    fn add_recv_spawn_system(app: &mut App) {
        app.add_systems(Update, spawn_balls.in_set(ClientOnly).in_set(GameLogic::Spawn));
    }
}

fn broadcast_ball_spawns(
    query: Query<(&NetworkObject, &Transform), Added<Ball>>,
    mut server: ResMut<RenetServer>,
) {
    for (network_obj, transform) in query.iter() {
        if network_obj.authority == Authority::Server {
            let network_spawn = NetworkSpawn::Ball(transform.clone());
            let message = ReliableMessageFromServer::Spawn(network_obj.clone(), network_spawn);
            let bytes = bincode::serialize(&message).unwrap();
            server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
        }
    }
}

fn spawn_balls(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    reader: Res<MessageReader>,
) {
    for msg in reader.messages() {
        let ReliableMessageFromServer::Spawn(network_obj, network_spawn) = msg else {
            continue;
        };
        if let NetworkSpawn::Ball(transform) = network_spawn {
            println!("random ball spawning");
            commands
                .spawn(Ball)
                .insert(PbrBundle {
                    mesh: meshes.add(Sphere::default().mesh().ico(5).unwrap()),
                    material: materials.add(Color::srgb(0.0, 0.0, 1.0)),
                    transform: *transform,
                    ..Default::default()
                })
                .insert(network_obj.clone());
        }
    }
}
