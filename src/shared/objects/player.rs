use std::{collections::VecDeque, time::Duration};

use bevy::{
    color::palettes::tailwind::{GREEN_500, YELLOW_500},
    input::mouse::MouseMotion,
    prelude::*,
    utils::HashMap,
    window::{CursorGrabMode, PrimaryWindow},
};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{ClientId, DefaultChannel, RenetClient, RenetServer};
use serde::{Deserialize, Serialize};

use crate::{
    message::{
        client::{MessageReaderOnClient, OrderedInput, UnreliableMessageFromClient},
        server::{
            self, OwnedPlayerSync, PlayerInit, PlayerPositionSync, ReliableMessageFromServer,
            Spawn, UnreliableMessageFromServer,
        },
        spawn::NetworkSpawn,
    },
    server::{ClientNetworkObjectMap, PlayerNeedsInit, PlayerWantsUpdates},
    shared::{
        console::ConsoleMessage,
        physics::{char_ctrl_to_move_opts, Kinematics},
        tick::Tick,
        GameLogic,
    },
};

use super::{gizmo::spawn_raycast_visual, grounded::Grounded, LastSyncTracker, NetworkObject};

const PREDICT: bool = true;
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
                tick_jump_cooldown,
            ),
        );

        if self.is_server {
            app.insert_resource(ClientInputs::default());
            app.add_systems(
                FixedUpdate,
                (
                    apply_inputs.in_set(GameLogic::Game),
                    read_inputs.in_set(GameLogic::ReadInput),
                    broadcast_player_data.in_set(GameLogic::Sync),
                    broadcast_player_spawns.in_set(GameLogic::Sync),
                    load_player.in_set(GameLogic::Sync),
                    init_players.in_set(GameLogic::Spawn),
                ),
            );
        } else {
            app.insert_resource(InputBuffer::default());
            app.add_systems(
                FixedUpdate,
                (
                    spawn_player_camera,
                    spawn_players.in_set(GameLogic::Spawn),
                    recv_position_sync.in_set(GameLogic::Sync),
                    recv_player_shot.in_set(GameLogic::Sync),
                    read_input.in_set(GameLogic::Start),
                ),
            );
            app.add_systems(
                Update,
                (
                    rotate_player,
                    rubber_band_player_camera.after(rotate_player),
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
}

/// TODO: fix memory leaks as this doesn't clean up disconnected clients.
#[derive(Resource, Default)]
pub struct ClientInputs {
    inputs: HashMap<NetworkObject, Vec<OrderedInput>>,
    clients: HashMap<NetworkObject, ClientId>,
}

impl ClientInputs {
    fn push_input(&mut self, net_obj: NetworkObject, input: OrderedInput, client_id: ClientId) {
        self.inputs.entry(net_obj.clone()).or_default().push(input);
        self.clients.insert(net_obj, client_id);
    }

    /// Removes and returns the lowest-order input for each `NetworkObject`.
    fn pop_inputs(&mut self) -> HashMap<NetworkObject, OrderedInput> {
        let mut inputs = HashMap::new();

        for (obj, ord_inputs) in self.inputs.iter_mut() {
            if let Some((min_index, _)) = ord_inputs
                .iter()
                .enumerate()
                .min_by_key(|(_, input)| input.order)
            {
                let input = ord_inputs.remove(min_index);
                inputs.insert(obj.clone(), input);
            }
        }

        inputs
    }

    /// Ensures that each buffer in `ClientInputs` is no longer than `max_length`.
    /// Removes the input with the lowest order if the buffer exceeds the limit.
    fn prune(&mut self, max_length: usize) {
        for (_, ord_inputs) in self.inputs.iter_mut() {
            while ord_inputs.len() > max_length {
                if let Some((min_index, _)) = ord_inputs
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, input)| input.order)
                {
                    ord_inputs.remove(min_index);
                }
            }
        }
    }

    fn get_client_id(&self, net_obj: &NetworkObject) -> Option<ClientId> {
        self.clients.get(net_obj).cloned()
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component, Default)]
pub struct LastInputTracker {
    order: u64,
}

#[derive(Component)]
pub struct LocalPlayerTag;

fn broadcast_player_spawns(
    query: Query<(&NetworkObject, &Transform), Added<Player>>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (network_obj, transform) in query.iter() {
        let net_spawn = NetworkSpawn::Player(transform.clone());
        let spawn = Spawn {
            net_spawn,
            net_obj: network_obj.clone(),
            tick: tick.clone(),
        };
        let message = ReliableMessageFromServer::Spawn(spawn);
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
        println!("spawning player");
    }
}

