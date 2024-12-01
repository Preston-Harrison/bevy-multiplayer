use std::collections::VecDeque;

use bevy::{
    ecs::query::{QueryData, QueryFilter as ECSQueryFilter},
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetClient};

use crate::{
    message::{
        client::{
            MessageReaderOnClient, OrderedInput, PlayerRotation, UnreliableMessageFromClient,
        },
        server::{
            OwnedPlayerSync, PlayerPositionSync, ReliableMessageFromServer,
            UnreliableMessageFromServer,
        },
        spawn::NetworkSpawn,
    },
    shared::{
        objects::{
            gizmo::spawn_bullet_tracer,
            grounded::Grounded,
            gun::{Gun, GunType, LocalPlayerGun},
            player::PlayerHead,
            LastSyncTracker, NetworkObject,
        },
        physics::apply_kinematics,
        GameLogic,
    },
    utils,
};

use super::{
    spawn::PlayerSpawnRequest, Input, LocalPlayer, LocalPlayerTag, Player, PlayerKinematics, Shot,
    ShotNothing, ShotPosition, ShotTarget, ShotType,
};

pub struct PlayerClientPlugin;

impl Plugin for PlayerClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(InputBuffer::default());
        app.insert_resource(TickBuffer::<PlayerSnapshot>::default());
        app.add_systems(
            FixedUpdate,
            (
                read_input.in_set(GameLogic::ReadInput),
                spawn_players.in_set(GameLogic::Spawn),
                recv_position_sync.in_set(GameLogic::Sync),
                sync_player_rotation.in_set(GameLogic::Sync),
                recv_player_shot.in_set(GameLogic::Sync),
                predict_movement.in_set(GameLogic::Game),
            ),
        );
        app.add_systems(
            Update,
            (
                rotate_player,
                rubber_band_player_camera.after(rotate_player),
                set_sprint_fov,
            ),
        );
    }
}

// For debugging
const RECONCILE: bool = true;
const PREDICT: bool = true;

/// Stores a queue of `T`, where the item at the back of the queue corresponds
/// to the most recent tick.
#[derive(Resource)]
pub struct TickBuffer<T> {
    items: VecDeque<T>,
}

impl<T> Default for TickBuffer<T> {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }
}

impl<T> TickBuffer<T> {
    fn push(&mut self, item: T) {
        self.items.push_back(item);
    }

    fn prune(&mut self, max_length: usize) {
        while self.items.len() > max_length {
            self.items.pop_front();
        }
    }

    fn get_latest(&self) -> Option<&T> {
        self.get_nth_from_latest(0)
    }

    fn get_nth_from_latest(&self, n: usize) -> Option<&T> {
        if n >= self.items.len() {
            None
        } else {
            self.items.get(self.items.len() - n - 1)
        }
    }
}

/// Stores a list of inputs (one for each tick), where the latest input is at
/// the back of `self.buffer`. The count is stored to order the inputs, and is
/// incremented by one when an input is pushed.
#[derive(Resource, Default)]
pub struct InputBuffer {
    buffer: TickBuffer<OrderedInput>,
    count: u64,
}

impl InputBuffer {
    fn push_input(&mut self, input: Input) -> u64 {
        self.count += 1;
        self.buffer.push(OrderedInput {
            input,
            order: self.count,
        });
        return self.count;
    }

    fn prune(&mut self, max_length: usize) {
        self.buffer.prune(max_length);
    }

    fn inputs_after_order(&self, order: u64) -> Vec<OrderedInput> {
        self.buffer
            .items
            .iter()
            .filter(|input| input.order > order)
            .cloned()
            .collect()
    }

    fn get_latest(&self) -> Option<&OrderedInput> {
        self.buffer.get_latest()
    }
}

/// Stores a queue of state that is needed to check if a rollback is necessary
/// when receiving a sync from the server.
type SnapshotHistory = TickBuffer<PlayerSnapshot>;

