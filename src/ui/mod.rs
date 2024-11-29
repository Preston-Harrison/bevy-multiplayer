use bevy::{prelude::*, render::view::RenderLayers};

use crate::shared::render::UI_CAMERA_ORDER;

pub struct UIPlugin {
    pub is_server: bool,
}

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        if !self.is_server {
            app.add_systems(FixedUpdate, spawn_crosshair);
        }
    }
}

#[derive(Default)]
struct IsSpawned(bool);

#[derive(Component)]
struct UICamera;

fn spawn_crosshair(
    mut gizmos: Gizmos,
    mut commands: Commands,
    mut is_spawned: Local<IsSpawned>,
    asset_server: Res<AssetServer>,
) {
    if is_spawned.0 {
        return;
    }
    is_spawned.0 = true;

    commands.spawn((
        Camera2dBundle {
            camera: Camera {
                order: UI_CAMERA_ORDER,
                ..default()
            },
            ..default()
        },
        IsDefaultUiCamera,
        RenderLayers::layer(1),
    ));

    info!("spawning crosshair");
    let crosshair: Handle<Image> = asset_server.load("crosshair.png");
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ..default()
        })
        .insert(RenderLayers::layer(1))
        .with_children(|parent| {
            parent
                .spawn((
                    NodeBundle {
                        style: Style {
                            width: Val::Px(64.0),
                            height: Val::Px(64.0),
                            ..default()
                        },
                        ..default()
                    },
                    UiImage::new(crosshair),
                ))
                .insert(RenderLayers::layer(1));
        });
}
