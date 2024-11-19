use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::{client::MessageReaderOnClient, server::ReliableMessageFromServer};

use super::{ClientOnly, GameLogic};

#[derive(Resource, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(u64);

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
        app.add_systems(
            FixedUpdate,
            (
                tick.in_set(GameLogic::Start),
                recv_tick_update
                    .in_set(ClientOnly)
                    .in_set(GameLogic::Start)
                    .after(tick),
            ),
        );
        if self.is_server {
            app.insert_resource(Tick::new(0));
        }
    }
}

fn tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

fn recv_tick_update(reader: Res<MessageReaderOnClient>, mut curr_tick: ResMut<Tick>) {
    for msg in reader.reliable_messages() {
        if let ReliableMessageFromServer::TickSync(sync) = msg {
            *curr_tick = get_client_tick(sync.tick, sync.unix_millis);
        }
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