pub struct PlayerSnapshot {
    translation: Vec3,
    kinematics: PlayerKinematics,
}
impl PlayerSnapshot {
    /// Returns if a snapshot is different to an `OwnedPlayerSync` within a
    /// small threshold.
    fn is_different(&self, owned_sync: &OwnedPlayerSync) -> bool {
        if owned_sync.translation.distance(self.translation) > 0.1 {
            return true;
        }
        return self.kinematics.is_different(&owned_sync.kinematics);
    }
}

#[derive(Debug, Component)]
pub struct PlayerCameraTarget;

#[derive(Debug, Component)]
pub struct PlayerCamera;

/// Rotates the player based on mouse movement.
pub fn rotate_player(
    mut mouse_motion: EventReader<MouseMotion>,
    mut player: Query<(&mut Transform, Entity), (With<LocalPlayerTag>, Without<PlayerHead>)>,
    mut player_head: Query<(&mut Transform, &Parent), With<PlayerHead>>,
    q_windows: Query<&Window, With<PrimaryWindow>>,
) {
    let primary_window = q_windows.single();
    if primary_window.cursor.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok((mut player_t, entity)) = player.get_single_mut() else {
        return;
    };
    let mut player_head_t = None;
    for (head_t, parent) in player_head.iter_mut() {
        if parent.get() == entity {
            player_head_t = Some(head_t);
            break;
        }
    }
    let Some(mut player_head_t) = player_head_t else {
        return;
    };

    let pitch_min = -85_f32.to_radians();
    let pitch_max = 85_f32.to_radians();

    let mut current_pitch = player_head_t.rotation.to_euler(EulerRot::XYZ).0;

    for motion in mouse_motion.read() {
        let yaw = -motion.delta.x * 0.003;
        let pitch = -motion.delta.y * 0.002;

        player_t.rotate_y(yaw);
        current_pitch = (current_pitch + pitch).clamp(pitch_min, pitch_max);
        player_head_t.rotation = Quat::from_rotation_x(current_pitch);
    }
}

/// Interpolates the actual camera to the target camera position. This is useful
/// for avoiding camera jitter.
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

/// Stores if shoot was pressed last frame for semi-auto fire.
#[derive(Default)]
pub struct PressedShootLastFrame(bool);

#[derive(Default)]
pub struct IsFreecam(bool);

// TODO: wrangle lifetimes
// Couldn't figure out lifetimes so macro it is lol. This just runs a closure
// that returns (camera global transform, bullet point global transform).
macro_rules! get_cam_and_bullet_point_global_t {
    ($global_transform_query:expr, $gun:expr, $cam_entity:expr) => {
        (|| {
            let cam_global_t = match $global_transform_query.get($cam_entity) {
                Ok(transform) => transform,
                Err(_) => return None,
            };

            if let Some(bullet_point) = $gun.bullet_point {
                if let Ok(bullet_point_global_t) = $global_transform_query.get(bullet_point) {
                    return Some((cam_global_t, bullet_point_global_t, $gun.gun_type.clone()));
                }
            }
            None
        })()
    };
}

