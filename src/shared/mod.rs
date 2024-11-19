use std::time::Duration;

use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use rand::Rng;

use crate::message::{client::MessageReaderOnClient, server::ReliableMessageFromServer};

use self::{
    cond::{run_if_is_client, run_if_is_server},
    objects::{player::PlayerPlugin, Ball, BallPlugin, NetworkObject},
};

pub mod cond;
pub mod objects;
pub mod render;
pub mod scenes;
pub mod tick;

pub const SERVER_ADDR: &str = "127.0.0.1:5000";

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    MainMenu,
    InGame,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameLogic {
    Start,
    /// Spawn and despawn.
    Spawn,
    Sync,
    Input,
    Game,
    End,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientOnly;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerOnly;

fn despawn(
    reader: Res<MessageReaderOnClient>,
    mut commands: Commands,
    query: Query<(Entity, &NetworkObject)>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Despawn(network_obj) = msg else {
            continue;
        };
        for (e, obj) in query.iter() {
            if obj.id == network_obj.id {
                commands.entity(e).despawn();
                break;
            }
        }
    }
}

pub struct Game;

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            despawn.in_set(ClientOnly).in_set(GameLogic::Spawn),
        );
        app.add_plugins((BallPlugin, PlayerPlugin));
        app.insert_resource(RandomBallTimer(Timer::new(
            Duration::from_secs(10),
            TimerMode::Repeating,
        )));
        app.add_systems(
            FixedUpdate,
            spawn_random_balls
                .in_set(ServerOnly)
                .in_set(GameLogic::Game),
        );
        app.configure_sets(
            FixedUpdate,
            (
                (
                    GameLogic::Start,
                    GameLogic::Spawn,
                    GameLogic::Sync,
                    GameLogic::Input,
                    GameLogic::Game,
                    GameLogic::End,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
                ClientOnly.run_if(run_if_is_client),
                ServerOnly.run_if(run_if_is_server),
            ),
        );
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
        let mut despawns = 0;
        for (entity, obj, _, _) in balls.iter() {
            despawns += 1;
            despawn_recursive_and_broadcast(&mut server, &mut commands, entity, obj.clone());
        }
        println!("spawning random balls, despawning {despawns}");
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

pub fn despawn_recursive_and_broadcast(
    server: &mut RenetServer,
    commands: &mut Commands,
    entity: Entity,
    net_obj: NetworkObject,
) {
    let message = ReliableMessageFromServer::Despawn(net_obj);
    let bytes = bincode::serialize(&message).unwrap();
    server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
    commands.entity(entity).despawn_recursive();
}
