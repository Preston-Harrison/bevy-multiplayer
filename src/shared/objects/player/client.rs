use std::collections::VecDeque;

use bevy::{
    color::palettes::tailwind::{GREEN_500, YELLOW_500},
    ecs::query::{QueryData, QueryFilter as ECSQueryFilter},
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetClient};

use crate::{
    message::{
        client::{MessageReaderOnClient, OrderedInput, UnreliableMessageFromClient},
        server::{OwnedPlayerSync, ReliableMessageFromServer, UnreliableMessageFromServer},
        spawn::NetworkSpawn,
    },
    shared::{
        console::ConsoleMessage,
        objects::{
            gizmo::spawn_raycast_visual, grounded::Grounded, LastSyncTracker, NetworkObject,
        },
        physics::apply_kinematics,
        GameLogic,
    },
};

use super::{
    Input, JumpCooldown, LocalPlayer, LocalPlayerTag, Player, PlayerKinematics, Shot, ShotNothing,
    ShotTarget,
};

pub struct PlayerClientPlugin;

impl Plugin for PlayerClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(InputBuffer::default());
        app.insert_resource(TickBuffer::<PlayerSnapshot>::default());
        app.add_systems(
            FixedUpdate,
            (
                spawn_player_camera, // Runs when LocalPlayerTag is added
                read_input.in_set(GameLogic::ReadInput),
                spawn_players.in_set(GameLogic::Spawn),
                recv_position_sync.in_set(GameLogic::Sync),
                recv_player_shot.in_set(GameLogic::Sync),
                predict_movement.in_set(GameLogic::Game),
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

// For debugging
const RECONCILE: bool = true;
const PREDICT: bool = true;

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

type SnapshotHistory = TickBuffer<PlayerSnapshot>;

pub struct PlayerSnapshot {
    translation: Vec3,
    kinematics: PlayerKinematics,
}
impl PlayerSnapshot {
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

pub fn spawn_player_camera(mut commands: Commands, players: Query<Entity, Added<LocalPlayerTag>>) {
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
    ibuf.prune(100);

    // TODO: figure out a way to send a zero valued input (or interpret lack of an
    // input using input.order) in a more effecient way.
    let message = UnreliableMessageFromClient::Input(OrderedInput { input, order });
    let bytes = bincode::serialize(&message).unwrap();
    client.send_message(DefaultChannel::Unreliable, bytes);
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
    velocity: &'static mut PlayerKinematics,
    jump_cooldown: &'static mut JumpCooldown,
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
    mut local_player: Query<LocalPlayerQueryForSync, LocalPlayerFilter>,
    ibuf: Res<InputBuffer>,
    mut history: ResMut<SnapshotHistory>,
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
                let Ok(mut record) = local_player.get_single_mut() else {
                    continue;
                };
                let is_local = *record.net_obj == owned_sync.net_obj;
                let is_most_recent = record.last_sync_tracker.last_tick < owned_sync.tick;
                if is_local && is_most_recent {
                    record.last_sync_tracker.last_tick = owned_sync.tick.clone();
                    let mut inputs = ibuf.inputs_after_order(owned_sync.last_input_order);
                    inputs.pop(); // Current frame input, will be processed later.
                    if inputs.len() == 0 {
                        continue;
                    }
                    let snapshot = history.get_nth_from_latest(inputs.len());
                    let should_reconcile = match snapshot {
                        Some(snapshot) => snapshot.is_different(owned_sync),
                        None => false,
                    };
                    if !should_reconcile || !RECONCILE {
                        continue;
                    }
                    if snapshot
                        .unwrap()
                        .kinematics
                        .is_different(&owned_sync.kinematics)
                    {
                        warn!("rolling back, kinematics differ");
                    } else {
                        warn!("rolling back, translations differ");
                    }
                    record.last_sync_tracker.last_tick = owned_sync.tick.clone();
                    record.transform.translation = owned_sync.translation;
                    *record.velocity = owned_sync.kinematics.clone();
                    record
                        .jump_cooldown
                        .timer
                        .set_elapsed(owned_sync.jump_cooldown_elapsed);
                    // BOOKMARK: rollback
                    for input in inputs {
                        super::apply_input(
                            &mut context,
                            &input.input,
                            &mut record.transform,
                            record.collider,
                            record.controller,
                            &time,
                            record.entity,
                            &mut record.velocity,
                            &mut record.grounded,
                            &mut record.jump_cooldown,
                        );
                        apply_kinematics(
                            &mut context,
                            record.entity,
                            record.controller,
                            &mut record.transform,
                            record.collider,
                            record.velocity.get_velocity(),
                            Some(&mut record.grounded),
                            time.delta_seconds(),
                        );
                        record.jump_cooldown.timer.tick(time.delta());
                        history.push(PlayerSnapshot {
                            translation: record.transform.translation,
                            kinematics: record.velocity.clone(),
                        });
                        history.prune(100);
                    }
                }
            }
            _ => {}
        }
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
    kinematics: &'static mut PlayerKinematics,
    jump_cooldown: &'static mut JumpCooldown,
}

#[derive(ECSQueryFilter)]
pub struct LocalPlayerFilter {
    _filter: (With<Player>, With<LocalPlayerTag>),
}

pub fn predict_movement(
    mut context: ResMut<RapierContext>,
    ibuf: Res<InputBuffer>,
    mut snapshots: ResMut<TickBuffer<PlayerSnapshot>>,
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
        &mut local_player.kinematics,
        &mut local_player.grounded,
        &mut local_player.jump_cooldown,
    );
    snapshots.push(PlayerSnapshot {
        translation: local_player.transform.translation,
        kinematics: local_player.kinematics.clone(),
    });
    snapshots.prune(100);
}
