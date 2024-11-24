use std::{collections::VecDeque, time::Duration};

use bevy::{
    color::palettes::tailwind::{GREEN_500, YELLOW_500},
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetClient};

use crate::{
    message::{
        client::{MessageReaderOnClient, OrderedInput, UnreliableMessageFromClient},
        server::{PlayerInit, ReliableMessageFromServer, UnreliableMessageFromServer},
        spawn::NetworkSpawn,
    },
    shared::{
        console::ConsoleMessage,
        objects::{
            gizmo::spawn_raycast_visual, grounded::Grounded, LastSyncTracker, NetworkObject,
        },
        physics::{apply_kinematics, Kinematics},
    },
};

use super::{
    Input, JumpCooldown, LocalPlayer, LocalPlayerTag, Player, Shot, ShotNothing, ShotTarget,
};

const RECONCILE: bool = true;
const PREDICT: bool = true;

#[derive(Component, Default)]
pub struct JumpCooldownHistory {
    history: VecDeque<Duration>,
}

impl JumpCooldownHistory {
    /// Adds a new elapsed time to the history.
    pub fn push(&mut self, elapsed_time: Duration) {
        self.history.push_back(elapsed_time);
        // Optionally prune to maintain the size limit
        if self.history.len() > 100 {
            self.prune();
        }
    }

    /// Prunes the history to keep only the last 100 entries.
    pub fn prune(&mut self) {
        while self.history.len() > 100 {
            self.history.pop_front(); // Removes the oldest entry
        }
    }
}

#[derive(Resource, Default)]
pub struct InputBuffer {
    inputs: VecDeque<OrderedInput>,
    count: u64,
}

impl InputBuffer {
    fn push_input(&mut self, input: Input) -> u64 {
        self.count += 1;
        self.inputs.push_back(OrderedInput {
            input,
            order: self.count,
        });
        return self.count;
    }

    fn prune(&mut self, max_length: usize) {
        while self.inputs.len() > max_length {
            self.inputs.pop_front();
        }
    }

    fn inputs_after_order(&self, order: u64) -> Vec<OrderedInput> {
        self.inputs
            .iter()
            .filter(|input| input.order > order)
            .cloned()
            .collect()
    }

    fn get_latest(&self) -> Option<Input> {
        let len = self.inputs.len();
        if len == 0 {
            return None;
        }
        self.inputs
            .get(self.inputs.len() - 1)
            .map(|v| v.input.clone())
    }
}

#[derive(Debug, Component)]
pub struct PlayerCameraTarget;

#[derive(Debug, Component)]
pub struct PlayerCamera;

pub fn spawn_player_camera(
    mut commands: Commands,
    players: Query<Entity, Added<LocalPlayerTag>>,
) {
    let Ok(entity) = players.get_single() else {
        return;
    };

    println!("spawning player camera");
    commands.entity(entity).with_children(|parent| {
        parent.spawn((
            PlayerCameraTarget,
            TransformBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
        ));
    });
    commands.spawn((
        PlayerCamera,
        Camera3dBundle {
            projection: PerspectiveProjection {
                fov: 60.0_f32.to_radians(),
                ..default()
            }
            .into(),
            ..default()
        },
    ));
}

pub fn rotate_player(
    mut mouse_motion: EventReader<MouseMotion>,
    mut player: Query<&mut Transform, (With<LocalPlayerTag>, Without<PlayerCameraTarget>)>,
    mut camera: Query<&mut Transform, (With<PlayerCameraTarget>, Without<LocalPlayerTag>)>,
    q_windows: Query<&Window, With<PrimaryWindow>>,
) {
    let primary_window = q_windows.single();
    if primary_window.cursor.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok(mut transform) = player.get_single_mut() else {
        return;
    };
    let Ok(mut camera) = camera.get_single_mut() else {
        return;
    };
    for motion in mouse_motion.read() {
        let yaw = -motion.delta.x * 0.003;
        let pitch = -motion.delta.y * 0.002;
        // Order of rotations is important, see <https://gamedev.stackexchange.com/a/136175/103059>
        transform.rotate_y(yaw);
        camera.rotate_local_x(pitch);
    }
}

pub fn rubber_band_player_camera(
    target: Query<&GlobalTransform, (With<PlayerCameraTarget>, Without<PlayerCamera>)>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<PlayerCameraTarget>)>,
) {
    let Ok(target) = target.get_single() else {
        return;
    };
    let Ok(mut camera) = camera.get_single_mut() else {
        return;
    };

    let target = target.compute_transform();
    camera.rotation = target.rotation;
    if camera.translation.distance_squared(target.translation) < (0.1 * 0.1) {
        camera.translation = target.translation;
    } else {
        camera.translation = camera.translation.lerp(target.translation, 0.3);
    }
}

#[derive(Default)]
pub struct PressedShootLastFrame(bool);

