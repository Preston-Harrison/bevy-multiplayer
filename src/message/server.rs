use bevy::prelude::*;
use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};
use serde::{Deserialize, Serialize};

use crate::shared::{
    objects::{player::Shot, NetworkObject},
    tick::Tick,
    GameLogic,
};

use super::{
    client::{ReliableMessageFromClient, UnreliableMessageFromClient},
    spawn::NetworkSpawn,
    MessageSet, MessagesAvailable,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct TickSync {
    pub tick: u64,
    pub unix_millis: u128,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Spawn {
    pub net_obj: NetworkObject,
    pub net_spawn: NetworkSpawn,
    pub tick: Tick,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlayerInit {
    pub net_obj: NetworkObject,
    pub transform: Transform,
    pub tick: Tick,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ReliableMessageFromServer {
    Spawn(Spawn),
    Despawn(NetworkObject),
    InitPlayer(PlayerInit),
    TickSync(TickSync),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlayerPositionSync {
    pub net_obj: NetworkObject,
    pub translation: Vec3,
    pub tick: Tick,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OwnedPlayerSync {
    pub net_obj: NetworkObject,
    pub translation: Vec3,
    pub jump_velocity: Vec3,
    pub tick: Tick,
    pub last_input_order: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum UnreliableMessageFromServer {
    TransformSync(NetworkObject, Transform, Tick),
    PlayerPositionSync(PlayerPositionSync),
    /// Sent only to the owner of the player. Used for reconciliation.
    OwnedPlayerSync(OwnedPlayerSync),
    /// PlayerShot has structure of (Shooter, Shot)
    PlayerShot(NetworkObject, Shot),
}

#[derive(Resource)]
pub struct MessageReaderOnServer {
    reliable_messages: Vec<(ClientId, ReliableMessageFromClient)>,
    unreliable_messages: Vec<(ClientId, UnreliableMessageFromClient)>,
}

impl MessageReaderOnServer {
    pub fn new() -> Self {
        Self {
            reliable_messages: Vec::new(),
            unreliable_messages: Vec::new(),
        }
    }

    pub fn reliable_messages(&self) -> &[(ClientId, ReliableMessageFromClient)] {
        self.reliable_messages.as_slice()
    }

    pub fn unreliable_messages(&self) -> &[(ClientId, UnreliableMessageFromClient)] {
        self.unreliable_messages.as_slice()
    }
}

pub struct ServerMessagePlugin;

impl Plugin for ServerMessagePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MessageReaderOnServer::new())
            .add_systems(
                FixedUpdate,
                read_messages_from_clients.in_set(MessageSet::Read),
            )
            .add_systems(FixedUpdate, clear_messages.after(MessageSet::Clear));
        app.configure_sets(
            FixedUpdate,
            (
                MessageSet::Read.before(GameLogic::Start),
                MessageSet::Clear.after(GameLogic::End),
                MessagesAvailable
                    .after(MessageSet::Read)
                    .before(MessageSet::Clear),
            ),
        );
    }
}

fn read_messages_from_clients(
    mut server: ResMut<RenetServer>,
    mut message_reader: ResMut<MessageReaderOnServer>,
) {
    for client_id in server.clients_id() {
        while let Some(message) =
            server.receive_message(client_id, DefaultChannel::ReliableUnordered)
        {
            if let Ok(parsed_message) = bincode::deserialize::<ReliableMessageFromClient>(&message)
            {
                message_reader
                    .reliable_messages
                    .push((client_id, parsed_message));
            } else {
                error!("Failed to deserialize message from server");
            }
        }

        while let Some(message) = server.receive_message(client_id, DefaultChannel::Unreliable) {
            if let Ok(parsed_message) =
                bincode::deserialize::<UnreliableMessageFromClient>(&message)
            {
                message_reader
                    .unreliable_messages
                    .push((client_id, parsed_message));
            } else {
                error!("Failed to deserialize message from server");
            }
        }
    }
}

fn clear_messages(mut message_reader: ResMut<MessageReaderOnServer>) {
    if message_reader.reliable_messages.len() > 0 {
        dbg!(&message_reader.reliable_messages);
    }
    message_reader.reliable_messages.clear();
    message_reader.unreliable_messages.clear();
}
