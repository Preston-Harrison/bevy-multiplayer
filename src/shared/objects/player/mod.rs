use std::time::Duration;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::shared::{
    physics::{char_ctrl_to_move_opts, Kinematics},
    GameLogic,
};

use super::{grounded::Grounded, NetworkObject};

pub mod client;
pub mod server;

const JUMP_KEY: &str = "jump";
const JUMP_VELOCITY: Vec3 = Vec3::new(0.0, 10.0, 0.0);

pub struct PlayerPlugin {
    pub is_server: bool,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                cancel_jump_velocity_if_just_landed.in_set(GameLogic::End),
                tick_jump_cooldown.in_set(GameLogic::Start),
            ),
        );

        if self.is_server {
            app.insert_resource(server::ClientInputs::default());
            app.add_systems(
                FixedUpdate,
                (
                    server::apply_inputs.in_set(GameLogic::Game),
                    server::read_inputs.in_set(GameLogic::ReadInput),
                    server::broadcast_player_data.in_set(GameLogic::Sync),
                    server::broadcast_player_spawns.in_set(GameLogic::Sync),
                    server::load_player.in_set(GameLogic::Sync),
                    server::init_players.in_set(GameLogic::Spawn),
                ),
            );
        } else {
            app.insert_resource(client::InputBuffer::default());
            app.add_systems(
                FixedUpdate,
                (
                    client::spawn_player_camera,
                    client::read_input.in_set(GameLogic::Start),
                    client::spawn_players.in_set(GameLogic::Spawn),
                    client::recv_position_sync.in_set(GameLogic::Sync),
                    client::recv_player_shot.in_set(GameLogic::Sync),
                    client::predict_movement.in_set(GameLogic::Game),
                ),
            );
            app.add_systems(
                Update,
                (
                    client::rotate_player,
                    client::rubber_band_player_camera.after(client::rotate_player),
                ),
            );
        }
    }
}

#[derive(Component, Default)]
pub struct JumpCooldown {
    timer: Timer,
}

impl JumpCooldown {
    fn new() -> Self {
        Self {
            timer: Timer::new(Duration::from_millis(200), TimerMode::Once),
        }
    }
}

#[derive(Resource)]
pub struct LocalPlayer(pub NetworkObject);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShotTarget {
    target: NetworkObject,
    relative_position: Vec3,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShotNothing {
    vector: Vec3,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Shot {
    ShotTarget(ShotTarget),
    ShotNothing(ShotNothing),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Input {
    direction: Vec3,
    jump: bool,
    shot: Option<Shot>,
}

impl Input {
    fn is_non_zero(&self) -> bool {
        self.direction.length_squared() > (0.1 * 0.1) || self.shot.is_some() || self.jump
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct LocalPlayerTag;

fn tick_jump_cooldown(
    mut query: Query<(&mut JumpCooldown, Option<&mut client::JumpCooldownHistory>)>,
    time: Res<Time>,
) {
    for (mut cooldown, history) in query.iter_mut() {
        cooldown.timer.tick(time.delta());
        if let Some(mut history) = history {
            history.push(cooldown.timer.elapsed());
            history.prune();
        }
    }
}
fn apply_input(
    context: &mut RapierContext,
    input: &Input,
    transform: &mut Transform,
    shape: &Collider,
    char_controller: &KinematicCharacterController,
    time: &Time,
    curr_player: Entity,
    kinematics: &mut Kinematics,
    grounded: &mut Grounded,
    jump_cooldown: &mut JumpCooldown,
) {
    let movement = input.direction * 5.0 * time.delta_seconds();
    if input.jump && grounded.grounded_this_tick() && jump_cooldown.timer.finished() {
        info!("setting jump from apply_input");
        kinematics.set_velocity(JUMP_KEY, JUMP_VELOCITY);
        jump_cooldown.timer.reset();
    } else if !grounded.was_grounded_last_tick() && grounded.grounded_this_tick() {
        kinematics.set_velocity(JUMP_KEY, Vec3::ZERO);
        info!("clearing jump from apply_input");
    }

    let out = context.move_shape(
        movement,
        shape,
        transform.translation,
        transform.rotation,
        0f32,
        &char_ctrl_to_move_opts(char_controller),
        QueryFilter::default().exclude_collider(curr_player),
        |_| {},
    );
    transform.translation += out.effective_translation;
    grounded.set_is_grounded(out.grounded);
}

fn cancel_jump_velocity_if_just_landed(
    mut query: Query<(&Grounded, &mut Kinematics, &JumpCooldown), With<Player>>,
) {
    for (grounded, mut kinematics, jump_cooldown) in query.iter_mut() {
        if grounded.grounded_this_tick() && jump_cooldown.timer.finished() {
            info!("clearing jump because cancelled");
            kinematics.set_velocity(JUMP_KEY, Vec3::ZERO);
        }
    }
}
