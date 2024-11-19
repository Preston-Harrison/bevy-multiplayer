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

    fn should_drop(message_loss: Option<f64>) -> bool {
        message_loss.is_some_and(|v| rand::random::<f64>() < v)
    }

    struct PendingMessage<T> {
        /// Time in seconds when the message should be delivered
        delivery_time: f64,
        message: T,
    }

    #[derive(Resource, Default)]
    pub struct MessageReaderOnClient {
        latency: Option<f64>,
        message_loss: Option<f64>,
        pending_reliable_messages: Vec<PendingMessage<ReliableMessageFromServer>>,
        pending_unreliable_messages: Vec<PendingMessage<UnreliableMessageFromServer>>,

        reliable_messages: Vec<ReliableMessageFromServer>,
        unreliable_messages: Vec<UnreliableMessageFromServer>,
    }

    impl MessageReaderOnClient {
        pub fn new(latency: Option<f64>, message_loss: Option<f64>) -> Self {
            Self {
                latency,
                message_loss,
                ..Default::default()
            }
        }

        pub fn reliable_messages(&self) -> &[ReliableMessageFromServer] {
            self.reliable_messages.as_slice()
        }

        pub fn unreliable_messages(&self) -> &[UnreliableMessageFromServer] {
            self.unreliable_messages.as_slice()
        }
    }

    pub struct ClientMessagePlugin {
        pub latency: Option<f64>,
        pub message_loss: Option<f64>,
    }

    impl Plugin for ClientMessagePlugin {
        fn build(&self, app: &mut App) {
            app.insert_resource(MessageReaderOnClient::new(self.latency, self.message_loss))
                .add_systems(
                    FixedUpdate,
                    read_messages_from_server.in_set(MessageSet::Read),
                )
                .add_systems(FixedUpdate, clear_messages.in_set(MessageSet::Clear));
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

    fn read_messages_from_server(
        client: Option<ResMut<RenetClient>>,
        mut message_reader: ResMut<MessageReaderOnClient>,
        time: Res<Time>,
    ) {
        let Some(mut client) = client else {
            return;
        };
        let current_time = time.elapsed_seconds_f64();

        for i in (0..message_reader.pending_reliable_messages.len()).rev() {
            if message_reader.pending_reliable_messages[i].delivery_time <= current_time {
                let pending_message = message_reader.pending_reliable_messages.swap_remove(i);
                message_reader
                    .reliable_messages
                    .push(pending_message.message);
            }
        }

        // Process unreliable messages
        for i in (0..message_reader.pending_unreliable_messages.len()).rev() {
            if message_reader.pending_unreliable_messages[i].delivery_time <= current_time {
                let pending_message = message_reader.pending_unreliable_messages.swap_remove(i);
                message_reader
                    .unreliable_messages
                    .push(pending_message.message);
            }
        }

        while let Some(message) = client.receive_message(DefaultChannel::ReliableUnordered) {
            if let Ok(parsed_message) = bincode::deserialize::<ReliableMessageFromServer>(&message)
            {
                match message_reader.latency {
                    Some(latency) => {
                        let delivery_time = current_time + latency;
                        message_reader
                            .pending_reliable_messages
                            .push(PendingMessage {
                                delivery_time,
                                message: parsed_message,
                            });
                    }
                    None => {
                        message_reader.reliable_messages.push(parsed_message);
                    }
                };
            } else {
                error!("Failed to deserialize message from server");
            }
        }

        while let Some(message) = client.receive_message(DefaultChannel::Unreliable) {
            if should_drop(message_reader.message_loss) {
                continue;
            }
            if let Ok(parsed_message) =
                bincode::deserialize::<UnreliableMessageFromServer>(&message)
            {
                match message_reader.latency {
                    Some(latency) => {
                        let delivery_time = current_time + latency;
                        message_reader
                            .pending_unreliable_messages
                            .push(PendingMessage {
                                delivery_time,
                                message: parsed_message,
                            });
                    }
                    None => {
                        message_reader.unreliable_messages.push(parsed_message);
                    }
                }
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

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct OrderedInput {
        pub input: player::Input,
        pub order: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum UnreliableMessageFromClient {
        Input(OrderedInput),
    }
}

pub mod server;

pub mod spawn {
    use bevy::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    pub enum NetworkSpawn {
        Player(Transform),
        Ball(Transform),
    }
}
