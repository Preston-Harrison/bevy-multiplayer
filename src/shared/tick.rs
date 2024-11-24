use std::time::Duration;

use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use serde::{Deserialize, Serialize};

use crate::message::{
    client::MessageReaderOnClient,
    server::{ReliableMessageFromServer, TickSync},
};

use super::GameLogic;

#[derive(Resource, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(u64);

#[derive(Resource)]
pub struct TickBroadcastTimer(Timer);

impl Default for TickBroadcastTimer {
    fn default() -> Self {
        Self(Timer::new(Duration::from_secs(10), TimerMode::Repeating))
    }
}

impl Tick {
    pub fn new(tick: u64) -> Self {
        Tick(tick)
    }

    pub fn get(&self) -> u64 {
        self.0
    }
}

pub struct TickPlugin {
    pub is_server: bool,
}

impl Plugin for TickPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (tick.in_set(GameLogic::Start),));
        if self.is_server {
            app.insert_resource(Tick::new(0));
            app.insert_resource(TickBroadcastTimer::default());
            app.add_systems(
                FixedUpdate,
                send_tick_update.in_set(GameLogic::Start).after(tick),
            );
        } else {
            app.add_systems(
                FixedUpdate,
                recv_tick_update.in_set(GameLogic::Start).after(tick),
            );
        }
    }
}

fn tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

fn recv_tick_update(reader: Res<MessageReaderOnClient>, mut curr_tick: ResMut<Tick>) {
    for msg in reader.reliable_messages() {
        if let ReliableMessageFromServer::TickSync(sync) = msg {
            // TODO: figure out why this is out of sync.
            let next_tick = get_client_tick(sync.tick, sync.unix_millis);
            info!("tick updated from {:?} to {:?}", *curr_tick, next_tick);
            *curr_tick = next_tick;
        }
    }
}

fn send_tick_update(
    mut timer: ResMut<TickBroadcastTimer>,
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    tick: Res<Tick>,
) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        let message = ReliableMessageFromServer::TickSync(TickSync {
            tick: tick.get(),
            unix_millis: get_unix_millis(),
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
        info!("tick sync sent");
    }
}

/// Returns the current Unix timestamp in milliseconds. Uses system time.
pub fn get_unix_millis() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time is before Unix epoch")
        .as_millis()
}

/// Returns the tick on the client given a tick on the server and the time that
/// tick was sent. Uses system time.
///
/// # Arguments
/// * `server_tick` - The tick on the server when the message was sent.
/// * `server_unix_millis` - The Unix timestamp (in milliseconds) when the message was sent from the server.
///
/// # Returns
/// The estimated current tick on the client based on the server tick and the elapsed time.
pub fn get_client_tick(server_tick: u64, server_unix_millis: u128) -> Tick {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Get the current system time in milliseconds
    let client_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before Unix epoch")
        .as_millis();

    // Compute the elapsed time in milliseconds since the server tick
    let elapsed_millis = client_unix_millis.saturating_sub(server_unix_millis);

    // TODO: paramaterise this
    // Assume 60 ticks per second (16.67 milliseconds per tick)
    const MILLIS_PER_TICK: u128 = 1000 / 60;

    // Compute the number of ticks that have passed since the server tick
    let elapsed_ticks = elapsed_millis / MILLIS_PER_TICK;

    // Return the estimated client tick
    Tick::new(server_tick + elapsed_ticks as u64)
}
