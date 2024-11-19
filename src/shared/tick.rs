use bevy::prelude::*;

use super::GameLogic;

#[derive(Resource)]
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
        app.add_systems(FixedUpdate, tick.in_set(GameLogic::Start));
        if self.is_server {
            app.insert_resource(Tick::new(0));
        }
    }
}

fn tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}
