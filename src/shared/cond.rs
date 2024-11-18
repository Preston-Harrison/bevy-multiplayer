use bevy::prelude::*;
use bevy_renet::renet::{RenetClient, RenetServer};

pub fn run_if_is_server(s: Option<Res<RenetServer>>) -> bool {
    s.is_some()
}

pub fn run_if_is_client(s: Option<Res<RenetClient>>) -> bool {
    s.is_some()
}
