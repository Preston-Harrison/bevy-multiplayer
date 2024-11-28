use bevy::{
    color::palettes::css::{BLUE, GREEN, RED},
    prelude::*,
};
use bevy_inspector_egui::quick::WorldInspectorPlugin;

use crate::utils::freecam::{FreeCamera, FreeCameraPlugin};

pub fn run() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FreeCameraPlugin,
            WorldInspectorPlugin::new(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (snap_to_game_cam, toggle_collider_visual))
        .run();
}

#[derive(Component)]
struct GameCamera;

#[derive(Component)]
struct PlayerColliderVisual;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        FreeCamera::new(10.0),
    ));

    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });

    commands
        .spawn((
            PbrBundle {
                mesh: meshes.add(Capsule3d::new(0.35, 1.0)),
                material: materials.add(StandardMaterial {
                    base_color: BLUE.into(),
                    ..default()
                }),
                transform: Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
                ..default()
            },
            PlayerColliderVisual,
            Name::new("Player Collider Visual"),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    GameCamera,
                    TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                        0.0, 1.5, 0.0,
                    ))),
                    Name::new("Game Camera"),
                ))
                .with_children(|parent| {
                    parent
                        .spawn((
                            SceneBundle {
                                scene: asset_server.load(
                                    GltfAssetLabel::Scene(0)
                                        .from_asset("kenney-weapons/blasterD.glb"),
                                ),
                                transform: Transform::from_translation(Vec3::new(0.2, -1.2, -0.9))
                                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, 3.1, 0.0)),
                                ..default()
                            },
                            Name::new("Gun"),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                PbrBundle {
                                    mesh: meshes.add(Sphere::new(0.02)),
                                    material: materials.add(StandardMaterial {
                                        base_color: RED.into(),
                                        ..Default::default()
                                    }),
                                    transform: Transform::from_translation(Vec3::new(
                                        -0.15, 0.04, 0.28,
                                    )),
                                    ..Default::default()
                                },
                                Name::new("Bullet Point"),
                            ));
                        });
                });
        });

    commands.spawn(PbrBundle {
        mesh: meshes.add(Plane3d::new(Vec3::Y, Vec2::new(10.0, 10.0))),
        material: materials.add(StandardMaterial {
            base_color: GREEN.into(),
            ..default()
        }),
        ..default()
    });
}

fn snap_to_game_cam(
    key: Res<ButtonInput<KeyCode>>,
    game_cam: Query<&Transform, (With<GameCamera>, Without<Camera>)>,
    mut cam: Query<&mut Transform, With<Camera>>,
) {
    if key.just_pressed(KeyCode::KeyP) {
        let game_cam_t = game_cam.single();
        let mut cam_t = cam.single_mut();
        *cam_t = *game_cam_t;
    }
}

fn toggle_collider_visual(
    key: Res<ButtonInput<KeyCode>>,
    mut player: Query<&mut Visibility, With<PlayerColliderVisual>>,
) {
    let Ok(mut player) = player.get_single_mut() else {
        return;
    };

    if key.just_pressed(KeyCode::KeyH) {
        *player = match *player {
            Visibility::Hidden => Visibility::Visible,
            Visibility::Visible => Visibility::Hidden,
            Visibility::Inherited => Visibility::Hidden,
        };
    };
}
