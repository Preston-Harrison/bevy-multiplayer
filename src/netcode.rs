use std::marker::PhantomData;

use bevy::{prelude::*, utils::HashMap};
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

#[derive(Resource, Default)]
pub struct Associations {
    player_id_to_client_id: HashMap<u64, u64>,
    client_id_to_player_id: HashMap<u64, u64>,

    player_id_to_server_id: HashMap<u64, u64>,
    server_id_to_player_id: HashMap<u64, u64>,

    server_id_to_entity: HashMap<u64, Entity>,
    entity_to_server_id: HashMap<Entity, u64>,
}

/// TODO: assertions
impl Associations {
    pub fn create_player(
        &mut self,
        player_id: u64,
        server_id: u64,
        client_id: u64,
        entity: Entity,
    ) {
        self.player_id_to_client_id.insert(player_id, client_id);
        self.client_id_to_player_id.insert(client_id, player_id);

        self.player_id_to_server_id.insert(player_id, server_id);
        self.server_id_to_player_id.insert(server_id, player_id);

        self.server_id_to_entity.insert(server_id, entity);
        self.entity_to_server_id.insert(entity, server_id);
    }

    pub fn remove_player(&mut self, player_id: u64) {
        let server_id = self.player_id_to_server_id.remove(&player_id).unwrap();
        let client_id = self.player_id_to_client_id.remove(&player_id).unwrap();
        let entity = self.server_id_to_entity.remove(&server_id).unwrap();

        self.client_id_to_player_id.remove(&client_id);
        self.server_id_to_player_id.remove(&server_id);
        self.entity_to_server_id.remove(&entity);
    }

    /// Returns (entity, server_id, player_id) for a client id.
    pub fn get_data_for_client_id(&self, client_id: u64) -> Option<(Entity, u64, u64)> {
        let player_id = self.client_id_to_player_id.get(&client_id)?;
        let server_id = self.player_id_to_server_id.get(player_id)?;
        let entity = self.server_id_to_entity.get(server_id)?;
        Some((*entity, *server_id, *player_id))
    }

    pub fn is_server_and_client_eq(&self, server_id: u64, client_id: u64) -> bool {
        let Some(player_id) = self.client_id_to_player_id.get(&client_id) else {
            return false;
        };
        let Some(sid) = self.player_id_to_server_id.get(player_id) else {
            return false;
        };

        *sid == server_id
    }
}

pub fn set_associations(
    mut assoc: ResMut<Associations>,
    spawn_q: Query<(Entity, &ServerObject)>,
    mut despawns: RemovedComponents<ServerObject>,
) {
    for (entity, server_obj) in spawn_q.iter() {
        let server_id = server_obj.as_u64();
        assoc.server_id_to_entity.insert(server_id, entity);
        assoc.entity_to_server_id.insert(entity, server_id);
    }

    for entity in despawns.read() {
        if let Some(server_id) = assoc.entity_to_server_id.get(&entity).map(|v| *v) {
            assoc.server_id_to_entity.remove(&server_id);
            assoc.entity_to_server_id.remove(&entity);
        }
    }
}

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

