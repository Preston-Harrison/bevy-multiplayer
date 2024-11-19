use std::collections::VecDeque;

use bevy::{input::mouse::MouseMotion, prelude::*, utils::HashMap};
use bevy_renet::renet::{DefaultChannel, RenetClient, RenetServer};
use serde::{Deserialize, Serialize};

use crate::{
    message::{
        client::{MessageReaderOnClient, OrderedInput, UnreliableMessageFromClient},
        server::{
            self, PlayerPositionSync, ReliableMessageFromServer, Spawn, UnreliableMessageFromServer,
        },
        spawn::NetworkSpawn,
    },
    server::ClientNetworkObjectMap,
    shared::{tick::Tick, ClientOnly, GameLogic, ServerOnly},
};

use super::{LastSyncTracker, NetworkObject};

#[derive(Resource)]
pub struct LocalPlayer(pub NetworkObject);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Input {
    direction: Vec3,
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

#[derive(Resource, Default)]
pub struct ClientInputs(HashMap<NetworkObject, Vec<OrderedInput>>);

impl ClientInputs {
    fn push_input(&mut self, net_obj: NetworkObject, input: OrderedInput) {
        self.0.entry(net_obj).or_default().push(input);
    }

    /// Removes and returns the lowest-order input for each `NetworkObject`.
    fn pop_inputs(&mut self) -> HashMap<NetworkObject, OrderedInput> {
        let mut inputs = HashMap::new();

        for (obj, ord_inputs) in self.0.iter_mut() {
            if let Some((min_index, _)) = ord_inputs
                .iter()
                .enumerate()
                .min_by_key(|(_, input)| input.order)
            {
                // Remove and store the lowest-order input
                let input = ord_inputs.remove(min_index);
                inputs.insert(obj.clone(), input);
            }
        }

        inputs
    }

    /// Ensures that each buffer in `ClientInputs` is no longer than `max_length`.
    /// Removes the input with the lowest order if the buffer exceeds the limit.
    fn prune(&mut self, max_length: usize) {
        for (_, ord_inputs) in self.0.iter_mut() {
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
}

#[derive(Component)]
pub struct Player;

#[derive(Component, Default)]
pub struct LastInputTracker {
    order: u64,
}

#[derive(Component)]
pub struct LocalPlayerTag;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClientInputs::default());
        app.insert_resource(InputBuffer::default());
        app.add_systems(
            FixedUpdate,
            (
                spawn_players.in_set(ClientOnly).in_set(GameLogic::Spawn),
                recv_player_data.in_set(ClientOnly).in_set(GameLogic::Sync),
                read_input.in_set(ClientOnly).in_set(GameLogic::Start),
                apply_inputs.in_set(ServerOnly).in_set(GameLogic::Game),
                read_inputs.in_set(ServerOnly).in_set(GameLogic::ReadInput),
                broadcast_player_data
                    .in_set(ServerOnly)
                    .in_set(GameLogic::Sync),
                broadcast_player_spawns
                    .in_set(ServerOnly)
                    .in_set(GameLogic::Sync),
            ),
        );
        app.add_systems(Update, rotate_player.in_set(ClientOnly));
    }
}

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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    reader: Res<MessageReaderOnClient>,
    local_player: Res<LocalPlayer>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Spawn(spawn) = msg else {
            continue;
        };
        if let NetworkSpawn::Player(transform) = spawn.net_spawn {
            println!("spawning player");
            let mut e = commands.spawn(Player);
            e.insert(PbrBundle {
                mesh: meshes.add(Sphere::default().mesh().ico(5).unwrap()),
                material: materials.add(Color::srgb(0.0, 1.0, 0.0)),
                transform,
                ..Default::default()
            })
            .insert(LastSyncTracker::<Transform>::new(spawn.tick.clone()))
            .insert(spawn.net_obj.clone());
            if spawn.net_obj == local_player.0 {
                e.insert(LocalPlayerTag);
            }
        }
    }
}

