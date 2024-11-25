use bevy::{ecs::query::QueryData, prelude::*};
use bevy_rapier3d::prelude::*;

use super::{
    objects::{
        grounded::{set_grounded, Grounded},
        player::Player,
        NetworkObject,
    },
    GameLogic,
};

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            .add_plugins(RapierDebugRenderPlugin::default());
        app.add_systems(
            FixedUpdate,
            (apply_kinematics_system.in_set(GameLogic::Kinematics),),
        );
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct KinematicsQuery {
    entity: Entity,
    net_obj: &'static NetworkObject,
    controller: &'static KinematicCharacterController,
    transform: &'static mut Transform,
    collider: &'static Collider,
    player: &'static mut Player,
    grounded: Option<&'static mut Grounded>,
}

fn apply_kinematics_system(
    mut context: ResMut<RapierContext>,
    mut query: Query<KinematicsQuery>,
    time: Res<Time>,
) {
    for mut item in query.iter_mut() {
        apply_kinematics(
            &mut context,
            item.entity,
            item.controller,
            &mut item.transform,
            item.collider,
            item.player.kinematics.get_velocity(),
            item.grounded.as_deref_mut(),
            time.delta_seconds(),
        );
    }
}

pub fn apply_kinematics(
    context: &mut RapierContext,
    entity: Entity,
    controller: &KinematicCharacterController,
    transform: &mut Transform,
    collider: &Collider,
    movement: Vec3,
    grounded: Option<&mut Grounded>,
    delta_seconds: f32,
) {
    let output = context.move_shape(
        movement * delta_seconds,
        collider,
        transform.translation,
        transform.rotation,
        0.0,
        &char_ctrl_to_move_opts(controller),
        QueryFilter::default().exclude_collider(entity),
        |_| {},
    );
    set_grounded(grounded, output.grounded);
    transform.translation += output.effective_translation;
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
