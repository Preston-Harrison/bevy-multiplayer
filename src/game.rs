use std::time::Duration;

use bevy::{ecs::schedule::ScheduleLabel, prelude::*, window::PrimaryWindow};
use serde::{Deserialize, Serialize};

use crate::{
    netcode::{
        input::{Input, InputBuffer, InputMapBuffer},
        read::ClientMessages,
        tick::Tick,
        ClientInfo, Deterministic, LocalPlayer, NetworkEntityTag, NetworkEntityType, PlayerId,
        PrespawnBehavior, Prespawned, RUMFromServer, ServerObject,
    },
    TICK_TIME,
};

const BULLET_SPEED: f32 = 30.0;

#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct GameLogic;

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Default)]
pub struct MousePosition {
    pub current: Option<Vec2>,
    pub last: Vec2,
}

pub fn set_cursor_location_on_client(
    mut mpos: ResMut<MousePosition>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let (camera, camera_transform) = q_camera.single();
    let window = q_window.single();
    mpos.current = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor))
        .map(|ray| ray.origin.truncate());

    if let Some(v) = mpos.current {
        mpos.last = v;
    }
}

pub fn run_game_logic_on_client(world: &mut World) {
    let tick = world.get_resource::<Tick>().expect("tick must exist");
    let mut adjust = tick.adjust;
    let mut current = tick.current;

    if adjust >= 1 {
        info!("fast forwarding");
        world.run_schedule(GameLogic);
        current += 1;

        while adjust > 0 {
            // Assume no input on fast forward ticks.
            world
                .get_resource_mut::<InputBuffer>()
                .expect("input buffer must exist")
                .inputs
                .push_back(Input::default());
            world.run_schedule(GameLogic);
            adjust -= 1;
            current += 1;
        }
    } else if adjust == 0 {
        world.run_schedule(GameLogic);
        current += 1;
    } else {
        info!("paused for tick");
        adjust += 1;
    }

    let mut tick = world.get_resource_mut::<Tick>().expect("tick must exist");
    tick.adjust = adjust;
    tick.current = current;
}

/// Runs the game logic schedule a single time.
pub fn run_game_logic_on_server(world: &mut World) {
    world.run_schedule(GameLogic);
}

#[derive(Component)]
pub struct Player {
    pub id: PlayerId,
}

pub fn move_player_from_input(transform: &mut Transform, input: &Input) {
    const SPEED: f32 = 100.0;

    transform.translation.x += input.x as f32 * SPEED * TICK_TIME as f32;
    transform.translation.y += input.y as f32 * SPEED * TICK_TIME as f32;
}

pub fn handle_local_input(
    mut cmds: Commands,
    i_buf: Res<InputBuffer>,
    mut player: Query<&mut Transform, With<LocalPlayer>>,
) {
    if let Some(input) = i_buf.inputs.get(0) {
        if player.get_single_mut().is_err() {
            return;
        }
        let mut transform = player.get_single_mut().unwrap();
        move_player_from_input(&mut transform, input);

        if let Some(ref shot) = input.shoot {
            let entity = spawn_bullet(
                &mut cmds,
                Transform::from_translation(shot.origin.extend(0.0)),
                shot.direction * BULLET_SPEED,
                shot.server_id,
            );
            cmds.entity(entity).insert(Prespawned {
                behavior: PrespawnBehavior::Ignore,
            });
        }
    }
}

pub fn handle_clients_input(
    mut cmds: Commands,
    mut i_buf: ResMut<InputMapBuffer>,
    mut players: Query<(&mut Transform, &Player)>,
) {
    let Some(inputs) = i_buf.inputs.pop_front() else {
        return;
    };

    for (id, input) in inputs.iter() {
        for (mut transform, player) in players.iter_mut() {
            if *id == player.id {
                move_player_from_input(&mut transform, input);

                if let Some(ref shot) = input.shoot {
                    spawn_bullet(
                        &mut cmds,
                        Transform::from_translation(shot.origin.extend(0.0)),
                        shot.direction * BULLET_SPEED,
                        shot.server_id,
                    );
                }
            }
        }
    }
}

