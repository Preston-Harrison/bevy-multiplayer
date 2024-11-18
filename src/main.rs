#![allow(irrefutable_let_patterns)]
/// Allowed as most enums will have more stuff in the future.
use std::env;

mod client;
mod message;
mod server;
mod shared;

fn main() {
    let mut args = env::args();
    args.next();
    match args.next().as_ref().map(|v| v.as_str()) {
        Some("server") => server::run(),
        Some("client") => client::run(),
        Some(v) => panic!("{v} is not a valid run mode"),
        None => panic!("no run mode passed"),
    }
}
