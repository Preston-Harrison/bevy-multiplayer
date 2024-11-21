use bevy::prelude::*;

use crate::shared::GameLogic;

pub struct GroundedPlugin;

impl Plugin for GroundedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, set_grounded_last_frame.in_set(GameLogic::Start));
    }
}

/// Grounded has a field that checks if it was grounded last tick. This tick is
/// called in GameLogic::End, but also can be called manually. This is useful
/// for reconciliation.
#[derive(Component, Default)]
pub struct Grounded {
    pub is_grounded: bool,
    grounded_last_tick: bool,
}

impl Grounded {
    pub fn tick(&mut self) {
        self.grounded_last_tick = self.is_grounded;
    }

    pub fn was_grounded_last_tick(&self) -> bool {
        self.grounded_last_tick
    }
}

fn set_grounded_last_frame(mut query: Query<&mut Grounded>) {
    for mut grounded in query.iter_mut() {
        info!("starting tick, grounded = {}, was_grounded = {}", grounded.is_grounded, grounded.grounded_last_tick);
        grounded.tick();
    }
}

pub fn set_grounded(grounded: &mut Option<Mut<Grounded>>, is_grounded: bool) {
    if let Some(ref mut grounded) = grounded {
        grounded.is_grounded = is_grounded;
    }
}
