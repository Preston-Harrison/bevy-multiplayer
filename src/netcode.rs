/// NOTE: Messages are considered unreliable and unordered, events are considered
/// reliable and unordered.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{game, impl_bytes};

use self::{input::InputBuffer, read::ClientMessages};

pub type PlayerId = u64;

#[derive(Resource)]
pub struct ClientInfo {
    pub id: PlayerId,
}

/// Tags a player with being local, meaning it will rollback instead of interpolate.
#[derive(Component)]
pub struct LocalPlayer;

pub trait Interpolate {
    /// Interpolate between self and target for one tick.
    fn interpolate(&mut self, target: &Self);
}

/// Allows components of an entity to interpolate to state updates sent by the server.
#[derive(Component)]
pub struct Interpolated<T: Component + Interpolate> {
    target: T,
}

pub fn interpolate<T: Component + Interpolate>(mut q: Query<(&mut T, &Interpolated<T>)>) {
    for (mut t, interp) in q.iter_mut() {
        t.interpolate(&interp.target);
    }
}

/// If this exists on an entity, it will be deleted when a ServerObject is spawned
/// with the same id.
#[derive(Component)]
pub struct Prespawned {
    id: u64,
}

/// A common identifier between the client and server that identifies an entity.
#[derive(Component)]
pub struct ServerObject(u64);

impl ServerObject {
    pub fn rand() -> Self {
        Self(rand::random())
    }

    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Replaces prespawned entities with server objects. Useful for things like bullets,
/// which are spawned immediately on the client, then handed to the server once an
/// acknoledgement is received.
pub fn replace_prespawned_on_client(
    mut commands: Commands,
    prespawned: Query<(Entity, &Prespawned)>,
    server_objs: Query<&ServerObject, Added<ServerObject>>,
) {
    for server_obj in server_objs.iter() {
        for (e, spawn) in prespawned.iter() {
            if server_obj.as_u64() == spawn.id {
                commands.entity(e).despawn_recursive();
            }
        }
    }
}

/// Unreliable messages from the client to server.
#[derive(Serialize, Deserialize, Clone)]
pub enum MsgFromClient {
    Input(input::Input),
}
impl_bytes!(MsgFromClient);

#[derive(Serialize, Deserialize, Clone)]
pub struct MsgFromClientWithId {
    id: PlayerId,
    msg: MsgFromClient,
}

/// Unreliable messages from the server to client.
#[derive(Serialize, Deserialize, Clone)]
pub enum MsgFromServer {
    TransformUpdate(ComponentUpdate<Transform>),
}
impl_bytes!(MsgFromServer);

/// An authoritative state update from the server on a certain tick.
#[derive(Serialize, Deserialize, Clone)]
pub struct ComponentUpdate<T: Component + Clone> {
    server_obj: u64,
    tick: u64,
    component: T,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum EventFromServer {
    PlayerJoined {
        server_obj: u64,
        id: PlayerId,
        transform: Transform,
    },
    PlayerLeft {
        server_obj: u64,
    },
}
impl_bytes!(EventFromServer);

pub fn apply_transform_on_client(
    msgs: Res<ClientMessages>,
    mut t_q: Query<(Entity, &mut Transform, &ServerObject, Option<&LocalPlayer>)>,
    tick: Res<tick::Tick>,
    mut commands: Commands,
    i_buf: Res<InputBuffer>,
) {
    for msg in msgs.unreliable.iter() {
        if let MsgFromServer::TransformUpdate(update) = msg {
            for (entity, mut transform, server_obj, local) in t_q.iter_mut() {
                if server_obj.as_u64() == update.server_obj {
                    if local.is_some() {
                        *transform = update.component.clone();
                        let ticks = tick.current() - update.tick;
                        for n in 0..ticks {
                            if let Some(input) = i_buf.inputs.get(n as usize) {
                                game::move_player(&mut transform, input);
                            }
                        }
                    } else {
                        commands.entity(entity).insert(Interpolated {
                            target: update.component,
                        });
                    }
                }
            }
        }
    }
}

impl Interpolate for Transform {
    fn interpolate(&mut self, target: &Self) {
        // 0.1 for 10% movement towards the target each tick
        self.translation = self.translation.lerp(target.translation, 0.1);
    }
}

pub mod read {
    use bevy::prelude::*;
    use bevy_renet::renet::{DefaultChannel, RenetClient, RenetServer};

    use super::{EventFromServer, MsgFromClient, MsgFromClientWithId, MsgFromServer};

    #[derive(Default, Resource, Clone)]
    pub struct ServerMessages {
        pub unreliable: Vec<MsgFromClientWithId>,
    }

    pub fn recv_on_server(mut server: ResMut<RenetServer>, mut messages: ResMut<ServerMessages>) {
        messages.unreliable.clear();

        let clients = server.clients_id_iter().collect::<Vec<_>>();
        for client in clients {
            while let Some(message) = server.receive_message(client, DefaultChannel::Unreliable) {
                if let Ok(um) = MsgFromClient::try_from(message) {
                    // TODO: get player id from client id instead of just using client id.
                    let msg = MsgFromClientWithId {
                        id: client.raw(),
                        msg: um,
                    };
                    messages.unreliable.push(msg);
                } else {
                    warn!("Received unparsable unreliable message from server");
                };
            }
        }
    }

