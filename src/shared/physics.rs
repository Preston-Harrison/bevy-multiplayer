use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::{
    objects::{player::LocalPlayerTag, NetworkObject},
    GameLogic,
};

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            .add_plugins(RapierDebugRenderPlugin::default());
        app.add_systems(FixedUpdate, apply_gravity.in_set(GameLogic::Game));
    }
}

const GRAVITY: f32 = -10.0;

#[derive(Component, Default)]
pub struct Gravity;

fn apply_gravity(
    mut context: ResMut<RapierContext>,
    player: Query<&NetworkObject, With<LocalPlayerTag>>,
    mut query: Query<
        (
            Entity,
            &NetworkObject,
            &KinematicCharacterController,
            &mut Transform,
            &Collider,
        ),
        With<Gravity>,
    >,
    time: Res<Time>,
) {
    let local_player_tag = player.get_single().ok();
    for (entity, net_obj, controller, mut transform, collider) in query.iter_mut() {
        if local_player_tag.is_some_and(|tag| tag != net_obj) {
            continue;
        }
        let movement = Vec3::Y * GRAVITY * time.delta_seconds();
        let output = context.move_shape(
            movement,
            collider,
            transform.translation,
            transform.rotation,
            0.0,
            &char_ctrl_to_move_opts(controller),
            QueryFilter::default().exclude_collider(entity),
            |_| {},
        );
        transform.translation += output.effective_translation;
    }
}

pub fn char_ctrl_to_move_opts(char_controller: &KinematicCharacterController) -> MoveShapeOptions {
    MoveShapeOptions {
        up: char_controller.up,
        offset: char_controller.offset,
        slide: char_controller.slide,
        autostep: char_controller.autostep,
        max_slope_climb_angle: char_controller.max_slope_climb_angle,
        min_slope_slide_angle: char_controller.min_slope_slide_angle,
        apply_impulse_to_dynamic_bodies: char_controller.apply_impulse_to_dynamic_bodies,
        snap_to_ground: char_controller.snap_to_ground,
        normal_nudge_factor: char_controller.normal_nudge_factor,
    }
}
