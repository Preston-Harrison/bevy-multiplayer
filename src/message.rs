pub mod client {
    use crate::shared::GameLogic;

    use super::server::ReliableMessageFromServer;
    use bevy::prelude::*;
    use bevy_renet::renet::{RenetClient, DefaultChannel};

    #[derive(Resource)]
    pub struct MessageReader {
        messages: Vec<ReliableMessageFromServer>,
    }

    impl MessageReader {
        pub fn new() -> Self {
            Self {
                messages: Vec::new(),
            }
        }

        pub fn messages(&self) -> &[ReliableMessageFromServer] {
            self.messages.as_slice()
        }

        fn push_message(&mut self, message: ReliableMessageFromServer) {
            self.messages.push(message);
        }

        fn clear_messages(&mut self) {
            self.messages.clear();
        }
    }

    pub struct ClientMessagePlugin;

    impl Plugin for ClientMessagePlugin {
        fn build(&self, app: &mut App) {
            app.insert_resource(MessageReader::new())
                .add_systems(Update, read_messages_from_server.in_set(GameLogic::Read))
                .add_systems(Update, clear_messages.after(GameLogic::Clear));
        }
    }

    fn read_messages_from_server(
        mut client: ResMut<RenetClient>,
        mut message_reader: ResMut<MessageReader>,
    ) {
        while let Some(message) = client.receive_message(DefaultChannel::ReliableUnordered) {
            // Deserialize the message into ReliableMessageFromServer
            if let Ok(parsed_message) = bincode::deserialize::<ReliableMessageFromServer>(&message) {
                println!("recv message {:?}", parsed_message);
                message_reader.push_message(parsed_message);
            } else {
                error!("Failed to deserialize message from server");
            }
        }
    }

    fn clear_messages(mut message_reader: ResMut<MessageReader>) {
        message_reader.clear_messages();
    }
}

pub mod server {
    use serde::{Deserialize, Serialize};

    use crate::shared::objects::NetworkObject;

    use super::spawn::NetworkSpawn;

    #[derive(Serialize, Deserialize, Debug)]
    pub enum ReliableMessageFromServer {
        Spawn(NetworkObject, NetworkSpawn),
        Despawn(NetworkObject),
    }
}

pub mod spawn {
    use bevy::prelude::*;
    use serde::{Deserialize, Serialize};

    pub trait CanNetworkSpawn {
        fn add_send_spawn_system(app: &mut App);
        fn add_recv_spawn_system(app: &mut App);
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum NetworkSpawn {
        Player,
        Ball(Transform),
    }
}