fn spawn_players(
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

fn broadcast_player_data(
    query: Query<(&NetworkObject, &Transform, &LastInputTracker, &Kinematics), With<Player>>,
    client_netmap: Res<ClientNetworkObjectMap>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (obj, transform, input_tracker, kinematics) in query.iter() {
        let Some(client_id) = client_netmap.net_obj_to_client.get(obj) else {
            warn!("no client id for player obj in broadcast_player_data");
            continue;
        };

        let message = UnreliableMessageFromServer::PlayerPositionSync(PlayerPositionSync {
            net_obj: obj.clone(),
            translation: transform.translation.clone(),
            tick: tick.clone(),
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message_except(*client_id, DefaultChannel::Unreliable, bytes);

        let message = UnreliableMessageFromServer::OwnedPlayerSync(OwnedPlayerSync {
            net_obj: obj.clone(),
            translation: transform.translation.clone(),
            tick: tick.clone(),
            kinematics: kinematics.clone(),
            last_input_order: input_tracker.order,
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.send_message(*client_id, DefaultChannel::Unreliable, bytes);
    }
}

fn recv_position_sync(
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
                ) = record;
                if *obj == owned_sync.net_obj && last_sync_tracker.last_tick < owned_sync.tick {
                    last_sync_tracker.last_tick = owned_sync.tick.clone();
                    transform.translation = owned_sync.translation;
                    *kinematics = owned_sync.kinematics.clone();
                    // TODO: actual rollback
                    if PREDICT {
                        let inputs = ibuf.inputs_after_order(owned_sync.last_input_order);
                        for input in inputs {
                            // apply_input(
                            //     &mut context,
                            //     &input.input,
                            //     &mut transform,
                            //     shape,
                            //     controller,
                            //     &time,
                            //     entity,
                            //     &mut kinematics,
                            //     &mut grounded,
                            // );
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn recv_player_shot(
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

#[derive(Default)]
struct PressedShootLastFrame(bool);

fn read_input(
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
    if input.is_non_zero() {
        let order = ibuf.push_input(input.clone());
        let message = UnreliableMessageFromClient::Input(OrderedInput { input, order });
        let bytes = bincode::serialize(&message).unwrap();
        client.send_message(DefaultChannel::Unreliable, bytes);
        ibuf.prune(100);
    }
}

fn read_inputs(
    mut inputs: ResMut<ClientInputs>,
    reader: Res<server::MessageReaderOnServer>,
    client_netmap: Res<ClientNetworkObjectMap>,
) {
    for (client_id, msg) in reader.unreliable_messages() {
        if let UnreliableMessageFromClient::Input(ordered_input) = msg {
            if let Some(net_obj) = client_netmap.client_to_net_obj.get(client_id) {
                inputs.push_input(net_obj.clone(), ordered_input.clone(), *client_id);
            } else {
                warn!("Unknown client_id: {}", client_id);
            }
        }
    }

    inputs.prune(10);
}

fn tick_jump_cooldown(mut query: Query<&mut JumpCooldown>, time: Res<Time>) {
    for mut cooldown in query.iter_mut() {
        cooldown.timer.tick(time.delta());
    }
}

fn apply_inputs(
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &NetworkObject,
            &mut LastInputTracker,
            &KinematicCharacterController,
            &Collider,
            &mut Kinematics,
            &mut Grounded,
            &mut JumpCooldown,
        ),
        With<Player>,
    >,
    time: Res<Time>,
    mut inputs: ResMut<ClientInputs>,
    mut context: ResMut<RapierContext>,
    mut server: ResMut<RenetServer>,
) {
    let net_obj_inputs = inputs.pop_inputs();
    for (
        entity,
        mut transform,
        net_obj,
        mut last_input_tracker,
        controller,
        shape,
        mut kinematics,
        mut grounded,
        mut jump_cooldown,
    ) in query.iter_mut()
    {
        if let Some(input) = net_obj_inputs.get(net_obj) {
            if let Some(shot) = &input.input.shot {
                let Some(inputter) = inputs.get_client_id(net_obj) else {
                    error!("input without client");
                    continue;
                };
                let message =
                    UnreliableMessageFromServer::PlayerShot(net_obj.clone(), shot.clone());
                let bytes = bincode::serialize(&message).unwrap();
                server.broadcast_message_except(inputter, DefaultChannel::Unreliable, bytes);
            }
            apply_input(
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
            last_input_tracker.order = input.order;
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
        info!("jumping");
        kinematics.set_velocity(JUMP_KEY, JUMP_VELOCITY);
        jump_cooldown.timer.reset();
    } else if !grounded.was_grounded_last_tick() && grounded.grounded_this_tick() {
        info!("cancelling");
        kinematics.set_velocity(JUMP_KEY, Vec3::ZERO);
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
            info!("setting to zero");
            kinematics.set_velocity(JUMP_KEY, Vec3::ZERO);
        }
    }
}

fn rotate_player(
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

fn rubber_band_player_camera(
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

fn load_player(
    mut player_load: EventReader<PlayerWantsUpdates>,
    player_query: Query<(&NetworkObject, &Transform), With<Player>>,
    tick: Res<Tick>,
    mut server: ResMut<RenetServer>,
) {
    for load in player_load.read() {
        for (net_obj, transform) in player_query.iter() {
            let net_spawn = NetworkSpawn::Player(transform.clone());
            let message = ReliableMessageFromServer::Spawn(Spawn {
                net_obj: net_obj.clone(),
                tick: tick.clone(),
                net_spawn,
            });
            let bytes = bincode::serialize(&message).unwrap();
            server.send_message(load.client_id, DefaultChannel::ReliableUnordered, bytes);
        }
    }
}

#[derive(Debug, Component)]
pub struct PlayerCameraTarget;

#[derive(Debug, Component)]
pub struct PlayerCamera;

fn spawn_player_camera(mut commands: Commands, players: Query<Entity, Added<LocalPlayerTag>>) {
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

fn init_players(
    mut player_init: EventReader<PlayerNeedsInit>,
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for init in player_init.read() {
        let transform = Transform::from_xyz(0.0, 1.0, 0.0);
        commands.spawn((
            Player,
            init.net_obj.clone(),
            LastInputTracker::default(),
            KinematicCharacterController::default(),
            RigidBody::KinematicPositionBased,
            Collider::capsule_y(0.5, 0.25),
            TransformBundle::from_transform(transform),
            Kinematics::new().with_gravity(),
            Grounded::default(),
            JumpCooldown::new(),
        ));

        info!("sending player init");
        let message = ReliableMessageFromServer::InitPlayer(PlayerInit {
            net_obj: init.net_obj.clone(),
            transform,
            tick: tick.clone(),
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.send_message(init.client_id, DefaultChannel::ReliableUnordered, bytes);
    }
}
