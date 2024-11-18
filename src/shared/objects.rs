use bevy::prelude::*;
use serde::{Serialize, Deserialize};
use crate::message::spawn::{CanNetworkSpawn, NetworkSpawn};

#[derive(Component)]
pub struct Player;

impl CanNetworkSpawn for Player {
    fn network_spawn(&self) -> NetworkSpawn {
        return NetworkSpawn::Player;
    }
}

#[derive(Serialize, Deserialize, Component, Clone, Debug)]
pub struct NetworkObject {
    pub id: u64,
    pub authority: Authority,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Authority {
    Server,
    Client(u64),
}