pub fn read_input(
    mut pressed_shoot: Local<PressedShootLastFrame>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut ibuf: ResMut<InputBuffer>,
    query: Query<(&Transform, Entity), With<LocalPlayerTag>>,
    camera: Query<&Transform, With<PlayerCamera>>,
    net_objs: Query<(&NetworkObject, &Transform)>,
    mut client: ResMut<RenetClient>,
    context: Res<RapierContext>,
    mut commands: Commands,
    mut console: EventWriter<ConsoleMessage>,
) {
    let Ok((player_transform, entity)) = query.get_single() else {
        error!("no player found when reading input");
        return;
    };
    let mut local_direction = Vec3::ZERO;

    if keyboard_input.pressed(KeyCode::KeyW) {
        local_direction -= Vec3::Z;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        local_direction += Vec3::Z;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        local_direction -= Vec3::X;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        local_direction += Vec3::X;
    }
    let pressed_shoot_last_frame = pressed_shoot.0;
    pressed_shoot.0 = mouse_input.pressed(MouseButton::Left);
    let shoot = !pressed_shoot_last_frame && pressed_shoot.0;

    let shot = if shoot {
        let camera = camera.single();
        let ray_pos = camera.translation;
        let ray_dir = *camera.forward();
        let max_toi = 10.0;
        if let Some((entity, toi)) = context.cast_ray(
            ray_pos,
            ray_dir,
            max_toi,
            false,
            QueryFilter::default().exclude_collider(entity),
        ) {
            spawn_raycast_visual(&mut commands, ray_pos, ray_dir, toi, GREEN_500, 2000);

            // TODO: handle if they shot something that wasn't a network object, e.g. the floor.
            let impact_point = ray_pos + (ray_dir * toi);
            net_objs.get(entity).ok().map(|(obj, transform)| {
                let relative_position = impact_point - transform.translation;
                Shot::ShotTarget(ShotTarget {
                    target: obj.clone(),
                    relative_position,
                })
            })
        } else {
            spawn_raycast_visual(&mut commands, ray_pos, ray_dir, max_toi, YELLOW_500, 2000);
            Some(Shot::ShotNothing(ShotNothing {
                vector: ray_dir * max_toi,
            }))
        }
    } else {
        None
    };

    if let Some(ref shot) = shot {
        console.send(ConsoleMessage::new(format!("you shot {:?}", shot)));
    }

    if local_direction.length_squared() > 0.0 {
        local_direction = local_direction.normalize();
    }

    let world_direction = player_transform.rotation * local_direction;
    let world_direction_xz = Vec3::new(world_direction.x, 0.0, world_direction.z);
    let final_direction = if world_direction_xz.length_squared() > 0.0 {
        world_direction_xz.normalize()
    } else {
        Vec3::ZERO
    };

    let input = Input {
        direction: final_direction,
        jump: keyboard_input.pressed(KeyCode::Space),
        shot,
    };
    let order = ibuf.push_input(input.clone());
    if input.is_non_zero() {
        let message = UnreliableMessageFromClient::Input(OrderedInput { input, order });
        let bytes = bincode::serialize(&message).unwrap();
        client.send_message(DefaultChannel::Unreliable, bytes);
        ibuf.prune(100);
    }
}

pub fn recv_player_shot(
    reader: Res<MessageReaderOnClient>,
    mut commands: Commands,
    player_query: Query<(&NetworkObject, &Transform), With<Player>>,
    net_obj_query: Query<(&NetworkObject, &Transform)>,
) {
    for msg in reader.unreliable_messages() {
        let UnreliableMessageFromServer::PlayerShot(shooter, shot) = msg else {
            continue;
        };
        let shooter_pos = player_query
            .iter()
            .find(|(obj, _)| *obj == shooter)
            .map(|(_, t)| t);
        let Some(shooter_pos) = shooter_pos else {
            continue;
        };

        match shot {
            Shot::ShotNothing(shot) => match shot.vector.try_normalize() {
                Some(vector) => {
                    spawn_raycast_visual(
                        &mut commands,
                        shooter_pos.translation,
                        vector,
                        shot.vector.length(),
                        YELLOW_500,
                        2000,
                    );
                }
                _ => warn!("got zero valued shot vector"),
            },
            Shot::ShotTarget(shot) => {
                let target_pos = net_obj_query
                    .iter()
                    .find(|(obj, _)| **obj == shot.target)
                    .map(|(_, t)| t);
                let Some(target_pos) = target_pos else {
                    error!("tried to recv shot for non existant: {:?}", shot);
                    continue;
                };
                let target_shot_pos = target_pos.translation + shot.relative_position;
                let ray = target_shot_pos - shooter_pos.translation;
                spawn_raycast_visual(
                    &mut commands,
                    shooter_pos.translation,
                    ray.normalize(),
                    ray.length(),
                    GREEN_500,
                    2000,
                );
            }
        }
    }
}

pub fn spawn_players(
    mut commands: Commands,
    reader: Res<MessageReaderOnClient>,
    local_player: Res<LocalPlayer>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Spawn(spawn) = msg else {
            continue;
        };
        if spawn.net_obj == local_player.0 {
            continue;
        };
        if let NetworkSpawn::Player(transform) = spawn.net_spawn {
            println!("spawning player");
            commands
                .spawn(Player)
                .insert(LastSyncTracker::<Transform>::new(spawn.tick.clone()))
                .insert((
                    KinematicCharacterController::default(),
                    RigidBody::KinematicPositionBased,
                    Collider::capsule_y(0.5, 0.25),
                    TransformBundle::from_transform(transform),
                ))
                .insert(spawn.net_obj.clone());
        }
    }
}

