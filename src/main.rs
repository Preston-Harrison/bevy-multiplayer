use std::env;

mod client;
mod server;
mod shared;
mod message;

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
