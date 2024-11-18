use std::{marker::PhantomData, time::Duration};

use bevy::prelude::*;
use rand::Rng;

use crate::message::{
    client::MessageReader, server::ReliableMessageFromServer, spawn::CanNetworkSpawn,
};

use self::{
    cond::{run_if_is_client, run_if_is_server},
    objects::{Ball, NetworkObject},
};

pub mod cond;
pub mod objects;

pub const SERVER_ADDR: &str = "127.0.0.1:5000";

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameLogic {
    Read,
    Spawn,
    Game,
    Clear,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientOnly;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerOnly;

pub struct MultiplayerSpawner<C> {
    _component: PhantomData<C>,
}

impl<C> MultiplayerSpawner<C> {
    fn new() -> Self {
        MultiplayerSpawner {
            _component: PhantomData::default(),
        }
    }
}

impl<C: Component + CanNetworkSpawn> Plugin for MultiplayerSpawner<C> {
    fn build(&self, app: &mut App) {
        C::add_recv_spawn_system(app);
        C::add_send_spawn_system(app);
    }
}

fn despawn(
    reader: Res<MessageReader>,
    mut commands: Commands,
    query: Query<(Entity, &NetworkObject)>,
) {
    for msg in reader.messages() {
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
        app.add_plugins(MultiplayerSpawner::<Ball>::new());
        app.insert_resource(RandomBallTimer(Timer::new(Duration::from_secs(10), TimerMode::Repeating)));
        app.add_systems(Update, spawn_random_balls.in_set(ServerOnly));
        app.configure_sets(
            Update,
            (
                (
                    GameLogic::Read,
                    GameLogic::Spawn,
                    GameLogic::Game,
                    GameLogic::Clear,
                )
                    .chain(),
                ClientOnly.run_if(run_if_is_client),
                ServerOnly.run_if(run_if_is_server),
            ),
        );
    }
}

#[derive(Resource)]
struct RandomBallTimer(Timer);

fn spawn_random_balls(
    commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<RandomBallTimer>,
) {
    if timer.0.tick(time.delta()).finished() {
        random_balls(commands);
    }
}

fn random_balls(mut commands: Commands) {
    let mut rng = rand::thread_rng();

    for _ in 0..3 {
        let x = rng.gen_range(-100..100) as f32;
        let y = rng.gen_range(-100..100) as f32;
        let z = rng.gen_range(-100..100) as f32;
        commands
            .spawn(Ball)
            .insert(Transform::from_xyz(x, y, z))
            .insert(NetworkObject::rand());
    }
}
