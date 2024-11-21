use bevy::prelude::*;

#[derive(Component, Default)]
pub struct Grounded {
    pub is_grounded: bool,
}

pub fn set_grounded(grounded: &mut Option<Mut<Grounded>>, is_grounded: bool) {
    if let Some(ref mut grounded) = grounded {
        grounded.is_grounded = is_grounded;
    }
}