/// Sends a transform update for all server entities.
pub fn send_transform_update(
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
                                game::move_player_from_input(&mut transform, input);
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

/// Sends all new bullets to client as entity spawn.
pub fn send_bullet_update(
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

#[derive(Component, Debug, PartialEq, Eq)]
pub enum NetworkEntityTag {
    Player,
    NPC,
    Bullet,
}

pub mod read {
    use std::{collections::VecDeque, time::Duration};

    use bevy::prelude::*;
    use bevy_renet::renet::{Bytes, ClientId, DefaultChannel, RenetClient, RenetServer};

    use crate::current_time;

    use super::{
        RUMFromClient, RUMFromClientWithId, RUMFromServer, UMFromClient, UMFromClientWithId,
        UMFromServer,
    };

    trait MessageStore {
        type Reliable;
        type Unreliable;
        fn unreliable(&mut self) -> &mut Vec<Self::Unreliable>;
        fn reliable(&mut self) -> &mut Vec<Self::Reliable>;
    }

    /// Allows for mock latency.
    #[derive(Resource)]
    pub struct DelayedMessagesServer {
        reliable: VecDeque<(ClientId, Bytes, Duration)>,
        unreliable: VecDeque<(ClientId, Bytes, Duration)>,
        latency_ms: u64,
    }

    impl DelayedMessagesServer {
        pub fn new(latency: u64) -> Self {
            Self {
                reliable: Default::default(),
                unreliable: Default::default(),
                latency_ms: latency,
            }
        }

        fn save_msgs(&mut self, server: &mut RenetServer) {
            let time = current_time();
            let clients = server.clients_id_iter().collect::<Vec<_>>();
            for client in clients {
                while let Some(msg) =
                    server.receive_message(client, DefaultChannel::ReliableUnordered)
                {
                    self.reliable.push_back((client, msg, time));
                }
                while let Some(msg) = server.receive_message(client, DefaultChannel::Unreliable) {
                    self.unreliable.push_back((client, msg, time));
                }
            }
        }

        fn read_msgs(&mut self, msgs: &mut ServerMessages) {
            let time = current_time();
            let read_threshold = time - Duration::from_millis(self.latency_ms);

            while self.reliable.get(0).is_some_and(|v| v.2 < read_threshold) {
                let (client, msg, _) = self.reliable.pop_front().unwrap();
                if let Ok(um) = RUMFromClient::try_from(msg) {
                    let msg = RUMFromClientWithId {
                        id: client.raw(),
                        msg: um,
                    };
                    msgs.reliable.push(msg);
                } else {
                    warn!("Received unparsable reliable message from client");
                }
            }

            while self.unreliable.get(0).is_some_and(|v| v.2 < read_threshold) {
                let (client, msg, _) = self.unreliable.pop_front().unwrap();
                if let Ok(um) = UMFromClient::try_from(msg) {
                    let msg = UMFromClientWithId {
                        id: client.raw(),
                        msg: um,
                    };
                    msgs.unreliable.push(msg);
                } else {
                    warn!("Received unparsable unreliable message from client");
                }
            }
        }
    }

    #[derive(Default, Resource, Clone)]
    pub struct ServerMessages {
        pub unreliable: Vec<UMFromClientWithId>,
        pub reliable: Vec<RUMFromClientWithId>,
    }

    /// Read raw incoming messages from clients to a buffer.
    pub fn recv_on_server(
        mut delay_msgs: ResMut<DelayedMessagesServer>,
        mut server: ResMut<RenetServer>,
        mut messages: ResMut<ServerMessages>,
    ) {
        messages.unreliable.clear();
        messages.reliable.clear();

        delay_msgs.save_msgs(&mut server);
        delay_msgs.read_msgs(&mut messages);
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
            client.send_message(
                DefaultChannel::ReliableUnordered,
                RUMFromClient::BroadcastTick(tick.current),
            );
        }
    }

    /// Checks for any client ticks, and broadcasts the tick adjustment they need
    /// to be at the desired server tick.
    pub fn send_adjustments(
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
    use bevy::{prelude::*, utils::hashbrown::HashMap};
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

    /// Reads client inputs to a buffer.
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
        read::ServerMessages, tick::Tick, Associations, NetworkEntity, NetworkEntityType,
        RUMFromClient, ServerObject,
    };

    /// Sends current tick to clients who have connected. Sends leave event and despawns
    /// clients who have left.
    pub fn handle_client_connect_and_disconnect(
        mut cmds: Commands,
        mut server_events: EventReader<ServerEvent>,
        mut server: ResMut<RenetServer>,
        assoc: Res<Associations>,
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
                    let (entity, server_id, _) =
                        assoc.get_data_for_client_id(client_id.raw()).unwrap();
                    cmds.entity(entity).despawn_recursive();
                    let leave_payload = RUMFromServer::PlayerLeft {
                        server_obj: server_id,
                    };
                    server.broadcast_message(DefaultChannel::ReliableUnordered, leave_payload);
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

pub mod chunk {
    use bevy::input::keyboard::KeyboardInput;
    use bevy::input::ButtonState;
    use bevy::prelude::*;
    use bevy::utils::{HashMap, HashSet};
    use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};

    use crate::game::{self, Bullet, Player};
    use crate::netcode::Associations;

    use super::read::ServerMessages;
    use super::{NetworkEntity, NetworkEntityTag, RUMFromClient, RUMFromServer, ServerObject};

    pub fn add_resources(app: &mut App) {
        app.insert_resource(Associations::default());
        app.insert_resource(ChunkManager::new(100.0));
        app.insert_resource(EntityRequests::default());
    }

    #[derive(Debug)]
    pub struct Chunk {
        pos: IVec2,

        /// Server objects present in the chunk.
        server_ids: HashSet<u64>,
    }

    impl Chunk {
        pub fn new(pos: IVec2) -> Self {
            Self {
                pos,
                server_ids: HashSet::new(),
            }
        }
    }

    #[derive(Resource, Debug)]
    pub struct ChunkManager {
        chunk_size: f32,
        chunks: HashMap<IVec2, Chunk>,
    }

    impl ChunkManager {
        pub fn new(chunk_size: f32) -> Self {
            Self {
                chunk_size,
                chunks: HashMap::new(),
            }
        }

        /// Returns server ids of all entities in this and surrounding chunks.
        pub fn server_ids_near(&self, chunk_pos: IVec2) -> HashSet<u64> {
            let mut set = HashSet::new();
            for chunk in self.chunks.values() {
                // Check surrounding chunks. Diagnoal = (1^2 + 1^2) = 2
                if (chunk.pos - chunk_pos).length_squared() <= 2 {
                    set.extend(chunk.server_ids.iter());
                }
            }
            set
        }
    }

    pub fn world_pos_to_chunk_pos(chunk_size: f32, world_pos: Vec2) -> IVec2 {
        (world_pos / chunk_size).floor().as_ivec2()
    }

    /// Updates chunk membership based on server entity transform.
    pub fn update_chunk_members(
        mut cmds: Commands,
        assoc: Res<Associations>,
        objs: Query<(Entity, &Transform, &ServerObject)>,
        tags: Query<&NetworkEntityTag>,
        mut cm: ResMut<ChunkManager>,
        mut e_req: ResMut<EntityRequests>,
        mut key: EventReader<KeyboardInput>,
    ) {
        let chunk_size = cm.chunk_size;

        // Determine which chunks should be loaded.
        let mut loaded = HashSet::<IVec2>::new();
        for (entity, transform, _) in objs.iter() {
            let Ok(tag) = tags.get(entity) else {
                warn!("saw server entity without tag");
                continue;
            };

            let world_pos = transform.translation.truncate();
            let chunk_pos = world_pos_to_chunk_pos(chunk_size, world_pos);

            if *tag == NetworkEntityTag::Player || *tag == NetworkEntityTag::NPC {
                loaded.insert(chunk_pos);
            }
        }

        // Add missing chunks
        for chunk in loaded.iter() {
            cm.chunks.entry(*chunk).or_insert(Chunk::new(*chunk));
        }

        // Update all loaded chunks.
        for chunk in cm.chunks.values_mut() {
            let (next, spawns, despawns) =
                get_chunk_update(chunk_size, chunk, objs.iter().map(|v| (v.1, v.2)));
            e_req.server_id_to_spawn.extend(spawns.into_iter());
            e_req.server_id_to_despawn.extend(despawns.into_iter());
            chunk.server_ids = next;
        }

        // Remove unloaded chunks.
        cm.chunks.retain(|pos, _| loaded.contains(pos));

        // Despawn entites in unloaded chunks.
        for (e, transform, obj) in objs.iter() {
            if !cm
                .chunks
                .contains_key(&get_chunk_pos(chunk_size, transform))
            {
                e_req.server_id_to_despawn.insert(obj.as_u64());
                cmds.entity(e).despawn_recursive();
            }
        }

        for k in key.read() {
            if k.key_code == KeyCode::KeyP && k.state == ButtonState::Pressed {
                dbg!(&cm);
            }
        }
    }

    fn get_chunk_update<'a>(
        chunk_size: f32,
        chunk: &Chunk,
        objs: impl Iterator<Item = (&'a Transform, &'a ServerObject)>,
    ) -> (HashSet<u64>, Vec<u64>, Vec<u64>) {
        let next_occupants = objs
            .filter_map(|(transform, server_obj)| {
                let world_pos = transform.translation.truncate();
                if world_pos_to_chunk_pos(chunk_size, world_pos) == chunk.pos {
                    Some(server_obj.as_u64())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();

        let spawns = chunk
            .server_ids
            .difference(&next_occupants)
            .map(|v| *v)
            .collect::<Vec<_>>();
        let leaves = next_occupants
            .difference(&chunk.server_ids)
            .map(|v| *v)
            .collect::<Vec<_>>();

        (next_occupants, spawns, leaves)
    }

    #[derive(Resource, Default)]
    pub struct EntityRequests {
        server_id_to_spawn: HashSet<u64>,
        server_id_to_despawn: HashSet<u64>,
        server_id_needing_update: HashSet<u64>,
    }

    impl EntityRequests {
        fn take(&mut self) -> (HashSet<u64>, HashSet<u64>, HashSet<u64>) {
            (
                std::mem::take(&mut self.server_id_to_spawn),
                std::mem::take(&mut self.server_id_to_despawn),
                std::mem::take(&mut self.server_id_needing_update),
            )
        }
    }

    pub fn broadcast_entity_spawns(world: &mut World) {
        let (spawns, despawns, needing_update) =
            world.get_resource_mut::<EntityRequests>().unwrap().take();

        let assoc = world.remove_resource::<Associations>().unwrap();
        let mut server = world.remove_resource::<RenetServer>().unwrap();
        let cm = world.remove_resource::<ChunkManager>().unwrap();

        for server_id in spawns {
            let entity = assoc.server_id_to_entity.get(&server_id).unwrap();
            let Some(e_ref) = world.get_entity(*entity) else {
                // Entity despawned.
                continue;
            };
            let net_entity = build_entity(&e_ref);

            for client in clients_needing_update(server_id, &cm, &assoc) {
                if !assoc.is_server_and_client_eq(server_id, client.raw()) {
                    server.send_message(
                        client,
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(net_entity.clone()),
                    );
                }
            }
        }

        for server_id in despawns {
            for client in clients_needing_update(server_id, &cm, &assoc) {
                server.send_message(
                    client,
                    DefaultChannel::ReliableUnordered,
                    RUMFromServer::EntityDespawn { server_id },
                )
            }
        }

        for server_id in needing_update {
            let player_id = assoc.server_id_to_player_id.get(&server_id).unwrap();
            let client_id = assoc.player_id_to_client_id.get(player_id).unwrap();

            let chunk_pos = cm
                .chunks
                .values()
                .find(|c| c.server_ids.contains(&server_id))
                .unwrap()
                .pos;

            for server_id in cm.server_ids_near(chunk_pos) {
                let entity = assoc.server_id_to_entity.get(&server_id).unwrap();
                if !assoc.is_server_and_client_eq(server_id, *client_id) {
                    server.send_message(
                        ClientId::from_raw(*client_id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(build_entity(&world.entity(*entity))),
                    );
                }
            }
        }

        world.insert_resource(assoc);
        world.insert_resource(server);
        world.insert_resource(cm);
    }

    fn get_chunk_pos(chunk_size: f32, transform: &Transform) -> IVec2 {
        world_pos_to_chunk_pos(chunk_size, transform.translation.truncate())
    }

    fn build_entity(e_ref: &EntityRef) -> NetworkEntity {
        match e_ref.get::<NetworkEntityTag>().unwrap() {
            NetworkEntityTag::Player => build_player_entity(&e_ref),
            NetworkEntityTag::Bullet => build_bullet_entity(&e_ref),
            NetworkEntityTag::NPC => build_npc_entity(&e_ref),
        }
    }

    fn build_player_entity(e_ref: &EntityRef) -> NetworkEntity {
        NetworkEntity {
            server_id: e_ref.get::<ServerObject>().unwrap().as_u64(),
            data: super::NetworkEntityType::Player {
                id: e_ref.get::<Player>().unwrap().id,
                transform: *e_ref.get::<Transform>().unwrap(),
            },
        }
    }

    fn build_npc_entity(e_ref: &EntityRef) -> NetworkEntity {
        NetworkEntity {
            server_id: e_ref.get::<ServerObject>().unwrap().as_u64(),
            data: super::NetworkEntityType::NPC {
                transform: *e_ref.get::<Transform>().unwrap(),
            },
        }
    }

    fn build_bullet_entity(e_ref: &EntityRef) -> NetworkEntity {
        NetworkEntity {
            server_id: e_ref.get::<ServerObject>().unwrap().as_u64(),
            data: super::NetworkEntityType::Bullet {
                bullet: e_ref.get::<Bullet>().unwrap().clone(),
                transform: *e_ref.get::<Transform>().unwrap(),
            },
        }
    }

    fn clients_needing_update(
        server_id: u64,
        cm: &ChunkManager,
        assoc: &Associations,
    ) -> Vec<ClientId> {
        let chunk = cm
            .chunks
            .values()
            .find(|c| c.server_ids.contains(&server_id));
        let Some(chunk) = chunk else {
            return vec![];
        };

        let proximity = cm.server_ids_near(chunk.pos);
        proximity
            .iter()
            .filter_map(|v| {
                let player = assoc.server_id_to_player_id.get(v)?;
                let raw_client = assoc.player_id_to_client_id.get(player)?;
                Some(ClientId::from_raw(*raw_client))
            })
            .collect()
    }

    /// Checks if any new clients have marked themselves as having started the game,
    /// and marks them as needing a full chunk (and nearby chunks) update.
    pub fn check_new_players(
        mut cmds: Commands,
        msgs: Res<ServerMessages>,
        mut assoc: ResMut<Associations>,
        mut e_req: ResMut<EntityRequests>,
        mut server: ResMut<RenetServer>,
    ) {
        for msg in msgs.reliable.iter() {
            if let RUMFromClient::StartedGame = msg.msg {
                let server_id = rand::random();
                let entity =
                    game::spawn_player(&mut cmds, server_id, msg.id, Transform::default(), false);
                let join_payload = RUMFromServer::PlayerJoined {
                    server_obj: server_id,
                    id: msg.id,
                    transform: Transform::default(),
                };
                server.broadcast_message(DefaultChannel::ReliableUnordered, join_payload);

                e_req.server_id_needing_update.insert(server_id);
                let player_id = msg.id;
                assoc.create_player(player_id, server_id, msg.id, entity);
            }
        }
    }

    #[derive(Component)]
    pub struct ChunkText(IVec2);

    pub fn draw_loaded_chunks(
        mut cmds: Commands,
        chunks: Query<(Entity, &ChunkText)>,
        cm: Res<ChunkManager>,
        assets: Res<AssetServer>,
    ) {
        let border = assets.load("border_1px_white.png");
        for chunk in cm.chunks.values() {
            if !chunks.iter().any(|(_, ct)| ct.0 == chunk.pos) {
                let center = ((chunk.pos.as_vec2() * cm.chunk_size)
                    + Vec2::new(cm.chunk_size / 2.0, cm.chunk_size / 2.0))
                .extend(1.0);
                cmds.spawn((
                    ChunkText(chunk.pos),
                    Text2dBundle {
                        text: Text::from_section(
                            format!("x: {}, y: {}", chunk.pos.x, chunk.pos.y),
                            TextStyle::default(),
                        ),
                        transform: Transform::from_translation(center),
                        ..Default::default()
                    },
                ))
                .insert(SpriteBundle {
                    transform: Transform::from_translation(center),
                    texture: border.clone(),
                    sprite: Sprite {
                        custom_size: Some(Vec2::new(cm.chunk_size, cm.chunk_size)),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .insert(ImageScaleMode::Sliced(TextureSlicer {
                    border: BorderRect::square(1.0),
                    center_scale_mode: SliceScaleMode::Stretch,
                    ..default()
                }));
            }
        }

        for (e, chunk) in chunks.iter() {
            if !cm.chunks.contains_key(&chunk.0) {
                cmds.entity(e).despawn_recursive();
            }
        }
    }
}
