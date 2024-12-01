use bevy::{ecs::query::QueryData, prelude::*, utils::HashMap};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};
use rand::Rng;

use crate::{
    message::{
        client::{OrderedInput, UnreliableMessageFromClient},
        server::{
            self, OwnedPlayerSync, PlayerInit, PlayerPositionSync, ReliableMessageFromServer,
            Spawn, UnreliableMessageFromServer,
        },
        spawn::NetworkSpawn,
    },
    server::{ClientNetworkObjectMap, PlayerNeedsInit, PlayerWantsUpdates},
    shared::{
        objects::{grounded::Grounded, health::Health, player::Player, NetworkObject},
        tick::Tick,
        GameLogic,
    },
    utils,
};

use super::{spawn::PlayerSpawnRequest, PlayerHead, ShotType};

pub struct PlayerServerPlugin;

impl Plugin for PlayerServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClientInputs::default());
        app.add_systems(
            FixedUpdate,
            (
                handle_player_looking_at.in_set(GameLogic::Game),
                apply_inputs.in_set(GameLogic::Game),
                read_inputs.in_set(GameLogic::ReadInput),
                broadcast_player_data.in_set(GameLogic::Sync),
                broadcast_player_spawns.in_set(GameLogic::Sync),
                load_player.in_set(GameLogic::Sync),
                init_players.in_set(GameLogic::Spawn),
            ),
        );
    }
}

/// Tracks the order of the most recent processed input for a player. The input
/// for the player should be processed by the time this is updated.
#[derive(Component, Default)]
pub struct LastInputTracker {
    order: u64,
}

/// Stores a mapping of player network objects to a list of unprocessed inputs.
/// Also stores a mapping of network objects to their client id.
///
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

/// Spawns a new player when a `PlayerNeedsInit` event is received.
pub fn init_players(
    mut player_init: EventReader<PlayerNeedsInit>,
    mut player_spawn_reqs: EventWriter<PlayerSpawnRequest>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for init in player_init.read() {
        let mut rng = rand::thread_rng();
        let transform =
            Transform::from_xyz(rng.gen_range(-30.0..30.0), 0.0, rng.gen_range(-30.0..30.0));
        player_spawn_reqs.send(PlayerSpawnRequest::Server(transform, init.net_obj.clone()));

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

/// Broadcasts a player spawn event whenever a new player is added.
pub fn broadcast_player_spawns(
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

/// Sends a `PlayerPositionSync` to everyone player except the player whose position
/// it is. Sends an `OwnedPlayerSync` to the player who owns the position.
pub fn broadcast_player_data(
    player_query: Query<(
        &NetworkObject,
        &Transform,
        &LastInputTracker,
        &Player,
        Entity,
    )>,
    player_head_query: Query<(&Transform, &Parent), With<PlayerHead>>,
    client_netmap: Res<ClientNetworkObjectMap>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (obj, transform, input_tracker, player, player_entity) in player_query.iter() {
        let Some(client_id) = client_netmap.net_obj_to_client.get(obj) else {
            warn!("no client id for player obj in broadcast_player_data");
            continue;
        };

        let mut head_rotation = 0.0;
        for (head_t, head_parent) in player_head_query.iter() {
            if head_parent.get() == player_entity {
                head_rotation = utils::transform::get_head_rotation_yaw(head_t);
            }
        }

        let message = UnreliableMessageFromServer::PlayerPositionSync(PlayerPositionSync {
            net_obj: obj.clone(),
            translation: transform.translation.clone(),
            tick: tick.clone(),
            body_rotation: utils::transform::get_body_rotation_pitch(transform),
            head_rotation,
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message_except(*client_id, DefaultChannel::Unreliable, bytes);

        let message = UnreliableMessageFromServer::OwnedPlayerSync(OwnedPlayerSync {
            net_obj: obj.clone(),
            translation: transform.translation.clone(),
            tick: tick.clone(),
            kinematics: player.kinematics.clone(),
            last_input_order: input_tracker.order,
            jump_cooldown_elapsed: player.jump_cooldown_timer.elapsed(),
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.send_message(*client_id, DefaultChannel::Unreliable, bytes);
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct InputQuery {
    entity: Entity,
    transform: &'static mut Transform,
    net_obj: &'static NetworkObject,
    last_input_tracker: &'static mut LastInputTracker,
    controller: &'static KinematicCharacterController,
    collider: &'static Collider,
    grounded: &'static mut Grounded,
    player: &'static mut Player,
}

/// Grabs the most recent input for each player and applies it using `apply_input`.
pub fn apply_inputs(
    mut health: Query<(&NetworkObject, &mut Health)>,
    mut query: Query<InputQuery, With<Player>>,
    time: Res<Time>,
    mut inputs: ResMut<ClientInputs>,
    mut context: ResMut<RapierContext>,
    mut server: ResMut<RenetServer>,
) {
    let net_obj_inputs = inputs.pop_inputs();
    for mut item in query.iter_mut() {
        if let Some(input) = net_obj_inputs.get(item.net_obj) {
            if let Some(shot) = &input.input.shot {
                let Some(inputter) = inputs.get_client_id(item.net_obj) else {
                    error!("input without client");
                    continue;
                };
                let message =
                    UnreliableMessageFromServer::PlayerShot(item.net_obj.clone(), shot.clone());
                let bytes = bincode::serialize(&message).unwrap();
                server.broadcast_message_except(inputter, DefaultChannel::Unreliable, bytes);
                if let ShotType::ShotTarget(target) = &shot.shot_type {
                    for (net_obj, mut health) in health.iter_mut() {
                        if *net_obj == target.target {
                            health.current = (health.current - shot.gun_type.damage()).max(0.0);
                        }
                    }
                }
            }
            super::apply_input(
                &mut context,
                &input.input,
                &mut item.transform,
                item.collider,
                item.controller,
                &time,
                item.entity,
                &mut item.player,
                &mut item.grounded,
            );
            item.last_input_tracker.order = input.order;
        }
    }
}

/// Listens for Input messages from clients and stores them in a buffer.
pub fn read_inputs(
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

pub fn handle_player_looking_at(
    mut player_query: Query<
        (&mut Transform, &NetworkObject, Entity),
        (With<Player>, Without<PlayerHead>),
    >,
    mut player_head: Query<(&mut Transform, &Parent), With<PlayerHead>>,
    reader: Res<server::MessageReaderOnServer>,
    client_netmap: Res<ClientNetworkObjectMap>,
) {
    for (client_id, msg) in reader.unreliable_messages() {
        if let UnreliableMessageFromClient::PlayerRotation(rotation) = msg {
            if let Some(net_obj) = client_netmap.client_to_net_obj.get(client_id) {
                for (mut transform, obj, player_entity) in player_query.iter_mut() {
                    if obj != net_obj {
                        continue;
                    }
                    utils::transform::set_body_rotation_pitch(&mut transform, rotation.body);
                    for (mut head_t, head_parent) in player_head.iter_mut() {
                        if head_parent.get() == player_entity {
                            utils::transform::set_head_rotation_yaw(&mut head_t, rotation.head);
                        }
                    }
                }
            } else {
                warn!("Unknown client_id: {}", client_id);
            }
        }
    }
}

/// Informs the new player of all the currently spawned players whenever a
/// `PlayerWantsUpdates` event is received.
pub fn load_player(
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
