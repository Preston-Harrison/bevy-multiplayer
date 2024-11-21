use bevy::{prelude::*, utils::HashMap};
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    objects::{
        grounded::{set_grounded, Grounded},
        player::LocalPlayerTag,
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
            (
                apply_kinematics.in_set(GameLogic::Kinematics),
                cancel_gravity_if_grounded
                    .in_set(GameLogic::Kinematics)
                    .before(apply_kinematics),
            ),
        );
    }
}

/// TODO make this serialize into smaller bytes.
#[derive(Component, Serialize, Deserialize, Debug, Clone)]
pub struct Kinematics {
    velocity: HashMap<String, Vec3>,
    acceleration: HashMap<String, Vec3>,
}

impl Kinematics {
    pub fn new() -> Self {
        Self {
            velocity: HashMap::new(),
            acceleration: HashMap::new(),
        }
    }

    pub fn with_gravity(mut self) -> Self {
        self.set_acceleration(GRAVITY_KEY, Vec3::Y * GRAVITY);
        self
    }

    pub fn set_velocity(&mut self, name: impl Into<String>, value: Vec3) {
        self.velocity.insert(name.into(), value);
    }

    pub fn set_acceleration(&mut self, name: impl Into<String>, value: Vec3) {
        self.acceleration.insert(name.into(), value);
    }

    pub fn accelerate(&mut self, seconds: f32) {
        for (k, v) in self.acceleration.iter() {
            let velocity = self.velocity.entry(k.clone()).or_default();
            *velocity += *v * seconds;
        }
    }

    pub fn get_displacement(&self, seconds: f32) -> Vec3 {
        self.velocity.values().sum::<Vec3>() * seconds
    }
}

const GRAVITY: f32 = -10.0;
const GRAVITY_KEY: &str = "gravity";

fn cancel_gravity_if_grounded(mut kinematics: Query<(&mut Kinematics, &Grounded)>) {
    for (mut kinematics, grounded) in kinematics.iter_mut() {
        if grounded.is_grounded {
            kinematics.set_velocity(GRAVITY_KEY, Vec3::ZERO);
        }
    }
}

fn apply_kinematics(
    mut context: ResMut<RapierContext>,
    player: Query<&NetworkObject, With<LocalPlayerTag>>,
    mut query: Query<(
        Entity,
        &NetworkObject,
        &KinematicCharacterController,
        &mut Transform,
        &Collider,
        &mut Kinematics,
        Option<&mut Grounded>,
    )>,
    time: Res<Time>,
) {
    let local_player_tag = player.get_single().ok();
    for (entity, net_obj, controller, mut transform, collider, mut velocity, mut grounded) in
        query.iter_mut()
    {
        if local_player_tag.is_some_and(|tag| tag != net_obj) {
            continue;
        }
        velocity.accelerate(time.delta_seconds());
        let movement = velocity.get_displacement(time.delta_seconds());
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
        set_grounded(&mut grounded, output.grounded);
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
