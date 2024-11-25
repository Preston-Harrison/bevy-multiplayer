use std::time::Duration;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::shared::{
    physics::char_ctrl_to_move_opts,
    GameLogic,
};

use self::client::PlayerClientPlugin;

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
            app.add_plugins(PlayerClientPlugin);
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
    kinematics: &mut PlayerKinematics,
    grounded: &mut Grounded,
    jump_cooldown: &mut JumpCooldown,
) {
    let movement = input.direction * 5.0 * time.delta_seconds();
    if input.jump && grounded.grounded_this_tick() && jump_cooldown.timer.finished() {
        info!("setting jump from apply_input");
        kinematics.tick(false, true, time.delta());
        jump_cooldown.timer.reset();
    } else {
        info!("clearing jump from apply_input");
        kinematics.tick(grounded.grounded_this_tick(), false, time.delta());
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AirTime {
    Grounded,
    Airborne(Duration),
}

#[derive(Component, Serialize, Deserialize, Debug, Clone)]
pub struct PlayerKinematics {
    time_in_air: AirTime,
    is_jumping: bool,
}

impl Default for PlayerKinematics {
    fn default() -> Self {
        Self {
            time_in_air: AirTime::Grounded,
            is_jumping: false,
        }
    }
}

impl PlayerKinematics {
    pub fn tick(&mut self, is_grounded: bool, jumped: bool, delta: Duration) {
        self.is_jumping |= jumped;
        if is_grounded {
            self.time_in_air = AirTime::Grounded;
            self.is_jumping = false;
        } else {
            self.time_in_air = match self.time_in_air {
                AirTime::Grounded => AirTime::Airborne(delta),
                AirTime::Airborne(time) => AirTime::Airborne(time + delta),
            };
        }
    }

    pub fn get_velocity(&self) -> Vec3 {
        let gravity = match self.time_in_air {
            AirTime::Airborne(duration) => Vec3::Y * -10.0 * duration.as_secs_f32(),
            AirTime::Grounded => Vec3::ZERO,
        };
        let jump = if self.is_jumping {
            Vec3::Y * 20.0
        } else {
            Vec3::ZERO
        };
        gravity + jump
    }

    pub fn is_different(&self, other: &Self) -> bool {
        // Compare time_in_air
        match (&self.time_in_air, &other.time_in_air) {
            (AirTime::Grounded, AirTime::Grounded) => {}
            (AirTime::Airborne(d1), AirTime::Airborne(d2)) => {
                if (*d1 > *d2 && *d1 - *d2 > Duration::from_millis(100))
                    || (*d2 > *d1 && *d2 - *d1 > Duration::from_millis(100))
                {
                    return true;
                }
            }
            _ => return true, // Different enum variants
        }

        // Compare is_jumping
        if self.is_jumping != other.is_jumping {
            return true;
        }

        false // No differences found
    }
}
