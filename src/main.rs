#![allow(irrefutable_let_patterns)]
/// Allowed as most enums will have more stuff in the future.
use std::env;

mod client;
mod ik;
mod message;
mod proc;
mod server;
mod shared;
mod test;
mod ui;
mod utils;

fn main() {
    let mut args = env::args();
    args.next();
    match args.next().as_ref().map(|v| v.as_str()) {
        Some("server") => server::run(),
        Some("client") => client::run(),
        Some("proc") => proc::run(),
        Some("test") => test::run(),
        Some("ik") => ik::run(),
        Some(v) => panic!("{v} is not a valid run mode"),
        None => panic!("no run mode passed"),
    }
}
