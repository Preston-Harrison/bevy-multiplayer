use std::collections::VecDeque;

use bevy::prelude::*;

// #[derive(Component)]
// pub struct Rollback<T: Component + Clone> {
//     queue: VecDeque<T>,
// }
//
// impl<T: Component + Clone> Rollback<T> {
//     fn track(&mut self, next: &T) {
//         self.queue.push_front(next.clone());
//     }
// }
//
// fn track_rollback<T: Component + Clone>(mut q: Query<(&T, &mut Rollback<T>)>) {
//     for (t, mut rollback) in q.iter_mut() {
//         rollback.track(t);
//     }
// }

pub trait Interpolate {
    /// Interpolate between self and target for one tick.
    fn interpolate(&mut self, target: &Self);
}

#[derive(Component)]
pub struct Interpolated<T: Component + Interpolate> {
    target: T,
}

fn interpolate<T: Component + Interpolate>(mut q: Query<(&mut T, &Interpolated<T>)>) {
    for (mut t, interp) in q.iter_mut() {
        t.interpolate(&interp.target);
    }
}

#[derive(Component)]
pub struct Prespawned {
    id: u64,
}

#[derive(Component)]
pub struct ServerObject {
    id: u64,
}

fn replace_prespawned(
    mut commands: Commands,
    prespawned: Query<(Entity, &Prespawned)>,
    server_objs: Query<&ServerObject, Added<ServerObject>>,
) {
    for server_obj in server_objs.iter() {
        for (e, spawn) in prespawned.iter() {
            if server_obj.id == spawn.id {
                commands.entity(e).despawn_recursive();
            }
        }
    }
}

pub mod input {
    pub struct InputBuffer {
        pub inputs: Vec<Input>,
    }

    pub struct Input {
        pub x: i8,
        pub y: i8,
    }
}
