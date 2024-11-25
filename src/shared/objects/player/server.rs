use bevy::{prelude::*, utils::HashMap};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};

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
        objects::{
            grounded::Grounded,
            player::{JumpCooldown, Player},
            NetworkObject,
        },
        physics::Kinematics,
        tick::Tick,
    },
};

#[derive(Component, Default)]
pub struct LastInputTracker {
    order: u64,
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

pub fn init_players(
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

pub fn broadcast_player_data(
    query: Query<
        (
            &NetworkObject,
            &Transform,
            &LastInputTracker,
            &Kinematics,
            &JumpCooldown,
        ),
        With<Player>,
    >,
    client_netmap: Res<ClientNetworkObjectMap>,
    mut server: ResMut<RenetServer>,
    tick: Res<Tick>,
) {
    for (obj, transform, input_tracker, kinematics, jump_cooldown) in query.iter() {
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
            jump_cooldown_elapsed: jump_cooldown.timer.elapsed(),
        });
        let bytes = bincode::serialize(&message).unwrap();
        server.send_message(*client_id, DefaultChannel::Unreliable, bytes);
    }
}

pub fn apply_inputs(
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
            last_input_tracker.order = input.order;
        }
    }
}

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