fn square(color: Color, size: f32) -> SpriteBundle {
    SpriteBundle {
        sprite: Sprite {
            color,
            custom_size: Some(Vec2::new(size, size)),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn spawn_player(
    cmds: &mut Commands,
    server_obj: u64,
    player_id: PlayerId,
    transform: Transform,
    is_local: bool,
) -> Entity {
    let mut builder = cmds.spawn((
        Player { id: player_id },
        ServerObject::from_u64(server_obj),
        SpriteBundle {
            sprite: Sprite {
                color: if is_local {
                    Color::rgb(0.0, 1.0, 0.0)
                } else {
                    Color::rgb(1.0, 0.0, 0.0)
                },
                custom_size: Some(Vec2::new(10.0, 10.0)),
                ..Default::default()
            },
            ..Default::default()
        },
        NetworkEntityTag::Player,
    ));
    builder.insert(transform);

    if is_local {
        builder.insert(LocalPlayer);
    }

    builder.id()
}

pub fn spawn_network_entities_on_client(
    mut cmds: Commands,
    msgs: Res<ClientMessages>,
    c_info: Res<ClientInfo>,
    prespawns: Query<(Entity, &Prespawned, &ServerObject)>,
    objs: Query<(Entity, &ServerObject, &NetworkEntityTag)>,
) {
    for msg in msgs.reliable.iter() {
        match msg {
            RUMFromServer::PlayerJoined {
                server_obj,
                id,
                transform,
            } => {
                spawn_player(&mut cmds, *server_obj, *id, *transform, c_info.id == *id);
            }
            RUMFromServer::EntitySpawn(spawn) => match &spawn.data {
                NetworkEntityType::Player { id, transform } => {
                    if *id == c_info.id {
                        warn!("got spawn request for current client");
                        continue;
                    }
                    spawn_player(
                        &mut cmds,
                        spawn.server_id,
                        *id,
                        *transform,
                        c_info.id == *id,
                    );
                }
                NetworkEntityType::NPC { transform } => {
                    let mut e = cmds.spawn((
                        square(Color::BLUE, 10.0),
                        ServerObject::from_u64(spawn.server_id),
                        NetworkEntityTag::NPC,
                    ));
                    e.insert(TransformBundle::from_transform(*transform));
                }
                NetworkEntityType::Bullet { bullet, transform } => {
                    // TODO: refactor
                    let mut should_spawn = true;
                    for (entity, prespawn, server_obj) in prespawns.iter() {
                        if server_obj.as_u64() == spawn.server_id {
                            match prespawn.behavior {
                                PrespawnBehavior::Ignore => {
                                    should_spawn = false;
                                }
                                PrespawnBehavior::Replace => {
                                    cmds.entity(entity).despawn_recursive();
                                }
                            }
                        }
                    }
                    if should_spawn {
                        spawn_bullet(&mut cmds, *transform, bullet.velocity, spawn.server_id);
                    }
                }
            },
            RUMFromServer::EntityDespawn { server_id } => {
                for (e, obj, tag) in objs.iter() {
                    if obj.as_u64() == *server_id {
                        info!("server said to despawn {:?}", tag);
                        cmds.entity(e).despawn_recursive();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn despawn_disconnected_players_on_client(
    mut cmds: Commands,
    msgs: Res<ClientMessages>,
    players: Query<(Entity, &ServerObject)>,
) {
    for msg in msgs.reliable.iter() {
        if let RUMFromServer::PlayerLeft { server_obj } = msg {
            for (entity, so) in players.iter() {
                if so.as_u64() == *server_obj {
                    cmds.entity(entity).despawn_recursive();
                }
            }
        }
    }
}

pub fn spawn_npc_on_server(mut cmds: Commands) {
    cmds.spawn((
        NPC {
            target: None,
            timer: Timer::new(Duration::from_secs(5), TimerMode::Once),
        },
        square(Color::BLUE, 10.0),
        ServerObject::from_u64(rand::random()),
        NetworkEntityTag::NPC,
    ));
}

#[derive(Component)]
pub struct NPC {
    target: Option<(f32, f32)>,
    timer: Timer,
}

pub fn move_npc_on_server(mut npcs: Query<(&mut Transform, &mut NPC)>, time: Res<Time>) {
    for (mut transform, mut npc) in npcs.iter_mut() {
        npc.timer.tick(time.delta());
        if npc.timer.just_finished() {
            match npc.target {
                Some(_) => {
                    npc.target = None;
                }
                None => {
                    npc.target = Some((
                        rand::random::<f32>() * 200.0 - 100.0,
                        rand::random::<f32>() * 200.0 - 100.0,
                    ))
                }
            }
            npc.timer = Timer::new(
                Duration::from_secs_f32(rand::random::<f32>() * 5.0),
                TimerMode::Once,
            );
        }

        match npc.target {
            Some((x, y)) => {
                const SPEED: f32 = 30.0;

                let diff = Vec2::new(x - transform.translation.x, y - transform.translation.y);
                let mag = SPEED * TICK_TIME as f32;

                // Avoid overshooting.
                if diff.length() < mag {
                    transform.translation = Vec2::new(x, y).extend(0.0);
                } else {
                    let dir = diff.normalize_or_zero();
                    transform.translation += (dir * mag).extend(0.0);
                }
            }
            None => {}
        }
    }
}

#[derive(Serialize, Deserialize, Component, Clone)]
pub struct Bullet {
    velocity: Vec2,
}

pub fn spawn_startup_bullet(mut cmds: Commands) {
    spawn_bullet(
        &mut cmds,
        Transform::default(),
        Vec2::Y * BULLET_SPEED,
        rand::random(),
    );
}

fn spawn_bullet(
    cmds: &mut Commands,
    transform: Transform,
    velocity: Vec2,
    server_id: u64,
) -> Entity {
    let bullet = Bullet { velocity };
    let mut b = cmds.spawn((
        bullet,
        square(Color::ORANGE, 5.0),
        Deterministic::<Transform>::default(),
        ServerObject::from_u64(server_id),
        NetworkEntityTag::Bullet,
    ));
    b.insert(TransformBundle::from_transform(transform));
    b.id()
}

pub fn move_bullet(mut cmds: Commands, mut q: Query<(Entity, &mut Transform, &Bullet)>) {
    for bullet in q.iter_mut() {
        let (entity, mut transform, bullet) = bullet;
        transform.translation += bullet.velocity.extend(0.0) * TICK_TIME as f32;

        if transform.translation.x.abs() > 1000.0 || transform.translation.y.abs() > 1000.0 {
            cmds.entity(entity).despawn_recursive();
        }
    }
}
