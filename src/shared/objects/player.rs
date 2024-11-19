use bevy::{input::mouse::MouseMotion, prelude::*, utils::HashMap};
use bevy_renet::renet::{DefaultChannel, RenetClient, RenetServer};
use serde::{Deserialize, Serialize};

use crate::{
    message::{
        client::{MessageReaderOnClient, UnreliableMessageFromClient},
        server::{self, ReliableMessageFromServer, UnreliableMessageFromServer},
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

/// `input_buffer.0[0]` is the newest input.
#[derive(Resource, Default)]
pub struct InputBuffer(HashMap<Tick, Input>);

#[derive(Resource, Default)]
pub struct ClientInputs(HashMap<Tick, HashMap<NetworkObject, Input>>);

#[derive(Component)]
pub struct Player;

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
) {
    for (network_obj, transform) in query.iter() {
        let network_spawn = NetworkSpawn::Player(transform.clone());
        let message = ReliableMessageFromServer::Spawn(network_obj.clone(), network_spawn);
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
    tick: Res<Tick>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Spawn(network_obj, network_spawn) = msg else {
            continue;
        };
        if let NetworkSpawn::Player(transform) = network_spawn {
            println!("spawning player");
            let mut e = commands.spawn(Player);
            e.insert(PbrBundle {
                mesh: meshes.add(Sphere::default().mesh().ico(5).unwrap()),
                material: materials.add(Color::srgb(0.0, 1.0, 0.0)),
                transform: *transform,
                ..Default::default()
            })
            .insert(LastSyncTracker::<Transform>::new(tick.clone()))
            .insert(network_obj.clone());
            if *network_obj == local_player.0 {
                e.insert(LocalPlayerTag);
            }
        }
    }
}

fn broadcast_player_data(
    query: Query<(&NetworkObject, &Transform), With<Player>>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (obj, transform) in query.iter() {
        let message = UnreliableMessageFromServer::PositionSync(
            obj.clone(),
            transform.translation.clone(),
            tick.clone(),
        );
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
    tick: Res<Tick>,
    ibuf: Res<InputBuffer>,
    time: Res<Time>,
) {
    for msg in reader.unreliable_messages() {
        let UnreliableMessageFromServer::PositionSync(net_obj, net_translation, sync_tick) = msg
        else {
            continue;
        };
        for (mut transform, obj, mut last_sync_tracker) in query.iter_mut() {
            if obj.id == net_obj.id && last_sync_tracker.last_tick < *sync_tick {
                last_sync_tracker.last_tick = sync_tick.clone();
                transform.translation = *net_translation;
                if net_obj.id != local_player.0.id {
                    continue;
                }

                if tick.get() <= last_sync_tracker.last_tick.get() {
                    warn!(
                        "got player sync tick in the future. current={}, sync={}",
                        tick.get(),
                        last_sync_tracker.last_tick.get()
                    );
                    continue;
                }

                let ticks = last_sync_tracker.last_tick.get()..=tick.get();
                for i in ticks {
                    if let Some(input) = &ibuf.0.get(&Tick::new(i)) {
                        apply_input(input, &mut transform, &time);
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
    tick: Res<Tick>,
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
            let message = UnreliableMessageFromClient::Input(
                Input {
                    direction: final_direction,
                },
                tick.clone(),
            );
            let bytes = bincode::serialize(&message).unwrap();
            client.send_message(DefaultChannel::Unreliable, bytes);
            ibuf.0.insert(
                tick.clone(),
                Input {
                    direction: final_direction,
                },
            );
        }

        let max_ticks_to_keep = 100;
        let oldest_tick_to_keep = tick.get().saturating_sub(max_ticks_to_keep);
        ibuf.0.retain(|t, _| t.get() >= oldest_tick_to_keep);
    }
}

fn read_inputs(
    mut inputs: ResMut<ClientInputs>,
    reader: Res<server::MessageReaderOnServer>,
    client_netmap: Res<ClientNetworkObjectMap>,
    tick: Res<Tick>,
) {
    for (client_id, msg) in reader.unreliable_messages() {
        if let UnreliableMessageFromClient::Input(input, input_tick) = msg {
            // Map the client ID to the player's NetworkObject
            if let Some(net_obj) = client_netmap.0.get(client_id) {
                // Insert the input into the HashMap for the specific tick
                inputs
                    .0
                    .entry(input_tick.clone())
                    .or_insert_with(HashMap::new)
                    .insert(net_obj.clone(), input.clone());
            } else {
                warn!("Unknown client_id: {}", client_id);
            }
        }
    }

    // Optionally, clean up old ticks beyond a certain limit
    let max_ticks_to_keep = 10;
    let oldest_tick_to_keep = tick.get().saturating_sub(max_ticks_to_keep);
    inputs.0.retain(|t, _| t.get() >= oldest_tick_to_keep);
}

fn apply_inputs(
    mut query: Query<(&mut Transform, &NetworkObject), With<Player>>,
    time: Res<Time>,
    inputs: Res<ClientInputs>,
    tick: Res<Tick>,
) {
    for (mut transform, net_obj) in query.iter_mut() {
        if let Some(input) = inputs.0.get(&*tick).and_then(|v| v.get(net_obj)) {
            apply_input(input, &mut transform, &time);
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
