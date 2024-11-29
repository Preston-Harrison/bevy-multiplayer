use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::shared::{
    objects::{
        grounded::Grounded,
        gun::{Gun, GunType},
        LastSyncTracker, NetworkObject,
    },
    proc::LoadsChunks,
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

pub fn spawn_players_from_spawn_requests(
    mut player_spawn_reqs: EventReader<PlayerSpawnRequest>,
    mut commands: Commands,
) {
    for req in player_spawn_reqs.read() {
        match req {
            PlayerSpawnRequest::Server(transform, net_obj) => {
                commands
                    .spawn((
                        Player::new(),
                        KinematicCharacterController::default(),
                        RigidBody::KinematicPositionBased,
                        Collider::capsule_y(0.5, 0.25),
                        SpatialBundle::from_transform(*transform),
                        Grounded::default(),
                        net_obj.clone(),
                        LoadsChunks,
                    ))
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
                        KinematicCharacterController::default(),
                        RigidBody::KinematicPositionBased,
                        Collider::capsule_y(0.5, 0.25),
                        SpatialBundle::from_transform(*transform),
                        Grounded::default(),
                        net_obj.clone(),
                        LoadsChunks,
                        LocalPlayerTag,
                        LastSyncTracker::<Transform>::new(tick.clone()),
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
                            ..default()
                        },
                    ))
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
                        KinematicCharacterController::default(),
                        RigidBody::KinematicPositionBased,
                        Collider::capsule_y(0.5, 0.25),
                        SpatialBundle::from_transform(*transform),
                        net_obj.clone(),
                        LastSyncTracker::<Transform>::new(tick.clone()),
                    ))
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
