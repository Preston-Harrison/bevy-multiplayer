use std::marker::PhantomData;
use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::tick::Tick;

pub mod player;
pub mod ball;

#[derive(Serialize, Deserialize, Component, Clone, Debug, Hash, PartialEq, Eq)]
pub struct NetworkObject {
    pub id: u64,
}

#[derive(Component, Clone, Debug)]
pub struct LastSyncTracker<T> {
    _component: PhantomData<T>,
    pub last_tick: Tick,
}

impl<T> LastSyncTracker<T> {
    pub fn new(tick: Tick) -> Self {
        Self {
            last_tick: tick,
            _component: PhantomData::default(),
        }
    }
}

impl NetworkObject {
    pub fn rand() -> Self {
        let mut rng = rand::thread_rng();
        let random_number: u64 = rng.gen();
        Self { id: random_number }
    }
}

