use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::{
    netcode::{
        input::{Input, InputBuffer, InputMapBuffer},
        read::ClientMessages,
        tick::Tick,
        ClientInfo, LocalPlayer, PlayerId, RUMFromServer, ServerObject,
    },
    TICK_TIME,
};

#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct GameLogic;

pub fn run_game_logic_on_client(world: &mut World) {
    let tick = world.get_resource::<Tick>().expect("tick must exist");
    let mut adjust = tick.adjust;
    let mut current = tick.current;

    info!("adjustment is {adjust}");

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
    info!("current tick is {current}");
    tick.current = current;
}

pub fn run_game_logic_on_server(world: &mut World) {
    world.run_schedule(GameLogic);
}

#[derive(Component)]
pub struct Player {
    pub id: PlayerId,
}

pub fn move_player(transform: &mut Transform, input: &Input) {
    const SPEED: f32 = 100.0;

    transform.translation.x += input.x as f32 * SPEED * TICK_TIME as f32;
    transform.translation.y += input.y as f32 * SPEED * TICK_TIME as f32;
}

pub fn move_on_client(
    i_buf: Res<InputBuffer>,
    mut player: Query<&mut Transform, With<LocalPlayer>>,
    tick: Res<Tick>,
) {
    if let Some(input) = i_buf.inputs.get(0) {
        if player.get_single_mut().is_err() {
            return;
        }
        let mut transform = player.get_single_mut().unwrap();
        info!("moving from input tick {}", tick.current);
        move_player(&mut transform, input);
    }
}

pub fn move_on_server(
    mut i_buf: ResMut<InputMapBuffer>,
    mut players: Query<(&mut Transform, &Player)>,
) {
    let Some(inputs) = i_buf.inputs.pop_front() else {
        return;
    };

    for (id, input) in inputs.iter() {
        for (mut transform, player) in players.iter_mut() {
            if *id == player.id {
                move_player(&mut transform, input);
            }
        }
    }
}

pub fn spawn_player(
    cmds: &mut Commands,
    server_obj: u64,
    player_id: PlayerId,
    transform: Transform,
    is_local: bool,
) {
    let mut builder = cmds.spawn((
        Player { id: player_id },
        ServerObject::from_u64(server_obj),
        SpriteBundle {
            sprite: Sprite {
                color: if is_local {
                    Color::rgb(0.0, 0.0, 1.0)
                } else {
                    Color::rgb(1.0, 0.0, 0.0)
                },
                custom_size: Some(Vec2::new(10.0, 10.0)),
                ..Default::default()
            },
            ..Default::default()
        },
    ));
    builder.insert(transform);

    if is_local {
        builder.insert(LocalPlayer);
    }
}

pub fn spawn_joined_players_on_client(
    mut cmds: Commands,
    msgs: Res<ClientMessages>,
    c_info: Res<ClientInfo>,
) {
    for msg in msgs.reliable.iter() {
        if let RUMFromServer::PlayerJoined {
            server_obj,
            id,
            transform,
        } = msg
        {
            spawn_player(&mut cmds, *server_obj, *id, *transform, c_info.id == *id)
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
