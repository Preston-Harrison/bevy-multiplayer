use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkSpawn {
    Player(Transform),
    Ball(Transform),
}
