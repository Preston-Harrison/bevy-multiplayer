use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageSet {
    Read,
    Clear,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessagesAvailable;

pub mod client {
    use crate::shared::{objects::player, GameLogic};

    use super::{
        server::{ReliableMessageFromServer, UnreliableMessageFromServer},
        MessageSet, MessagesAvailable,
    };
    use bevy::prelude::*;
    use bevy_renet::renet::{DefaultChannel, RenetClient};
    use serde::{Deserialize, Serialize};

    #[derive(Resource)]
    pub struct MessageReaderOnClient {
        reliable_messages: Vec<ReliableMessageFromServer>,
        unreliable_messages: Vec<UnreliableMessageFromServer>,
    }

    impl MessageReaderOnClient {
        pub fn new() -> Self {
            Self {
                reliable_messages: Vec::new(),
                unreliable_messages: Vec::new(),
            }
        }

        pub fn reliable_messages(&self) -> &[ReliableMessageFromServer] {
            self.reliable_messages.as_slice()
        }

        pub fn unreliable_messages(&self) -> &[UnreliableMessageFromServer] {
            self.unreliable_messages.as_slice()
        }
    }

    pub struct ClientMessagePlugin;

    impl Plugin for ClientMessagePlugin {
        fn build(&self, app: &mut App) {
            app.insert_resource(MessageReaderOnClient::new())
                .add_systems(Update, read_messages_from_server.in_set(MessageSet::Read))
                .add_systems(Update, clear_messages.in_set(MessageSet::Clear));
            app.configure_sets(
                Update,
                (
                    MessageSet::Read.before(GameLogic::Read),
                    MessageSet::Clear.after(GameLogic::Clear),
                    MessagesAvailable
                        .after(MessageSet::Read)
                        .before(MessageSet::Clear),
                ),
            );
        }
    }

    fn read_messages_from_server(
        client: Option<ResMut<RenetClient>>,
        mut message_reader: ResMut<MessageReaderOnClient>,
    ) {
        let Some(mut client) = client else {
            return;
        };
        while let Some(message) = client.receive_message(DefaultChannel::ReliableUnordered) {
            if let Ok(parsed_message) = bincode::deserialize::<ReliableMessageFromServer>(&message)
            {
                message_reader.reliable_messages.push(parsed_message);
            } else {
                error!("Failed to deserialize message from server");
            }
        }

        while let Some(message) = client.receive_message(DefaultChannel::Unreliable) {
            if let Ok(parsed_message) =
                bincode::deserialize::<UnreliableMessageFromServer>(&message)
            {
                message_reader.unreliable_messages.push(parsed_message);
            } else {
                error!("Failed to deserialize message from server");
            }
        }
    }

    fn clear_messages(mut message_reader: ResMut<MessageReaderOnClient>) {
        message_reader.reliable_messages.clear();
        message_reader.unreliable_messages.clear();
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    pub enum ReliableMessageFromClient {
        Connected,
        ReadyForUpdates,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum UnreliableMessageFromClient {
        Input(player::Input),
    }
}

pub mod server {
    use bevy::prelude::*;
    use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};
    use serde::{Deserialize, Serialize};

    use crate::shared::{objects::NetworkObject, GameLogic};

    use super::{
        client::{ReliableMessageFromClient, UnreliableMessageFromClient},
        spawn::NetworkSpawn,
        MessageSet, MessagesAvailable,
    };

    #[derive(Serialize, Deserialize, Debug)]
    pub enum ReliableMessageFromServer {
        Spawn(NetworkObject, NetworkSpawn),
        Despawn(NetworkObject),
        SetPlayerNetworkObject(NetworkObject),
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum UnreliableMessageFromServer {
        TransformSync(NetworkObject, Transform),
        PositionSync(NetworkObject, Vec3),
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
                .add_systems(Update, read_messages_from_clients.in_set(MessageSet::Read))
                .add_systems(Update, clear_messages.after(MessageSet::Clear));
            app.configure_sets(
                Update,
                (
                    MessageSet::Read.before(GameLogic::Read),
                    MessageSet::Clear.after(GameLogic::Clear),
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
                if let Ok(parsed_message) =
                    bincode::deserialize::<ReliableMessageFromClient>(&message)
                {
                    message_reader
                        .reliable_messages
                        .push((client_id, parsed_message));
                } else {
                    error!("Failed to deserialize message from server");
                }
            }

            while let Some(message) = server.receive_message(client_id, DefaultChannel::Unreliable)
            {
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
}

pub mod spawn {
    use bevy::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    pub enum NetworkSpawn {
        Player(Transform),
        Ball(Transform),
    }
}
