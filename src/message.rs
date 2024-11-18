pub mod client {
    use bevy::prelude::*;
    use super::server::ReliableMessageFromServer;

    #[derive(Resource)]
    pub struct MessageReader;

    impl MessageReader {
        pub fn messages(&self) -> &[ReliableMessageFromServer] {
            todo!()
        }
    }
}

pub mod server {
    use serde::{Deserialize, Serialize};

    use crate::shared::objects::NetworkObject;

    use super::spawn::NetworkSpawn;

    #[derive(Serialize, Deserialize)]
    pub enum ReliableMessageFromServer {
        Spawn(NetworkObject, NetworkSpawn),
        Despawn(NetworkObject),
    }
}

pub mod spawn {
    use bevy::prelude::*;
    use serde::{Serialize, Deserialize};

    use crate::shared;

    pub trait CanNetworkSpawn {
        fn network_spawn(&self) -> NetworkSpawn;
    }

    #[derive(Serialize, Deserialize)]
    pub enum NetworkSpawn {
        Player,
    }

    impl NetworkSpawn {
        pub fn get_bundle(&self) -> impl Bundle {
            match self {
                Self::Player => shared::objects::Player,
            }
        }
    }
}
