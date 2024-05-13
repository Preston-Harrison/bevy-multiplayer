use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use serde::{Deserialize, Serialize};

use crate::{
    game::{self, Bullet},
    impl_bytes,
};

use self::{input::InputBuffer, read::ClientMessages, tick::Tick};

pub type PlayerId = u64;

#[derive(Resource, Debug)]
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
    pub target: T,
}

pub fn interpolate<T: Component + Interpolate>(mut q: Query<(&mut T, &Interpolated<T>)>) {
    for (mut t, interp) in q.iter_mut() {
        t.interpolate(&interp.target);
    }
}

/// Must exist on an object with a ServerObject to do anything.
#[derive(Component)]
pub struct Prespawned {
    pub behavior: PrespawnBehavior,
}

#[derive(Eq, PartialEq)]
pub enum PrespawnBehavior {
    // Will ignore spawn requests.
    Ignore,
    // Will delete this entity and replace it with the server entity.
    Replace,
}

#[derive(Component, Default)]
pub struct Deterministic<T: Component> {
    data: PhantomData<T>,
}

/// A common identifier between the client and server that identifies an entity.
#[derive(Component)]
pub struct ServerObject(u64);

impl ServerObject {
    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

pub fn broadcast_transforms_on_server(
    mut server: ResMut<RenetServer>,
    objs: Query<(&ServerObject, &Transform)>,
    tick: Res<Tick>,
) {
    for (so, transform) in objs.iter() {
        server.broadcast_message(
            DefaultChannel::Unreliable,
            UMFromServer::TransformUpdate(ComponentUpdate {
                component: transform.clone(),
                server_obj: so.as_u64(),
                tick: tick.current,
            }),
        );
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InputWithTick {
    input: input::Input,
    tick: u64,
}

/// Unreliable messages from the client to server.
#[derive(Serialize, Deserialize, Clone)]
pub enum UMFromClient {
    Input(InputWithTick),
}
impl_bytes!(UMFromClient);

#[derive(Clone)]
pub struct UMFromClientWithId {
    id: PlayerId,
    msg: UMFromClient,
}

/// Unreliable messages from the server to client.
#[derive(Serialize, Deserialize, Clone)]
pub enum UMFromServer {
    TransformUpdate(ComponentUpdate<Transform>),
}
impl_bytes!(UMFromServer);

/// An authoritative state update from the server on a certain tick.
#[derive(Serialize, Deserialize, Clone)]
pub struct ComponentUpdate<T: Component + Clone> {
    server_obj: u64,
    tick: u64,
    component: T,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum RUMFromServer {
    PlayerJoined {
        server_obj: u64,
        id: PlayerId,
        transform: Transform,
    },
    PlayerLeft {
        server_obj: u64,
    },
    EntitySpawn(NetworkEntity),
    EntityDespawn {
        server_id: u64,
    },
    AdjustTick(i8),
    BroadcastTick(u64),
}
impl_bytes!(RUMFromServer);

#[derive(Serialize, Deserialize, Clone)]
pub enum RUMFromClient {
    BroadcastTick(u64),
    /// Client sends this when it is in the game and ready to receive game updates.
    StartedGame,
}
impl_bytes!(RUMFromClient);

#[derive(Clone)]
pub struct RUMFromClientWithId {
    id: u64,
    msg: RUMFromClient,
}

pub fn apply_transform_on_client(
    msgs: Res<ClientMessages>,
    mut t_q: Query<
        (Entity, &mut Transform, &ServerObject, Option<&LocalPlayer>),
        Without<Deterministic<Transform>>,
    >,
    tick: Res<tick::Tick>,
    mut commands: Commands,
    i_buf: Res<InputBuffer>,
) {
    let max_update = msgs
        .unreliable
        .iter()
        .filter_map(|v| match v {
            UMFromServer::TransformUpdate(update) => Some(update.tick),
            _ => None,
        })
        .max();
    for msg in msgs.unreliable.iter() {
        if let UMFromServer::TransformUpdate(update) = msg {
            if update.tick != max_update.unwrap() {
                continue;
            }
            for (entity, mut transform, server_obj, local) in t_q.iter_mut() {
                if server_obj.as_u64() == update.server_obj {
                    if local.is_some() {
                        *transform = update.component.clone();
                        let ticks = tick.current.saturating_sub(update.tick);
                        for n in (1..ticks).rev() {
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

pub fn broadcast_bullets_on_server(
    mut server: ResMut<RenetServer>,
    bullets: Query<(&Transform, &Bullet, &ServerObject), Added<Bullet>>,
) {
    for (transform, bullet, server_obj) in bullets.iter() {
        server.broadcast_message(
            DefaultChannel::ReliableUnordered,
            RUMFromServer::EntitySpawn(NetworkEntity {
                server_id: server_obj.as_u64(),
                data: NetworkEntityType::Bullet {
                    bullet: bullet.clone(),
                    transform: *transform,
                },
            }),
        );
    }
}

impl Interpolate for Transform {
    fn interpolate(&mut self, target: &Self) {
        // 0.1 for 10% movement towards the target each tick
        self.translation = self.translation.lerp(target.translation, 0.1);
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkEntity {
    pub data: NetworkEntityType,
    pub server_id: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum NetworkEntityType {
    Player {
        id: PlayerId,
        transform: Transform,
    },
    NPC {
        transform: Transform,
    },
    Bullet {
        bullet: Bullet,
        transform: Transform,
    },
}

pub mod read {
    use bevy::prelude::*;
    use bevy_renet::renet::{DefaultChannel, RenetClient, RenetServer};

    use super::{
        RUMFromClient, RUMFromClientWithId, RUMFromServer, UMFromClient, UMFromClientWithId,
        UMFromServer,
    };

    #[derive(Default, Resource, Clone)]
    pub struct ServerMessages {
        pub unreliable: Vec<UMFromClientWithId>,
        pub reliable: Vec<RUMFromClientWithId>,
    }

    pub fn recv_on_server(mut server: ResMut<RenetServer>, mut messages: ResMut<ServerMessages>) {
        messages.unreliable.clear();
        messages.reliable.clear();

        let clients = server.clients_id_iter().collect::<Vec<_>>();
        for client in clients {
            while let Some(message) = server.receive_message(client, DefaultChannel::Unreliable) {
                if let Ok(um) = UMFromClient::try_from(message) {
                    // TODO: get player id from client id instead of just using client id.
                    let msg = UMFromClientWithId {
                        id: client.raw(),
                        msg: um,
                    };
                    messages.unreliable.push(msg);
                } else {
                    warn!("Received unparsable unreliable message from server");
                };
            }

            while let Some(message) =
                server.receive_message(client, DefaultChannel::ReliableUnordered)
            {
                if let Ok(um) = RUMFromClient::try_from(message) {
                    // TODO: get player id from client id instead of just using client id.
                    let msg = RUMFromClientWithId {
                        id: client.raw(),
                        msg: um,
                    };
                    messages.reliable.push(msg);
                } else {
                    warn!("Received unparsable reliable unordered message from server");
                };
            }
        }
    }

    #[derive(Default, Resource, Clone)]
    pub struct ClientMessages {
        pub unreliable: Vec<UMFromServer>,
        pub reliable: Vec<RUMFromServer>,
    }

    impl ClientMessages {
        fn clear(&mut self) {
            self.unreliable.clear();
            self.reliable.clear();
        }
    }

    pub fn recv_on_client(mut client: ResMut<RenetClient>, mut messages: ResMut<ClientMessages>) {
        messages.clear();

        while let Some(message) = client.receive_message(DefaultChannel::Unreliable) {
            if let Ok(um) = UMFromServer::try_from(message) {
                messages.unreliable.push(um);
            } else {
                warn!("Received unparsable unreliable message from server");
            };
        }

        while let Some(message) = client.receive_message(DefaultChannel::ReliableUnordered) {
            if let Ok(um) = RUMFromServer::try_from(message) {
                messages.reliable.push(um);
            } else {
                warn!("Received unparsable unreliable message from server");
            };
        }
    }
}

pub mod tick {
    use std::time::Duration;

    use bevy::prelude::*;
    use bevy_renet::renet::{ClientId, DefaultChannel, RenetClient, RenetServer};

    use super::{
        read::{ClientMessages, ServerMessages},
        RUMFromClient, RUMFromServer,
    };

    #[derive(Resource, Default)]
    pub struct Tick {
        pub current: u64,

        /// The server requested tick adjustment to keep the client ahead of the server.
        pub adjust: i8,
    }

    pub fn increment_tick_on_server(mut tick: ResMut<Tick>) {
        tick.current += 1;
    }

    pub fn set_adjustment_tick_on_client(mut tick: ResMut<Tick>, msgs: Res<ClientMessages>) {
        for msg in msgs.reliable.iter() {
            if let RUMFromServer::AdjustTick(adjust) = msg {
                tick.adjust = *adjust;
            }
        }
    }

    pub fn initialize_tick_on_client(mut cmds: Commands, msgs: Res<ClientMessages>) {
        for msg in msgs.reliable.iter() {
            if let RUMFromServer::BroadcastTick(tick) = msg {
                cmds.insert_resource(Tick {
                    current: *tick + 5,
                    adjust: 0,
                });
            }
        }
    }

    pub fn ask_for_game_updates_on_client(mut client: ResMut<RenetClient>) {
        client.send_message(
            DefaultChannel::ReliableUnordered,
            RUMFromClient::StartedGame,
        );
    }

    #[derive(Resource)]
    pub struct TickBroadcastTimer {
        timer: Timer,
    }

    impl Default for TickBroadcastTimer {
        fn default() -> Self {
            Self {
                timer: Timer::new(Duration::from_secs(5), TimerMode::Repeating),
            }
        }
    }

    pub fn broadcast_tick_on_client(
        time: Res<Time>,
        tick: Res<Tick>,
        mut timer: ResMut<TickBroadcastTimer>,
        mut client: ResMut<RenetClient>,
    ) {
        timer.timer.tick(time.delta());
        if timer.timer.just_finished() {
            info!("sending current tick");
            client.send_message(
                DefaultChannel::ReliableUnordered,
                RUMFromClient::BroadcastTick(tick.current),
            );
        }
    }

    pub fn broadcast_adjustment_on_server(
        tick: Res<Tick>,
        msgs: Res<ServerMessages>,
        mut server: ResMut<RenetServer>,
    ) {
        for msg in msgs.reliable.iter() {
            if let RUMFromClient::BroadcastTick(client_tick) = msg.msg {
                // TODO: check for overflows
                let diff = (tick.current as i64 - client_tick as i64) as i8;
                // Client should be 2 ticks ahead.
                let adjustment = diff + 2;
                if adjustment != 0 {
                    info!("sending adjustment tick");
                    server.send_message(
                        ClientId::from_raw(msg.id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::AdjustTick(adjustment),
                    );
                }
            }
        }
    }
}

pub mod input {
    use bevy::{prelude::*, utils::hashbrown::HashMap, window::PrimaryWindow};
    use bevy_renet::renet::{DefaultChannel, RenetClient};
    use serde::{Deserialize, Serialize};

    use crate::{game::MousePosition, impl_bytes, netcode::InputWithTick, utils::Buffer};

    use super::{read::ServerMessages, tick::Tick, LocalPlayer, PlayerId, UMFromClient};

    #[derive(Resource)]
    pub struct InputBuffer {
        pub inputs: Buffer<Input>,
    }

    impl Default for InputBuffer {
        fn default() -> Self {
            Self {
                inputs: Buffer::new(20),
            }
        }
    }

    #[derive(Resource)]
    pub struct InputMapBuffer {
        pub inputs: Buffer<HashMap<PlayerId, Input>>,
    }

    impl Default for InputMapBuffer {
        fn default() -> Self {
            let mut inputs = Buffer::new(10);
            inputs.fill_with(|| HashMap::new());
            Self { inputs }
        }
    }

    #[derive(Serialize, Deserialize, Clone, Debug, Default)]
    pub struct ShootInput {
        pub direction: Vec2,
        pub origin: Vec2,
        pub server_id: u64,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, Default)]
    pub struct Input {
        pub x: i8,
        pub y: i8,
        pub shoot: Option<ShootInput>,
    }
    impl_bytes!(Input);

    impl Input {
        pub fn is_no_input(&self) -> bool {
            self.x == 0 && self.y == 0 && self.shoot.is_none()
        }
    }

    #[derive(Serialize, Deserialize, Clone)]
    pub struct InputWithId {
        input: Input,
        player_id: PlayerId,
    }

    pub fn read_input_on_server(
        msgs: Res<ServerMessages>,
        mut i_buf: ResMut<InputMapBuffer>,
        tick: Res<Tick>,
    ) {
        i_buf.inputs.fill_with(|| HashMap::new());

        for msg in msgs.unreliable.iter() {
            if let UMFromClient::Input(input) = &msg.msg {
                if input.tick < tick.current {
                    warn!("dropped late input");
                    continue;
                }
                let tick_diff = input.tick - tick.current;
                info!("tick diff: {tick_diff}");
                if let Some(buf) = i_buf.inputs.get_mut(tick_diff as usize) {
                    buf.insert(msg.id, input.input.clone());
                } else {
                    warn!("dropped input too far in future");
                }
            }
        }
    }

    pub fn read_input_on_client(
        key: Res<ButtonInput<KeyCode>>,
        mouse: Res<ButtonInput<MouseButton>>,
        mouse_pos: Res<MousePosition>,
        player_q: Query<&Transform, With<LocalPlayer>>,
        mut i_buf: ResMut<InputBuffer>,
        mut client: ResMut<RenetClient>,
        tick: Res<Tick>,
    ) {
        let Ok(player_t) = player_q.get_single() else {
            return;
        };
        let mut input = Input {
            x: 0,
            y: 0,
            shoot: None,
        };

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

        if let Some(position) = mouse_pos.current {
            if mouse.pressed(MouseButton::Left) {
                let dir = (position - player_t.translation.truncate()).normalize_or_zero();
                let origin = player_t.translation.truncate();
                input.shoot = Some(ShootInput {
                    direction: dir,
                    origin,
                    server_id: rand::random(),
                });
            }
        }

        i_buf.inputs.push_front(input.clone());

        if !input.is_no_input() {
            client.send_message(
                DefaultChannel::Unreliable,
                UMFromClient::Input(InputWithTick {
                    input,
                    tick: tick.current,
                }),
            );
        }
    }
}

pub mod conn {
    use bevy::{
        ecs::{event::EventReader, system::ResMut},
        log::info,
        prelude::*,
    };
    use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer, ServerEvent};

    use crate::{
        game::{self, Bullet, Player, NPC},
        netcode::RUMFromServer,
    };

    use super::{
        read::ServerMessages, tick::Tick, NetworkEntity, NetworkEntityType, RUMFromClient,
        ServerObject,
    };

    pub fn handle_connect_on_server(
        mut cmds: Commands,
        mut server_events: EventReader<ServerEvent>,
        mut server: ResMut<RenetServer>,
        player_q: Query<(Entity, &Player, &ServerObject)>,
        tick: Res<Tick>,
    ) {
        // TODO: broadcast player join/leave events.
        for event in server_events.read() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    info!("client {client_id} connected");
                    server.send_message(
                        *client_id,
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::BroadcastTick(tick.current),
                    );
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    info!("client {client_id} disconnected because {reason}");
                    for (entity, player, server_obj) in player_q.iter() {
                        if player.id == client_id.raw() {
                            cmds.entity(entity).despawn_recursive();
                            let leave_payload = RUMFromServer::PlayerLeft {
                                server_obj: server_obj.as_u64(),
                            };
                            server.broadcast_message(
                                DefaultChannel::ReliableUnordered,
                                leave_payload,
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn send_join_messages_on_server(
        mut cmds: Commands,
        msgs: Res<ServerMessages>,
        mut server: ResMut<RenetServer>,
        players: Query<(&ServerObject, &Player, &Transform)>,
        npcs: Query<(&ServerObject, &Transform), With<NPC>>,
        bullets: Query<(&ServerObject, &Transform, &Bullet)>,
    ) {
        for msg in msgs.reliable.iter() {
            if let RUMFromClient::StartedGame = msg.msg {
                let server_obj = rand::random();
                game::spawn_player(&mut cmds, server_obj, msg.id, Transform::default(), false);
                let join_payload = RUMFromServer::PlayerJoined {
                    server_obj,
                    id: msg.id,
                    transform: Transform::default(),
                };
                server.broadcast_message(DefaultChannel::ReliableUnordered, join_payload);

                for (obj, player, transform) in players.iter() {
                    if player.id == msg.id {
                        continue;
                    }
                    server.send_message(
                        ClientId::from_raw(msg.id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(NetworkEntity {
                            server_id: obj.as_u64(),
                            data: NetworkEntityType::Player {
                                id: player.id,
                                transform: *transform,
                            },
                        }),
                    );
                }

                for (obj, transform) in npcs.iter() {
                    server.send_message(
                        ClientId::from_raw(msg.id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(NetworkEntity {
                            server_id: obj.as_u64(),
                            data: NetworkEntityType::NPC {
                                transform: *transform,
                            },
                        }),
                    );
                }

                for (obj, transform, bullet) in bullets.iter() {
                    server.send_message(
                        ClientId::from_raw(msg.id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(NetworkEntity {
                            server_id: obj.as_u64(),
                            data: NetworkEntityType::Bullet {
                                bullet: bullet.clone(),
                                transform: *transform,
                            },
                        }),
                    );
                }
            }
        }
    }
}
