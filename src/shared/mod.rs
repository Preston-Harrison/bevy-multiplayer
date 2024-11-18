use std::time::Duration;

use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use rand::Rng;

use crate::message::{client::MessageReader, server::ReliableMessageFromServer};

use self::{
    cond::{run_if_is_client, run_if_is_server},
    objects::{Ball, BallPlugin, NetworkObject},
};

pub mod cond;
pub mod objects;
pub mod render;
pub mod scenes;

pub const SERVER_ADDR: &str = "127.0.0.1:5000";

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    MainMenu,
    InGame,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameLogic {
    Read,
    Spawn,
    Sync,
    Game,
    Clear,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientOnly;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerOnly;

fn despawn(
    reader: Res<MessageReader>,
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
            }
        }
    }
}

pub struct Game;

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, despawn.in_set(ClientOnly));
        app.add_plugins(BallPlugin);
        app.insert_resource(RandomBallTimer(Timer::new(
            Duration::from_secs(10),
            TimerMode::Repeating,
        )));
        app.add_systems(
            Update,
            spawn_random_balls
                .in_set(ServerOnly)
                .in_set(GameLogic::Game),
        );
        app.configure_sets(
            Update,
            (
                (
                    GameLogic::Read,
                    GameLogic::Spawn,
                    GameLogic::Sync,
                    GameLogic::Game,
                    GameLogic::Clear,
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
        for (entity, obj, _, _) in balls.iter() {
            let message = ReliableMessageFromServer::Despawn(obj.clone());
            let bytes = bincode::serialize(&message).unwrap();
            server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
            commands.entity(entity).despawn();
        }
        println!("spawning random balls");
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
