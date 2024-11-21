use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

use super::tick::Tick;

pub mod ball;
pub mod gizmo;
pub mod player;

#[derive(Serialize, Deserialize, Component, Clone, Debug, Hash, PartialEq, Eq)]
pub enum NetworkObject {
    Dynamic(u64),
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
}