    #[derive(Default, Resource, Clone)]
    pub struct ClientMessages {
        pub unreliable: Vec<MsgFromServer>,
        pub reliable: Vec<EventFromServer>,
    }

    pub fn recv_on_client(mut client: ResMut<RenetClient>, mut messages: ResMut<ClientMessages>) {
        messages.unreliable.clear();
        while let Some(message) = client.receive_message(DefaultChannel::Unreliable) {
            if let Ok(um) = MsgFromServer::try_from(message) {
                messages.unreliable.push(um);
            } else {
                warn!("Received unparsable unreliable message from server");
            };
        }

        messages.reliable.clear();
        while let Some(message) = client.receive_message(DefaultChannel::ReliableUnordered) {
            if let Ok(um) = EventFromServer::try_from(message) {
                messages.reliable.push(um);
            } else {
                warn!("Received unparsable unreliable message from server");
            };
        }
    }
}

pub mod tick {
    use bevy::prelude::*;

    pub fn increment_tick(mut tick: ResMut<Tick>) {
        tick.current += 1;
    }

    #[derive(Resource, Default)]
    pub struct Tick {
        current: u64,
    }

    impl Tick {
        pub fn current(&self) -> u64 {
            self.current
        }
    }
}

pub mod input {
    use bevy::{prelude::*, utils::hashbrown::HashMap};
    use bevy_renet::renet::{DefaultChannel, RenetClient};
    use serde::{Deserialize, Serialize};

    use crate::{impl_bytes, utils::Queue};

    use super::{read::ServerMessages, MsgFromClient, PlayerId};

    #[derive(Resource)]
    pub struct InputBuffer {
        pub inputs: Queue<Input>,
    }

    impl Default for InputBuffer {
        fn default() -> Self {
            Self {
                inputs: Queue::new(50),
            }
        }
    }

    #[derive(Resource)]
    pub struct InputMapBuffer {
        pub inputs: Queue<HashMap<PlayerId, Input>>,
    }

    impl Default for InputMapBuffer {
        fn default() -> Self {
            Self {
                inputs: Queue::new(50),
            }
        }
    }

    #[derive(Serialize, Deserialize, Clone, Debug, Default, Eq, PartialEq)]
    pub struct Input {
        pub x: i8,
        pub y: i8,
    }
    impl_bytes!(Input);

    #[derive(Serialize, Deserialize, Clone)]
    pub struct InputWithId {
        input: Input,
        player_id: PlayerId,
    }

    pub fn read_input_on_server(msgs: Res<ServerMessages>, mut i_buf: ResMut<InputMapBuffer>) {
        let mut inputs = HashMap::<PlayerId, Input>::new();
        for msg in msgs.unreliable.iter() {
            if let MsgFromClient::Input(input) = &msg.msg {
                inputs.insert(msg.id, input.clone());
            }
        }

        i_buf.inputs.push(inputs);
    }

    pub fn read_input_on_client(
        key: Res<ButtonInput<KeyCode>>,
        mut i_buf: ResMut<InputBuffer>,
        mut client: ResMut<RenetClient>,
    ) {
        let mut input = Input { x: 0, y: 0 };

        if key.pressed(KeyCode::KeyW) {
            input.y += 1;
        }
        if key.pressed(KeyCode::KeyS) {
            input.y -= 1;
        }
        if key.pressed(KeyCode::KeyA) {
            input.x -= 1;
        }
        if key.pressed(KeyCode::KeyD) {
            input.x += 1;
        }

        i_buf.inputs.push(input.clone());

        if input != Input::default() {
            client.send_message(DefaultChannel::Unreliable, MsgFromClient::Input(input));
        }
    }
}

pub mod conn {
    use bevy::{
        ecs::{event::EventReader, system::ResMut},
        log::info,
        prelude::*,
    };
    use bevy_renet::renet::{DefaultChannel, RenetServer, ServerEvent};

    use crate::{game, netcode::EventFromServer};

    pub fn handle_connect_on_server(
        mut cmds: Commands,
        mut server_events: EventReader<ServerEvent>,
        mut server: ResMut<RenetServer>,
    ) {
        // TODO: broadcast player join/leave events.
        for event in server_events.read() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    info!("client {client_id} connected");
                    let server_obj = rand::random();
                    game::spawn_player(
                        &mut cmds,
                        server_obj,
                        client_id.raw(),
                        Transform::default(),
                        false,
                    );
                    let join_payload = EventFromServer::PlayerJoined {
                        server_obj,
                        id: client_id.raw(),
                        transform: Transform::default(),
                    };
                    server.broadcast_message(DefaultChannel::ReliableUnordered, join_payload);
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    info!("client {client_id} disconnected because {reason}");
                }
            }
        }
    }
}
