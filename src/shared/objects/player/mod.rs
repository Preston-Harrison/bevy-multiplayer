use std::time::Duration;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::shared::{
    physics::char_ctrl_to_move_opts, proc::LoadsChunks, tick::Tick, GameLogic, SpawnMode,
};

use self::{
    client::PlayerClientPlugin,
    server::{LastInputTracker, PlayerServerPlugin},
};

use super::{
    grounded::Grounded,
    gun::{Gun, GunType},
    LastSyncTracker, NetworkObject,
};

pub mod client;
pub mod server;

pub struct PlayerPlugin {
    pub is_server: bool,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (tick_jump_cooldown.in_set(GameLogic::Start),));

        if self.is_server {
            app.add_plugins(PlayerServerPlugin);
        } else {
            app.add_plugins(PlayerClientPlugin);
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
pub struct Player {
    pub jump_cooldown_timer: Timer,
    pub kinematics: PlayerKinematics,
}

/// Represents where the head (or camera target, on the client) of the player is. Guns are children of this.
#[derive(Component)]
pub struct PlayerHead;

impl Player {
    fn new() -> Self {
        Self {
            jump_cooldown_timer: Timer::new(Duration::from_millis(200), TimerMode::Once),
            kinematics: PlayerKinematics::default(),
        }
    }
}

#[derive(Component)]
pub struct LocalPlayerTag;

fn tick_jump_cooldown(mut query: Query<&mut Player>, time: Res<Time>) {
    for mut player in query.iter_mut() {
        player.jump_cooldown_timer.tick(time.delta());
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
    player: &mut Player,
    grounded: &mut Grounded,
) {
    let movement = input.direction * 5.0 * time.delta_seconds();
    if input.jump && grounded.is_grounded() && player.jump_cooldown_timer.finished() {
        player.kinematics.update(false, true);
        player.jump_cooldown_timer.reset();
    } else {
        player.kinematics.update(grounded.is_grounded(), false);
    }
    player.kinematics.tick(time.delta());

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

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    pub fn tick(&mut self, delta: Duration) {
        self.time_in_air = match self.time_in_air {
            AirTime::Grounded => AirTime::Airborne(delta),
            AirTime::Airborne(time) => AirTime::Airborne(time + delta),
        };
    }

    pub fn update(&mut self, is_grounded: bool, jumped: bool) {
        self.is_jumping |= jumped;
        if is_grounded {
            self.time_in_air = AirTime::Grounded;
            self.is_jumping = false;
        } else {
        }
    }

    pub fn get_velocity(&self) -> Vec3 {
        let gravity = match self.time_in_air {
            AirTime::Airborne(duration) => Vec3::Y * -10.0 * duration.as_secs_f32(),
            AirTime::Grounded => Vec3::ZERO,
        };
        let jump = if self.is_jumping {
            Vec3::Y * 5.0
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

pub fn spawn_player(
    spawn_mode: SpawnMode<(), Tick>,
    commands: &mut Commands,
    transform: Transform,
    net_obj: NetworkObject,
) {
    let mut entity = commands.spawn((
        Player::new(),
        KinematicCharacterController::default(),
        RigidBody::KinematicPositionBased,
        Collider::capsule_y(0.5, 0.25),
        SpatialBundle::from_transform(transform),
        Grounded::default(),
        net_obj.clone(),
        LoadsChunks,
    ));

    match spawn_mode {
        SpawnMode::Server(_) => {
            entity.insert(LastInputTracker::default());
            entity.with_children(|parent| {
                parent
                    .spawn((
                        PlayerHead,
                        SpatialBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
                    ))
                    .with_children(|parent| {
                        parent.spawn((SpatialBundle::default(), Gun::new(GunType::PurpleRifle)));
                    });
            });
        }
        SpawnMode::Client(tick) => {
            entity
                .insert(LocalPlayerTag)
                .insert(LastSyncTracker::<Transform>::new(tick));
        }
    }
}
