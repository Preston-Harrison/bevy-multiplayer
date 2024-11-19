use std::marker::PhantomData;

use crate::message::{
    client::MessageReaderOnClient,
    server::{ReliableMessageFromServer, Spawn, UnreliableMessageFromServer},
    spawn::NetworkSpawn,
};
use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{tick::Tick, ClientOnly, GameLogic, ServerOnly};

pub mod player;

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

#[derive(Component)]
pub struct Ball;

pub struct BallPlugin;

impl Plugin for BallPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                broadcast_ball_spawns.in_set(ServerOnly),
                spawn_balls.in_set(ClientOnly).in_set(GameLogic::Spawn),
                broadcast_ball_data.in_set(ServerOnly),
                recv_ball_data.in_set(ClientOnly).in_set(GameLogic::Sync),
            ),
        );
    }
}

fn broadcast_ball_spawns(
    query: Query<(&NetworkObject, &Transform), Added<Ball>>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (network_obj, transform) in query.iter() {
        let network_spawn = NetworkSpawn::Ball(transform.clone());
        let spawn = Spawn {
            net_obj: network_obj.clone(),
            net_spawn: network_spawn,
            tick: tick.clone(),
        };
        let message = ReliableMessageFromServer::Spawn(spawn);
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
    }
}

fn spawn_balls(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    reader: Res<MessageReaderOnClient>,
    tick: Res<Tick>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Spawn(spawn) = msg else {
            continue;
        };
        if let NetworkSpawn::Ball(transform) = spawn.net_spawn {
            commands
                .spawn(Ball)
                .insert(PbrBundle {
                    mesh: meshes.add(Sphere::default().mesh().ico(5).unwrap()),
                    material: materials.add(Color::srgb(0.0, 0.0, 1.0)),
                    transform,
                    ..Default::default()
                })
                .insert(LastSyncTracker::<Transform>::new(tick.clone()))
                .insert(spawn.net_obj.clone());
        }
    }
}

fn broadcast_ball_data(
    query: Query<(&NetworkObject, &Transform), With<Ball>>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (obj, transform) in query.iter() {
        let message = UnreliableMessageFromServer::TransformSync(
            obj.clone(),
            transform.clone(),
            tick.clone(),
        );
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::Unreliable, bytes);
    }
}

fn recv_ball_data(
    reader: Res<MessageReaderOnClient>,
    mut query: Query<
        (
            &mut Transform,
            &NetworkObject,
            &mut LastSyncTracker<Transform>,
        ),
        With<Ball>,
    >,
) {
    for msg in reader.unreliable_messages() {
        let UnreliableMessageFromServer::TransformSync(net_obj, net_transform, sync_tick) = msg
        else {
            continue;
        };
        for (mut transform, obj, mut tracker) in query.iter_mut() {
            if obj.id == net_obj.id {
                if tracker.last_tick < *sync_tick {
                    *transform = *net_transform;
                    tracker.last_tick = sync_tick.clone();
                }
                break;
            }
        }
    }
}
