use bevy::{color::palettes::css::BLUE, prelude::*, render::view::RenderLayers};
use bevy_rapier3d::prelude::*;

use crate::shared::{
    objects::{
        grounded::Grounded,
        gun::{Gun, GunType},
        health::Health,
        LastSyncTracker, NetworkObject,
    },
    proc::LoadsChunks,
    render::{DEFAULT_CAMERA_ORDER, DEFAULT_RENDER_LAYER},
    tick::Tick,
};

use super::{
    client::{PlayerCamera, PlayerCameraTarget},
    server::LastInputTracker,
    LocalPlayerTag, Player, PlayerHead,
};

#[derive(Event)]
pub enum PlayerSpawnRequest {
    Server(Transform, NetworkObject),
    Local(Transform, NetworkObject, Tick),
    Remote(Transform, NetworkObject, Tick),
}

#[derive(Bundle)]
struct PlayerPhysicsBundle {
    controller: KinematicCharacterController,
    collider: Collider,
    rigid_body: RigidBody,
}

impl Default for PlayerPhysicsBundle {
    fn default() -> Self {
        Self {
            controller: KinematicCharacterController::default(),
            collider: Collider::capsule_y(0.5, 0.25),
            rigid_body: RigidBody::KinematicPositionBased,
        }
    }
}

#[derive(Default)]
pub struct PlayerVisualHandles {
    mesh: Option<Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}

fn get_player_visual(
    handles: &mut PlayerVisualHandles,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> impl Bundle {
    let mesh = handles
        .mesh
        .get_or_insert_with(|| meshes.add(Capsule3d::new(0.25, 1.0).mesh()));
    let material = handles
        .material
        .get_or_insert_with(|| materials.add(StandardMaterial::from_color(BLUE)));
    PbrBundle {
        mesh: mesh.clone(),
        material: material.clone(),
        ..default()
    }
}

const PLAYER_HEALTH: f32 = 100.0;

pub fn spawn_players_from_spawn_requests(
    mut visual_handles: Local<PlayerVisualHandles>,
    mut player_spawn_reqs: EventReader<PlayerSpawnRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for req in player_spawn_reqs.read() {
        match req {
            PlayerSpawnRequest::Server(transform, net_obj) => {
                commands
                    .spawn((
                        Player::new(),
                        PlayerPhysicsBundle::default(),
                        Grounded::default(),
                        net_obj.clone(),
                        LoadsChunks,
                        get_player_visual(&mut visual_handles, &mut meshes, &mut materials),
                        Health::new(PLAYER_HEALTH),
                    ))
                    .insert(SpatialBundle::from_transform(*transform))
                    .insert(LastInputTracker::default())
                    .with_children(|parent| {
                        parent
                            .spawn((
                                PlayerHead,
                                SpatialBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
                            ))
                            .with_children(|parent| {
                                parent.spawn((
                                    SpatialBundle::default(),
                                    Gun::new(GunType::PurpleRifle),
                                ));
                            });
                    });
            }
            PlayerSpawnRequest::Local(transform, net_obj, tick) => {
                commands
                    .spawn((
                        Player::new(),
                        PlayerPhysicsBundle::default(),
                        SpatialBundle::from_transform(*transform),
                        Grounded::default(),
                        net_obj.clone(),
                        LoadsChunks,
                        LocalPlayerTag,
                        LastSyncTracker::<Transform>::new(*tick),
                        LastSyncTracker::<Health>::new(*tick),
                        Health::new(PLAYER_HEALTH),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            PlayerCameraTarget,
                            PlayerHead,
                            SpatialBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
                        ));
                    });
                commands
                    .spawn((
                        PlayerCamera,
                        Camera3dBundle {
                            projection: PerspectiveProjection {
                                fov: 60.0_f32.to_radians(),
                                ..default()
                            }
                            .into(),
                            camera: Camera {
                                order: DEFAULT_CAMERA_ORDER,
                                ..default()
                            },
                            ..default()
                        },
                    ))
                    .insert(RenderLayers::layer(DEFAULT_RENDER_LAYER))
                    .insert(SpatialBundle::default())
                    .with_children(|parent| {
                        // TODO: consider parenting this to the player head, not the camera.
                        parent.spawn((SpatialBundle::default(), Gun::new(GunType::PurpleRifle)));
                    });
            }
            PlayerSpawnRequest::Remote(transform, net_obj, tick) => {
                commands
                    .spawn((
                        Player::new(),
                        PlayerPhysicsBundle::default(),
                        net_obj.clone(),
                        LastSyncTracker::<Transform>::new(*tick),
                        LastSyncTracker::<Health>::new(*tick),
                        get_player_visual(&mut visual_handles, &mut meshes, &mut materials),
                        Health::new(PLAYER_HEALTH),
                    ))
                    .insert(SpatialBundle::from_transform(*transform))
                    .with_children(|parent| {
                        parent
                            .spawn((
                                PlayerHead,
                                SpatialBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
                            ))
                            .with_children(|parent| {
                                parent.spawn((
                                    SpatialBundle::default(),
                                    Gun::new(GunType::PurpleRifle),
                                ));
                            });
                    });
            }
        }
    }
}
