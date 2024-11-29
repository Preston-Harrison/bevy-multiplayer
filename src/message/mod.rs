use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageSet {
    Read,
    Clear,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessagesAvailable;

pub mod client;
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