pub fn recv_position_sync(
    reader: Res<MessageReaderOnClient>,
    mut nonlocal_players: Query<
        (
            &mut Transform,
            &NetworkObject,
            &mut LastSyncTracker<Transform>,
        ),
        (With<Player>, Without<LocalPlayerTag>),
    >,
    mut local_player: Query<
        (
            Entity,
            &mut Transform,
            &NetworkObject,
            &mut LastSyncTracker<Transform>,
            &Collider,
            &KinematicCharacterController,
            &mut Grounded,
            &mut Kinematics,
            &mut JumpCooldown,
            &mut JumpCooldownHistory,
        ),
        (With<Player>, With<LocalPlayerTag>),
    >,
    ibuf: Res<InputBuffer>,
    time: Res<Time>,
    mut context: ResMut<RapierContext>,
) {
    for msg in reader.unreliable_messages() {
        match msg {
            UnreliableMessageFromServer::PlayerPositionSync(pos_sync) => {
                for (mut transform, obj, mut last_sync_tracker) in nonlocal_players.iter_mut() {
                    if *obj == pos_sync.net_obj && last_sync_tracker.last_tick < pos_sync.tick {
                        last_sync_tracker.last_tick = pos_sync.tick.clone();
                        transform.translation = pos_sync.translation;
                    }
                }
            }
            UnreliableMessageFromServer::OwnedPlayerSync(owned_sync) => {
                let Ok(record) = local_player.get_single_mut() else {
                    continue;
                };
                let (
                    entity,
                    mut transform,
                    obj,
                    mut last_sync_tracker,
                    shape,
                    controller,
                    mut grounded,
                    mut kinematics,
                    mut jump_cooldown,
                    mut jump_history,
                ) = record;
                if *obj == owned_sync.net_obj && last_sync_tracker.last_tick < owned_sync.tick {
                    last_sync_tracker.last_tick = owned_sync.tick.clone();
                    transform.translation = owned_sync.translation;
                    *kinematics = owned_sync.kinematics.clone();
                    if RECONCILE {
                        let mut inputs = ibuf.inputs_after_order(owned_sync.last_input_order);
                        for _ in 0..(inputs.len() - 1) {
                            jump_history.history.pop_back();
                        }
                        if let Some(duration) = jump_history.history.pop_back() {
                            jump_cooldown.timer.set_elapsed(duration);
                        }
                        // BOOKMARK: rollback
                        inputs.pop(); // Current frame input
                        for input in inputs {
                            super::apply_input(
                                &mut context,
                                &input.input,
                                &mut transform,
                                shape,
                                controller,
                                &time,
                                entity,
                                &mut kinematics,
                                &mut grounded,
                                &mut jump_cooldown,
                            );
                            apply_kinematics(
                                &mut context,
                                entity,
                                controller,
                                &mut transform,
                                shape,
                                &mut kinematics,
                                Some(&mut grounded),
                                &time,
                            );
                            jump_history.push(jump_cooldown.timer.elapsed());
                            jump_cooldown.timer.tick(time.delta());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn predict_movement(
    mut context: ResMut<RapierContext>,
    ibuf: Res<InputBuffer>,
    mut local_player: Query<
        (
            Entity,
            &mut Transform,
            &Collider,
            &KinematicCharacterController,
            &mut Grounded,
            &mut Kinematics,
            &mut JumpCooldown,
        ),
        (With<Player>, With<LocalPlayerTag>),
    >,
    time: Res<Time>,
) {
    let Ok(local_player) = local_player.get_single_mut() else {
        warn!("no local player");
        return;
    };

    let Some(input) = ibuf.get_latest() else {
        warn!("no latest input");
        return;
    };

    if !PREDICT {
        return;
    }

    info!("vec: {}", input.direction.length());

    let (
        entity,
        mut transform,
        collider,
        controller,
        mut grounded,
        mut kinematics,
        mut jump_cooldown,
    ) = local_player;

    super::apply_input(
        &mut context,
        &input,
        &mut transform,
        collider,
        controller,
        &time,
        entity,
        &mut kinematics,
        &mut grounded,
        &mut jump_cooldown,
    );
}

pub fn spawn_player(commands: &mut Commands, player_info: &PlayerInit) {
    commands
        .spawn(Player)
        .insert(Kinematics::new().with_gravity())
        .insert(LastSyncTracker::<Transform>::new(player_info.tick.clone()))
        .insert((
            KinematicCharacterController::default(),
            RigidBody::KinematicPositionBased,
            Collider::capsule_y(0.5, 0.25),
            TransformBundle::from_transform(player_info.transform),
        ))
        .insert(Grounded::default())
        .insert(player_info.net_obj.clone())
        .insert(JumpCooldown::default())
        .insert(JumpCooldownHistory::default())
        .insert(LocalPlayerTag);
}
