use bevy::{prelude::*, render::view::RenderLayers};

use crate::shared::render::{UI_CAMERA_ORDER, UI_RENDER_LAYER};

pub struct UIPlugin {
    pub is_server: bool,
}

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        if !self.is_server {
            app.add_systems(FixedUpdate, (spawn_crosshair, health::spawn_health_bar));
            app.add_systems(Update, health::draw_local_health_bar);
        }
    }
}

#[derive(Default)]
struct IsSpawned(bool);

fn spawn_crosshair(
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
        RenderLayers::layer(UI_RENDER_LAYER),
    ));

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
        .insert(RenderLayers::layer(UI_RENDER_LAYER))
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
                .insert(RenderLayers::layer(UI_RENDER_LAYER));
        });
}

pub mod health {
    use bevy::{
        color::palettes::css::{GREEN, GREY},
        prelude::*,
        render::view::RenderLayers,
    };

    use crate::shared::render::UI_RENDER_LAYER;

    #[derive(Component)]
    pub struct Health {
        current: i32,
        max: i32,
    }

    #[derive(Default)]
    pub struct IsSpawned(bool);

    pub fn spawn_health_bar(mut is_spawned: Local<IsSpawned>, mut commands: Commands) {
        if is_spawned.0 {
            return;
        }
        is_spawned.0 = true;

        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        position_type: PositionType::Absolute,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::top(Val::Percent(5.0)),
                        ..default()
                    },
                    ..default()
                },
                RenderLayers::layer(UI_RENDER_LAYER),
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        NodeBundle {
                            style: Style {
                                width: Val::Px(500.0),
                                height: Val::Px(32.0),
                                align_items: AlignItems::Start,
                                ..default()
                            },
                            background_color: BackgroundColor(GREY.into()),
                            ..default()
                        },
                        RenderLayers::layer(UI_RENDER_LAYER),
                        Health {
                            current: 50,
                            max: 100,
                        },
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            NodeBundle {
                                style: Style {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                background_color: BackgroundColor(GREEN.into()),
                                ..default()
                            },
                            RenderLayers::layer(UI_RENDER_LAYER),
                        ));
                    });
            });
    }

    pub fn draw_local_health_bar(
        health: Query<(&Health, &Children)>,
        mut width: Query<&mut Style>,
    ) {
        let Ok((health_bar, children)) = health.get_single() else {
            warn!("no health.get_single()");
            return;
        };
        let Some(child) = children.get(0) else {
            warn!("children");
            return;
        };

        let Ok(mut health_bar_style) = width.get_mut(*child) else {
            warn!("no style");
            return;
        };

        health_bar_style.width =
            Val::Percent(100.0 * health_bar.current as f32 / health_bar.max as f32);
    }
}
