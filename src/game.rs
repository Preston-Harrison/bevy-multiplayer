use bevy::prelude::*;

use crate::netcode::{
    input::{Input, InputBuffer, InputMapBuffer},
    LocalPlayer, PlayerId, ServerObject,
};

#[derive(Component)]
pub struct Player {
    pub id: PlayerId,
}

pub fn move_player(transform: &mut Transform, input: &Input) {
    const SPEED: f32 = 5.0;

    transform.translation.x += input.x as f32 * SPEED;
    transform.translation.y += input.y as f32 * SPEED;
}

pub fn move_on_client(
    i_buf: Res<InputBuffer>,
    mut player: Query<&mut Transform, With<LocalPlayer>>,
) {
    if let Some(input) = i_buf.inputs.get(0) {
        let mut transform = player.get_single_mut().unwrap();
        move_player(&mut transform, input);
    }
}

pub fn move_on_server(i_buf: Res<InputMapBuffer>, mut players: Query<(&mut Transform, &Player)>) {
    let Some(inputs) = i_buf.inputs.get(0) else {
        return;
    };

    for (id, input) in inputs.iter() {
        for (mut transform, player) in players.iter_mut() {
            if *id == player.id {
                info!("moving player {id}");
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
    info!("spawning player");
    let mut builder = cmds.spawn((
        Player { id: player_id },
        ServerObject::from_u64(server_obj),
        SpriteBundle {
            sprite: Sprite {
                color: Color::rgb(1.0, 0.0, 0.0),
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
