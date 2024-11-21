use std::time::Duration;

use crate::{
    message::{
        client::MessageReaderOnClient,
        server::{ReliableMessageFromServer, Spawn, UnreliableMessageFromServer},
        spawn::NetworkSpawn,
    },
    server::PlayerWantsUpdates,
    shared::{despawn_recursive_and_broadcast, tick::Tick, GameLogic},
};
use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use rand::Rng;

use super::{LastSyncTracker, NetworkObject};

#[derive(Component)]
pub struct Ball;

pub struct BallPlugin {
    pub is_server: bool,
}

impl Plugin for BallPlugin {
    fn build(&self, app: &mut App) {
        if self.is_server {
            app.insert_resource(RandomBallTimer(Timer::new(
                Duration::from_secs(10),
                TimerMode::Repeating,
            )));
            app.add_systems(
                FixedUpdate,
                (
                    broadcast_ball_spawns.in_set(GameLogic::Spawn),
                    broadcast_ball_data.in_set(GameLogic::Sync),
                    spawn_random_balls.in_set(GameLogic::Game),
                    load_balls.in_set(GameLogic::Sync),
                ),
            );
        } else {
            app.add_systems(
                FixedUpdate,
                (
                    spawn_balls.in_set(GameLogic::Spawn),
                    recv_ball_data.in_set(GameLogic::Sync),
                ),
            );
        }
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
                .insert(LastSyncTracker::<Transform>::new(spawn.tick.clone()))
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

#[derive(Resource)]
struct RandomBallTimer(Timer);

#[derive(Component)]
struct MoveUp;

fn spawn_random_balls(
    mut balls: Query<(Entity, &NetworkObject, &mut Transform, Option<&MoveUp>), With<Ball>>,
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<RandomBallTimer>,
    mut server: ResMut<RenetServer>,
) {
    if timer.0.tick(time.delta()).finished() {
        for (entity, obj, _, _) in balls.iter() {
            despawn_recursive_and_broadcast(&mut server, &mut commands, entity, obj.clone());
        }
        random_balls(commands);
    } else {
        for (_, _, mut transform, move_up) in balls.iter_mut() {
            if move_up.is_some() {
                transform.translation.z += 10.0 * time.delta_seconds();
            } else {
                transform.translation.z -= 10.0 * time.delta_seconds();
            }
        }
    }
}

fn random_balls(mut commands: Commands) {
    let mut rng = rand::thread_rng();

    for _ in 0..20 {
        let x = rng.gen_range(-30..30) as f32;
        let y = rng.gen_range(-30..30) as f32;
        let z = rng.gen_range(-30..30) as f32;
        let mut e = commands.spawn(Ball);
        e.insert(Transform::from_xyz(x, y, z));
        e.insert(NetworkObject::rand());
        if rng.gen_range(0..=1) == 1 {
            e.insert(MoveUp);
        }
    }
}

fn load_balls(
    mut player_load: EventReader<PlayerWantsUpdates>,
    mut server: ResMut<RenetServer>,
    ball_query: Query<(&NetworkObject, &Transform), With<Ball>>,
    tick: Res<Tick>,
) {
    for load in player_load.read() {
        for (net_obj, transform) in ball_query.iter() {
            let net_spawn = NetworkSpawn::Ball(transform.clone());
            let message = ReliableMessageFromServer::Spawn(Spawn {
                net_obj: net_obj.clone(),
                tick: tick.clone(),
                net_spawn,
            });
            let bytes = bincode::serialize(&message).unwrap();
            server.send_message(load.client_id, DefaultChannel::ReliableUnordered, bytes);
        }
    }
}
