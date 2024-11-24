use bevy::prelude::*;

use crate::shared::GameLogic;

pub struct GroundedPlugin;

impl Plugin for GroundedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            set_grounded_last_frame.in_set(GameLogic::Start),
        );
    }
}

/// Grounded has a field that checks if it was grounded last tick. This tick is
/// called in GameLogic::End, but also can be called manually. This is useful
/// for reconciliation.
#[derive(Component, Default)]
pub struct Grounded {
    is_grounded: bool,
    grounded_last_tick: bool,
    grounded_this_tick: bool,
}

impl Grounded {
    pub fn tick(&mut self) {
        self.grounded_last_tick = self.grounded_this_tick;
        self.grounded_this_tick = self.is_grounded;
    }

    pub fn was_grounded_last_tick(&self) -> bool {
        self.grounded_last_tick
    }

    pub fn is_grounded(&self) -> bool {
        self.is_grounded
    }

    pub fn set_is_grounded(&mut self, is_grounded: bool) {
        self.is_grounded = is_grounded;
        self.grounded_this_tick |= self.is_grounded;
    }

    pub fn grounded_this_tick(&self) -> bool {
        self.grounded_this_tick
    }
}

fn set_grounded_last_frame(mut query: Query<&mut Grounded>) {
    for mut grounded in query.iter_mut() {
        grounded.tick();
    }
}

pub fn set_grounded(mut grounded: Option<&mut Grounded>, is_grounded: bool) {
    if let Some(ref mut grounded) = grounded {
        grounded.set_is_grounded(is_grounded)
    }
}