/// Reads input from the keyboard and mouse and stores it in a buffer. Doesn't
/// include rotation, like looking around.
pub fn read_input(
    mut pressed_shoot: Local<PressedShootLastFrame>,
    mut freecam: Local<IsFreecam>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut ibuf: ResMut<InputBuffer>,
    local_player: Query<(&Transform, Entity), With<LocalPlayerTag>>,
    camera: Query<Entity, With<PlayerCamera>>,
    mut local_gun: Query<&mut Gun, With<LocalPlayerGun>>,
    global_transform_query: Query<&GlobalTransform>,
    net_objs: Query<(&NetworkObject, &Transform)>,
    mut client: ResMut<RenetClient>,
    context: Res<RapierContext>,
    mut commands: Commands,
    time: Res<Time>,
) {
    let Ok((player_transform, entity)) = local_player.get_single() else {
        error!("no player found when reading input");
        return;
    };
    let local_direction = get_direction(&keyboard_input).normalize_or_zero();
    if keyboard_input.just_pressed(KeyCode::KeyQ) {
        freecam.0 = !freecam.0;
    }

    let pressed_shoot_last_frame = pressed_shoot.0;
    pressed_shoot.0 = mouse_input.pressed(MouseButton::Left);
    let Ok(cam_entity) = camera.get_single() else {
        return;
    };

    let local_gun = local_gun.get_single_mut().ok();
    let shot = local_gun.and_then(|mut gun| {
        let shoot = (!pressed_shoot_last_frame || gun.gun_type.is_full_auto()) && pressed_shoot.0;
        if shoot && gun.try_shoot(time.elapsed()) {
            match get_cam_and_bullet_point_global_t!(&global_transform_query, gun, cam_entity) {
                Some((cam_global_t, bullet_point_global_t, gun_type)) => get_shot(
                    &mut commands,
                    &context,
                    entity,
                    cam_global_t,
                    bullet_point_global_t,
                    &net_objs,
                    gun_type,
                ),
                None => None,
            }
        } else {
            None
        }
    });

    let world_direction = player_transform.rotation * local_direction;
    let world_direction_xz = Vec3::new(world_direction.x, 0.0, world_direction.z);
    let input = Input {
        direction: world_direction_xz.normalize_or_zero(),
        sprint: keyboard_input.pressed(KeyCode::ShiftLeft),
        jump: keyboard_input.pressed(KeyCode::Space),
        shot,
    };
    let order = ibuf.push_input(input.clone());
    ibuf.prune(100);

    // TODO: figure out a way to send a zero valued input (or interpret lack of an
    // input using input.order) in a more effecient way.
    let message = UnreliableMessageFromClient::Input(OrderedInput { input, order });
    let bytes = bincode::serialize(&message).unwrap();
    client.send_message(DefaultChannel::Unreliable, bytes);
}

/// Gets a non-normalized vector from WASD input.
fn get_direction(keyboard_input: &ButtonInput<KeyCode>) -> Vec3 {
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
    local_direction
}

/// Spawns a raycast towards a target and returns a `Shot` if there's a hit.
/// First raycasts from the camera to a target, then returns the shot from the
/// bullet point camera target. This is so that the bullet goes where the player
/// is looking, but still comes out of the gun.
fn get_shot(
    commands: &mut Commands,
    context: &RapierContext,
    shooter: Entity,
    camera: &GlobalTransform,
    bullet_point: &GlobalTransform,
    net_objs: &Query<(&NetworkObject, &Transform)>,
    gun_type: GunType,
) -> Option<Shot> {
    let bullet_range = gun_type.range();

    // Cast ray from camera to find where the bullet should go.
    let cam_ray_pos = camera.translation();
    // TODO: tune or calculate this number
    // The number exists so that the bullet pretty much aligns with the camera
    // when shooting nothing (at max range of bullet).
    let cam_range = bullet_range + 1.0;
    let cam_ray_dir = *camera.forward();
    let cam_raycast = context.cast_ray(
        cam_ray_pos,
        cam_ray_dir,
        cam_range,
        false,
        QueryFilter::default().exclude_collider(shooter),
    );
    let cam_hit_point = cam_ray_pos
        + match cam_raycast {
            Some((_, toi)) => cam_ray_dir * toi,
            None => cam_ray_dir * cam_range,
        };

    // Cast ray from the bullet to the camera hit point.
    let bullet_ray_pos = bullet_point.translation();
    let bullet_ray_dir = (-bullet_ray_pos + cam_hit_point).normalize();
    let raycast = context.cast_ray(
        bullet_ray_pos,
        bullet_ray_dir,
        bullet_range,
        false,
        QueryFilter::default().exclude_collider(shooter),
    );
    let shot_type = match raycast {
        Some((entity, toi)) => {
            spawn_bullet_tracer(commands, bullet_ray_pos, bullet_ray_dir, toi, true);
            let impact_point = bullet_ray_pos + (bullet_ray_dir * toi);
            match net_objs.get(entity).ok() {
                Some((obj, transform)) => {
                    let relative_position = impact_point - transform.translation;
                    Some(ShotType::ShotTarget(ShotTarget {
                        target: obj.clone(),
                        relative_position,
                    }))
                }
                None => Some(ShotType::ShotPosition(ShotPosition {
                    position: impact_point,
                })),
            }
        }
        None => {
            spawn_bullet_tracer(
                commands,
                bullet_ray_pos,
                bullet_ray_dir,
                bullet_range,
                false,
            );
            Some(ShotType::ShotNothing(ShotNothing {
                vector: bullet_ray_dir * bullet_range,
            }))
        }
    };
    shot_type.map(|shot_type| Shot {
        shot_type,
        gun_type,
    })
}

