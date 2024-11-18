use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetClient, RenetServer};

use crate::message::{client, server::ReliableMessageFromServer, spawn::CanNetworkSpawn};

use self::objects::{Authority, NetworkObject};

pub mod objects;

pub const SERVER_ADDR: &str = "127.0.0.1:5000";

fn run_if_is_server(s: Option<Res<RenetServer>>) -> bool {
    s.is_some()
}

fn run_if_is_client(s: Option<Res<RenetClient>>) -> bool {
    s.is_some()
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct GameLogicSet;

pub struct MultiplayerSpawner<C> {
    _component: PhantomData<C>,
}

impl<C: Component + CanNetworkSpawn> MultiplayerSpawner<C> {
    fn sync_to_clients(
        query: Query<(&NetworkObject, &C), Added<C>>,
        mut server: ResMut<RenetServer>,
    ) {
        for (network_obj, component) in query.iter() {
            if network_obj.authority == Authority::Server {
                let network_spawn = component.network_spawn();
                let message = ReliableMessageFromServer::Spawn(network_obj.clone(), network_spawn);
                let bytes = bincode::serialize(&message).unwrap();
                server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
            }
        }
    }

    fn handle_spawns_from_server(
        reader: Res<client::MessageReader>,
        mut commands: Commands,
        query: Query<(&NetworkObject, Entity)>,
    ) {
        for msg in reader.messages() {
            match msg {
                ReliableMessageFromServer::Spawn(network_obj, network_spawn) => {
                    commands
                        .spawn(network_spawn.get_bundle())
                        .insert(network_obj.clone());
                }
                ReliableMessageFromServer::Despawn(network_obj) => {
                    for (obj, entity) in query.iter() {
                        if obj.id == network_obj.id {
                            commands.entity(entity).despawn();
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl <C: Component + CanNetworkSpawn>Plugin for MultiplayerSpawner<C> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::sync_to_clients.run_if(run_if_is_server));
        app.add_systems(Update, Self::handle_spawns_from_server.run_if(run_if_is_client));
    }
}
