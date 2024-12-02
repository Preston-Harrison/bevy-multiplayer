use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

use super::tick::Tick;

pub mod gizmo;
pub mod grounded;
pub mod gun;
pub mod health;
pub mod player;
pub mod tracer;
pub mod worm;

#[derive(Serialize, Deserialize, Component, Clone, Debug, Hash, PartialEq, Eq)]
pub enum NetworkObject {
    Dynamic(u64),
    /// Static should be used for network objects that don't depend on the server
    /// for assignment, i.e. they are generated deterministically.
    Static(u64),
}

impl NetworkObject {
    pub fn new_rand() -> Self {
        let mut rng = rand::thread_rng();
        let random_number: u64 = rng.gen();
        Self::Dynamic(random_number)
    }

    pub fn new_static(id: u64) -> Self {
        Self::Static(id)
    }
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

    pub fn should_update(&mut self, tick: Tick) -> bool {
        let should_update = self.last_tick < tick;
        if should_update {
            self.last_tick = tick;
        };
        return should_update;
    }
}