fn broadcast_player_data(
    query: Query<(&NetworkObject, &Transform, &LastInputTracker), With<Player>>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (obj, transform, last_input) in query.iter() {
        let message = UnreliableMessageFromServer::PlayerPositionSync(PlayerPositionSync {
            net_obj: obj.clone(),
            translation: transform.translation.clone(),
            tick: tick.clone(),
            last_input_order: last_input.order,
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::Unreliable, bytes);
    }
}

fn recv_player_data(
    reader: Res<MessageReaderOnClient>,
    mut query: Query<
        (
            &mut Transform,
            &NetworkObject,
            &mut LastSyncTracker<Transform>,
        ),
        With<Player>,
    >,
    local_player: Res<LocalPlayer>,
    ibuf: Res<InputBuffer>,
    time: Res<Time>,
) {
    for msg in reader.unreliable_messages() {
        let UnreliableMessageFromServer::PlayerPositionSync(pos_sync) = msg else {
            continue;
        };
        for (mut transform, obj, mut last_sync_tracker) in query.iter_mut() {
            if obj.id == pos_sync.net_obj.id && last_sync_tracker.last_tick < pos_sync.tick {
                last_sync_tracker.last_tick = pos_sync.tick.clone();
                transform.translation = pos_sync.translation;

                if pos_sync.net_obj.id == local_player.0.id {
                    let inputs = ibuf.inputs_after_order(pos_sync.last_input_order);
                    for input in inputs {
                        apply_input(&input.input, &mut transform, &time);
                    }
                }
            }
        }
    }
}

fn read_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut ibuf: ResMut<InputBuffer>,
    query: Query<&Transform, With<LocalPlayerTag>>,
    mut client: ResMut<RenetClient>,
) {
    if let Ok(player_transform) = query.get_single() {
        let mut local_direction = Vec3::ZERO;

        // Map WASD input to local directions
        if keyboard_input.pressed(KeyCode::KeyW) {
            local_direction -= Vec3::Z; // Forward
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            local_direction += Vec3::Z; // Backward
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            local_direction -= Vec3::X; // Left
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            local_direction += Vec3::X; // Right
        }

        if local_direction.length_squared() > 0.0 {
            local_direction = local_direction.normalize();
        }

        // Convert local direction to world space using the player's transform
        let world_direction = player_transform.rotation * local_direction;

        // Project the direction onto the XZ plane to prevent upward movement
        let world_direction_xz = Vec3::new(world_direction.x, 0.0, world_direction.z);

        // Normalize the XZ direction
        let final_direction = if world_direction_xz.length_squared() > 0.0 {
            world_direction_xz.normalize()
        } else {
            Vec3::ZERO
        };

        if final_direction.length_squared() > 0.0 {
            let input = Input {
                direction: final_direction,
            };
            let order = ibuf.push_input(input.clone());
            let message = UnreliableMessageFromClient::Input(OrderedInput { input, order });
            let bytes = bincode::serialize(&message).unwrap();
            client.send_message(DefaultChannel::Unreliable, bytes);
            ibuf.prune(100);
        }
    }
}

fn read_inputs(
    mut inputs: ResMut<ClientInputs>,
    reader: Res<server::MessageReaderOnServer>,
    client_netmap: Res<ClientNetworkObjectMap>,
) {
    for (client_id, msg) in reader.unreliable_messages() {
        if let UnreliableMessageFromClient::Input(ordered_input) = msg {
            // Map the client ID to the player's NetworkObject
            if let Some(net_obj) = client_netmap.0.get(client_id) {
                // Insert the input into the HashMap for the specific tick
                inputs.push_input(net_obj.clone(), ordered_input.clone());
            } else {
                warn!("Unknown client_id: {}", client_id);
            }
        }
    }

    inputs.prune(10);
}

fn apply_inputs(
    mut query: Query<(&mut Transform, &NetworkObject, &mut LastInputTracker), With<Player>>,
    time: Res<Time>,
    mut inputs: ResMut<ClientInputs>,
) {
    let net_obj_inputs = inputs.pop_inputs();
    for (mut transform, net_obj, mut last_input_tracker) in query.iter_mut() {
        if let Some(input) = net_obj_inputs.get(net_obj) {
            apply_input(&input.input, &mut transform, &time);
            last_input_tracker.order = input.order;
        }
    }
}

fn apply_input(input: &Input, transform: &mut Transform, time: &Time) {
    transform.translation += input.direction * 5.0 * time.delta_seconds();
}

fn rotate_player(
    mut mouse_motion: EventReader<MouseMotion>,
    mut player: Query<&mut Transform, With<LocalPlayerTag>>,
) {
    let Ok(mut transform) = player.get_single_mut() else {
        return;
    };
    for motion in mouse_motion.read() {
        let yaw = -motion.delta.x * 0.003;
        let pitch = -motion.delta.y * 0.002;
        // Order of rotations is important, see <https://gamedev.stackexchange.com/a/136175/103059>
        transform.rotate_y(yaw);
        transform.rotate_local_x(pitch);
    }
}
