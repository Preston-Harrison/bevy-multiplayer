use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};

use crate::{
    message::{
        client::MessageReaderOnClient,
        server::{HealthSync, ReliableMessageFromServer},
    },
    shared::{tick::Tick, GameLogic},
};

use super::{LastSyncTracker, NetworkObject};

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
}

pub struct HealthPlugin {
    pub is_server: bool,
}

impl Plugin for HealthPlugin {
    fn build(&self, app: &mut App) {
        if self.is_server {
            app.add_systems(FixedUpdate, send_health.in_set(GameLogic::Sync));
        } else {
            app.add_systems(FixedUpdate, sync_health.in_set(GameLogic::Sync));
        }
    }
}

fn sync_health(
    reader: Res<MessageReaderOnClient>,
    mut query: Query<(&NetworkObject, &mut Health, &mut LastSyncTracker<Health>)>,
) {
    for msg in reader.reliable_messages() {
        if let ReliableMessageFromServer::HealthSync(sync) = msg {
            for (obj, mut health, mut tracker) in query.iter_mut() {
                if *obj == sync.net_obj {
                    if tracker.should_update(sync.tick) {
                        health.current = sync.health;
                    }
                }
            }
        }
    }
}

fn send_health(
    mut server: ResMut<RenetServer>,
    query: Query<(&NetworkObject, &Health)>,
    tick: Res<Tick>,
) {
    for (net_obj, health) in query.iter() {
        let message = ReliableMessageFromServer::HealthSync(HealthSync {
            net_obj: net_obj.clone(),
            tick: *tick,
            health: health.current,
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
    }
}