/// Receives `Shot` messages from the server and spawns a visual.
pub fn recv_player_shot(
    reader: Res<MessageReaderOnClient>,
    mut commands: Commands,
    player_query: Query<(&NetworkObject, Entity), With<Player>>,
    player_head_query: Query<(&Parent, Entity), With<PlayerHead>>,
    gun_query: Query<(&Parent, &Gun)>,
    global_transform_query: Query<&GlobalTransform>,
    net_obj_query: Query<(&NetworkObject, &Transform)>,
) {
    for msg in reader.unreliable_messages() {
        let UnreliableMessageFromServer::PlayerShot(shooter, shot) = msg else {
            continue;
        };

        // TODO: fix this hellish code
        let mut shooter_pos = None;
        for (net_obj, player_entity) in player_query.iter() {
            if net_obj == shooter {
                for (head_parent, head_entity) in player_head_query.iter() {
                    if head_parent.get() == player_entity {
                        for (gun_parent, gun) in gun_query.iter() {
                            if gun_parent.get() == head_entity {
                                if let Some(bullet_point) = gun.bullet_point {
                                    shooter_pos = global_transform_query
                                        .get(bullet_point)
                                        .ok()
                                        .map(|v| v.translation());
                                    if shooter_pos.is_none() {
                                        warn!("bullet point entity is in gun, but transform not found");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let Some(shooter_pos) = shooter_pos else {
            warn!("tried to shoot but no shooter position");
            dbg!(player_query.iter().collect::<Vec<_>>());
            dbg!(player_head_query.iter().collect::<Vec<_>>());
            dbg!(gun_query.iter().collect::<Vec<_>>());
            continue;
        };

        match &shot.shot_type {
            ShotType::ShotNothing(shot) => match shot.vector.try_normalize() {
                Some(vector) => {
                    spawn_bullet_tracer(
                        &mut commands,
                        shooter_pos,
                        vector,
                        shot.vector.length(),
                        false,
                    );
                }
                _ => warn!("got zero valued shot vector"),
            },
            ShotType::ShotPosition(shot) => spawn_bullet_tracer(
                &mut commands,
                shooter_pos,
                -shooter_pos + shot.position,
                (-shooter_pos + shot.position).length(),
                true,
            ),
            ShotType::ShotTarget(shot) => {
                let target_pos = net_obj_query
                    .iter()
                    .find(|(obj, _)| **obj == shot.target)
                    .map(|(_, t)| t);
                let Some(target_pos) = target_pos else {
                    error!("tried to recv shot for non existant: {:?}", shot);
                    continue;
                };
                let target_shot_pos = target_pos.translation + shot.relative_position;
                let ray = target_shot_pos - shooter_pos;
                spawn_bullet_tracer(
                    &mut commands,
                    shooter_pos,
                    ray.normalize(),
                    ray.length(),
                    true,
                );
            }
        }
    }
}

/// Handles `Spawn` events from the server and spawns players.
pub fn spawn_players(
    reader: Res<MessageReaderOnClient>,
    local_player: Res<LocalPlayer>,
    mut player_spawn_requests: EventWriter<PlayerSpawnRequest>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Spawn(spawn) = msg else {
            continue;
        };
        if spawn.net_obj == local_player.0 {
            continue;
        };
        if let NetworkSpawn::Player(transform) = spawn.net_spawn {
            player_spawn_requests.send(PlayerSpawnRequest::Remote(
                transform,
                spawn.net_obj.clone(),
                spawn.tick.clone(),
            ));
        }
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct LocalPlayerQueryForSync {
    entity: Entity,
    transform: &'static mut Transform,
    net_obj: &'static NetworkObject,
    last_sync_tracker: &'static mut LastSyncTracker<Transform>,
    collider: &'static Collider,
    controller: &'static KinematicCharacterController,
    grounded: &'static mut Grounded,
    player: &'static mut Player,
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct NonLocalPlayers {
    entity: Entity,
    transform: &'static mut Transform,
    net_obj: &'static NetworkObject,
    last_sync_tracker: &'static mut LastSyncTracker<Transform>,
}

/// Receives player synchronization events. For `PlayerPositionSync`, this just
/// sets the new position with `sync_nonlocal`. For `OwnedPlayerSync`, this performs
/// rollback with `check_and_rollback`.
fn recv_position_sync(
    reader: Res<MessageReaderOnClient>,
    mut nonlocal_players: Query<NonLocalPlayers, (With<Player>, Without<LocalPlayerTag>)>,
    mut player_head_query: Query<(&mut Transform, &Parent), (With<PlayerHead>, Without<Player>)>,
    mut local_player: Query<LocalPlayerQueryForSync, LocalPlayerFilter>,
    ibuf: Res<InputBuffer>,
    mut history: ResMut<SnapshotHistory>,
    time: Res<Time>,
    mut context: ResMut<RapierContext>,
) {
    for msg in reader.unreliable_messages() {
        match msg {
            UnreliableMessageFromServer::PlayerPositionSync(pos_sync) => {
                sync_nonlocal(&mut nonlocal_players, &mut player_head_query, pos_sync);
            }
            UnreliableMessageFromServer::OwnedPlayerSync(owned_sync) => {
                let Ok(mut record) = local_player.get_single_mut() else {
                    continue;
                };
                let is_local = *record.net_obj == owned_sync.net_obj;
                let is_most_recent = record.last_sync_tracker.last_tick < owned_sync.tick;
                if is_local && is_most_recent {
                    check_and_rollback(
                        &mut context,
                        &mut record,
                        owned_sync,
                        &ibuf,
                        &mut history,
                        &time,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Applies synchronization for non-local players.
fn sync_nonlocal(
    nonlocal_players: &mut Query<NonLocalPlayers, (With<Player>, Without<LocalPlayerTag>)>,
    player_head_query: &mut Query<(&mut Transform, &Parent), (With<PlayerHead>, Without<Player>)>,
    pos_sync: &PlayerPositionSync,
) {
    for mut player in nonlocal_players.iter_mut() {
        let is_same_player = *player.net_obj == pos_sync.net_obj;
        let is_most_recent = player.last_sync_tracker.last_tick < pos_sync.tick;
        if is_same_player && is_most_recent {
            player.last_sync_tracker.last_tick = pos_sync.tick.clone();
            player.transform.translation = pos_sync.translation;
            utils::transform::set_body_rotation_pitch(
                &mut player.transform,
                pos_sync.body_rotation,
            );
            for (mut head_t, head_parent) in player_head_query.iter_mut() {
                if head_parent.get() == player.entity {
                    utils::transform::set_head_rotation_yaw(&mut head_t, pos_sync.head_rotation);
                }
            }
        }
    }
}

/// Applies an owned player sync to the local player. Performs rollback and
/// reconciliation if there's a difference between the sync and the local player
/// snapshot.
fn check_and_rollback(
    context: &mut RapierContext,
    record: &mut LocalPlayerQueryForSyncItem,
    owned_sync: &OwnedPlayerSync,
    ibuf: &InputBuffer,
    history: &mut SnapshotHistory,
    time: &Time,
) {
    record.last_sync_tracker.last_tick = owned_sync.tick.clone();
    let mut inputs = ibuf.inputs_after_order(owned_sync.last_input_order);
    inputs.pop(); // Current frame input, will be processed later.
    if inputs.len() == 0 {
        return;
    }
    let snapshot = history.get_nth_from_latest(inputs.len());
    let should_reconcile = match snapshot {
        Some(snapshot) => snapshot.is_different(owned_sync),
        None => false,
    };
    if !should_reconcile || !RECONCILE {
        return;
    }
    record.last_sync_tracker.last_tick = owned_sync.tick.clone();
    record.transform.translation = owned_sync.translation;
    record.player.kinematics = owned_sync.kinematics.clone();
    record
        .player
        .jump_cooldown_timer
        .set_elapsed(owned_sync.jump_cooldown_elapsed);
    for input in inputs {
        super::apply_input(
            context,
            &input.input,
            &mut record.transform,
            record.collider,
            record.controller,
            &time,
            record.entity,
            &mut record.player,
            &mut record.grounded,
        );
        apply_kinematics(
            context,
            record.entity,
            record.controller,
            &mut record.transform,
            record.collider,
            record.player.kinematics.get_velocity(),
            Some(&mut record.grounded),
            time.delta_seconds(),
        );
        record.player.jump_cooldown_timer.tick(time.delta());
        history.push(PlayerSnapshot {
            translation: record.transform.translation,
            kinematics: record.player.kinematics.clone(),
        });
        history.prune(100);
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct LocalPlayerQuery {
    entity: Entity,
    transform: &'static mut Transform,
    collider: &'static Collider,
    controller: &'static KinematicCharacterController,
    grounded: &'static mut Grounded,
    player: &'static mut Player,
}

#[derive(ECSQueryFilter)]
pub struct LocalPlayerFilter {
    _filter: (With<Player>, With<LocalPlayerTag>),
}

/// Grabs the most recent input and applies it locally. After applying the input,
/// a snapshot of the player is stored in the snapshot history.
pub fn predict_movement(
    mut context: ResMut<RapierContext>,
    ibuf: Res<InputBuffer>,
    mut snapshots: ResMut<SnapshotHistory>,
    mut local_player: Query<LocalPlayerQuery, LocalPlayerFilter>,
    time: Res<Time>,
) {
    if !PREDICT {
        return;
    }
    let Ok(mut local_player) = local_player.get_single_mut() else {
        warn!("no local player");
        return;
    };
    let Some(input) = ibuf.get_latest() else {
        warn!("no latest input");
        return;
    };

    super::apply_input(
        &mut context,
        &input.input,
        &mut local_player.transform,
        local_player.collider,
        local_player.controller,
        &time,
        local_player.entity,
        &mut local_player.player,
        &mut local_player.grounded,
    );
    snapshots.push(PlayerSnapshot {
        translation: local_player.transform.translation,
        kinematics: local_player.player.kinematics.clone(),
    });
    snapshots.prune(100);
}

fn sync_player_rotation(
    mut client: ResMut<RenetClient>,
    player_query: Query<(Entity, &Transform), With<LocalPlayerTag>>,
    player_head_query: Query<(&Transform, &Parent), With<PlayerHead>>,
) {
    let Ok((player_entity, player_t)) = player_query.get_single() else {
        return;
    };

    for (head_t, head_parent) in player_head_query.iter() {
        if head_parent.get() == player_entity {
            let message = UnreliableMessageFromClient::PlayerRotation(PlayerRotation {
                body: utils::transform::get_body_rotation_pitch(player_t),
                head: utils::transform::get_head_rotation_yaw(head_t),
            });
            let bytes = bincode::serialize(&message).unwrap();
            client.send_message(DefaultChannel::Unreliable, bytes);
            break;
        }
    }
}

fn set_sprint_fov(ibuf: Res<InputBuffer>, mut proj: Query<&mut Projection, With<PlayerCamera>>) {
    let Some(input) = ibuf.get_latest() else {
        return;
    };

    let Ok(mut proj) = proj.get_single_mut() else {
        return;
    };

    if let Projection::Perspective(ref mut perspective) = *proj {
        if input.input.sprint {
            perspective.fov = perspective.fov.lerp(70f32.to_radians(), 0.1);
        } else {
            perspective.fov = perspective.fov.lerp(60f32.to_radians(), 0.1);
        }
    }
}
