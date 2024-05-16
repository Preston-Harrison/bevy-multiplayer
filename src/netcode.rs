use std::marker::PhantomData;

use bevy::{prelude::*, utils::HashMap};
use bevy_renet::renet::{DefaultChannel, RenetServer};
use serde::{Deserialize, Serialize};

use crate::{
    chunk::ChunkManager,
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

    /// Returns (entity, server_id, player_id) for a client id.
    pub fn get_data_for_client_id(&self, client_id: u64) -> Option<(Entity, u64, u64)> {
        let player_id = self.client_id_to_player_id.get(&client_id)?;
        let server_id = self.player_id_to_server_id.get(player_id)?;
        let entity = self.server_id_to_entity.get(server_id)?;
        Some((*entity, *server_id, *player_id))
    }
}

pub fn cleanup_deleted_server_objs(
    mut cm: ResMut<ChunkManager>,
    mut assoc: ResMut<Associations>,
    mut reader: RemovedComponents<ServerObject>,
) {
    for entity in reader.read() {

        let Some(server_id) = assoc.entity_to_server_id.remove(&entity) else {
            warn!("tried to clean up entity with no server id");
            continue;
        };
        assoc.server_id_to_entity.remove(&server_id);
        cm.despawn(server_id);

        let Some(player_id) = assoc.server_id_to_player_id.remove(&server_id) else {
            continue;
        };
        assoc.player_id_to_server_id.remove(&player_id);

        let Some(client_id) = assoc.player_id_to_client_id.remove(&player_id) else {
            warn!("saw server with no client");
            continue;
        };
        assoc.client_id_to_player_id.remove(&client_id);
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
    /// Will ignore spawn requests.
    Ignore,
    /// Will delete this entity and replace it with the server entity.
    #[allow(dead_code)]
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

impl Interpolate for Transform {
    fn interpolate(&mut self, target: &Self) {
        // 0.1 for 10% movement towards the target each tick
        self.translation = self.translation.lerp(target.translation, 0.1);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkEntity {
    pub data: NetworkEntityType,
    pub server_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Component, Debug, PartialEq, Eq, Clone, Copy)]
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
    use bevy_renet::renet::{DefaultChannel, RenetServer, ServerEvent};

    use crate::netcode::RUMFromServer;

    use super::{tick::Tick, Associations};

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
}

pub mod chunk {
    use bevy::input::keyboard::KeyboardInput;
    use bevy::input::ButtonState;
    use bevy::prelude::*;
    use bevy::utils::HashSet;
    use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};

    use crate::chunk;
    use crate::game::{self, Bullet, Player};
    use crate::netcode::Associations;

    use super::read::ServerMessages;
    use super::{NetworkEntity, NetworkEntityTag, RUMFromClient, RUMFromServer, ServerObject};

    pub fn add_resources(app: &mut App) {
        app.insert_resource(Associations::default());
        app.insert_resource(chunk::ChunkManager::new(100.0));
        app.insert_resource(GameSyncRequest::default());
    }

    #[derive(Resource, Default)]
    pub struct GameSyncRequest {
        client_ids: HashSet<u64>,
    }

    /// Updates chunk membership based on server entity transform.
    pub fn update_chunk_members(
        mut cmds: Commands,
        assoc: Res<Associations>,
        objs: Query<(Entity, &Transform, &ServerObject)>,
        tags: Query<(Entity, &ServerObject, &NetworkEntityTag)>,
        mut cm: ResMut<chunk::ChunkManager>,
        mut key: EventReader<KeyboardInput>,
    ) {
        let chunk_size = cm.chunk_size();

        for (_, t, obj) in objs.iter() {
            let chunk_pos = chunk::transform_to_chunk_pos(chunk_size, *t);
            cm.load_chunks_near(chunk_pos);
            cm.set_chunk_location(obj.as_u64(), chunk_pos);
        }

        let observers = tags
            .iter()
            .filter_map(|(_, server_obj, tag)| {
                if *tag == NetworkEntityTag::NPC || *tag == NetworkEntityTag::Player {
                    Some(server_obj.as_u64())
                } else {
                    None
                }
            })
            .collect::<HashSet<u64>>();

        let dangling = cm.purge_chunks(&observers);

        for dangler in dangling.iter() {
            if let Some(entity) = assoc.server_id_to_entity.get(dangler) {
                cmds.entity(*entity).despawn_recursive();
            };
        }

        for k in key.read() {
            if k.key_code == KeyCode::KeyP && k.state == ButtonState::Pressed {
                dbg!(&cm);
            }
        }
    }

    pub fn broadcast_entity_spawns(world: &mut World) {
        let syncs = std::mem::take(
            &mut world
                .get_resource_mut::<GameSyncRequest>()
                .unwrap()
                .client_ids,
        );
        let assoc = world.remove_resource::<Associations>().unwrap();
        let mut server = world.remove_resource::<RenetServer>().unwrap();
        let mut cm = world.remove_resource::<chunk::ChunkManager>().unwrap();

        for (client_id, player_id) in assoc.client_id_to_player_id.iter() {
            let server_id = assoc.player_id_to_server_id.get(player_id).unwrap();

            if syncs.contains(client_id) {
                for spawn in cm.get_nearby_occupants(*server_id) {
                    if spawn == *server_id {
                        continue;
                    }
                    let entity = assoc
                        .server_id_to_entity
                        .get(&spawn)
                        .expect("server id must have entity");
                    let Some(e_ref) = world.get_entity(*entity) else {
                        // Entity despawned.
                        continue;
                    };
                    let net_entity = build_entity(&e_ref);

                    server.send_message(
                        ClientId::from_raw(*client_id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(net_entity.clone()),
                    );
                }
            } else {
                for spawn in cm.get_nearby_spawns(*server_id) {
                    if spawn == *server_id {
                        continue;
                    }
                    let entity = assoc.server_id_to_entity.get(&spawn).unwrap();
                    let Some(e_ref) = world.get_entity(*entity) else {
                        // Entity despawned.
                        continue;
                    };
                    let net_entity = build_entity(&e_ref);

                    info!("sent spawn for {}", spawn);
                    server.send_message(
                        ClientId::from_raw(*client_id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntitySpawn(net_entity.clone()),
                    );
                }

                for despawn in cm.get_nearby_despawns(*server_id) {
                    if despawn == *server_id {
                        continue;
                    }
                    info!("sent despawn for {}", despawn);
                    server.send_message(
                        ClientId::from_raw(*client_id),
                        DefaultChannel::ReliableUnordered,
                        RUMFromServer::EntityDespawn { server_id: despawn },
                    );
                }
            }
        }

        cm.update_visible_objs();

        world.insert_resource(assoc);
        world.insert_resource(server);
        world.insert_resource(cm);
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

    /// Checks if any new clients have marked themselves as having started the game,
    /// and marks them as needing a full chunk (and nearby chunks) update.
    pub fn check_new_players(
        mut cmds: Commands,
        msgs: Res<ServerMessages>,
        mut assoc: ResMut<Associations>,
        mut syncs: ResMut<GameSyncRequest>,
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

                syncs.client_ids.insert(msg.id);
                let player_id = msg.id;
                info!("created obj {server_id} for client {}", msg.id);
                assoc.create_player(player_id, server_id, msg.id, entity);
            }
        }
    }

    #[derive(Component)]
    pub struct ChunkText(IVec2);

    pub fn draw_loaded_chunks(
        mut cmds: Commands,
        chunks: Query<(Entity, &ChunkText)>,
        cm: Res<chunk::ChunkManager>,
        assets: Res<AssetServer>,
    ) {
        let border = assets.load("border_1px_white.png");
        for (pos, _) in cm.chunks() {
            if !chunks.iter().any(|(_, ct)| ct.0 == *pos) {
                let center = ((pos.as_vec2() * cm.chunk_size())
                    + Vec2::new(cm.chunk_size() / 2.0, cm.chunk_size() / 2.0))
                .extend(1.0);
                cmds.spawn((
                    ChunkText(*pos),
                    Text2dBundle {
                        text: Text::from_section(
                            format!("x: {}, y: {}", pos.x, pos.y),
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
                        custom_size: Some(Vec2::new(cm.chunk_size(), cm.chunk_size())),
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
            if !cm.chunks().any(|(pos, _)| pos == &chunk.0) {
                cmds.entity(e).despawn_recursive();
            }
        }
    }
}
