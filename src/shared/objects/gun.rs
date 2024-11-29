use std::time::Duration;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct GunPlugin;

impl Plugin for GunPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, spawn_gun_model);
    }
}

#[derive(Component)]
pub struct LocalPlayerGun;

#[derive(Component, Debug)]
pub struct Gun {
    pub gun_type: GunType,
    pub bullet_point: Option<Entity>,
    pub last_fire_time: Duration,
}

impl Gun {
    pub fn new(gun_type: GunType) -> Self {
        Self {
            gun_type,
            bullet_point: None,
            last_fire_time: Duration::default(),
        }
    }

    pub fn try_shoot(&mut self, elapsed: Duration) -> bool {
        let can_shoot = elapsed - self.last_fire_time >= self.gun_type.bullet_delay();
        if can_shoot {
            self.last_fire_time = elapsed;
        };
        can_shoot
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GunType {
    PurpleRifle,
}

impl GunType {
    pub fn range(&self) -> f32 {
        match self {
            Self::PurpleRifle => 30.0,
        }
    }

    pub fn damage(&self) -> f32 {
        match self {
            Self::PurpleRifle => 10.0,
        }
    }

    pub fn bullets_per_second(&self) -> f32 {
        match self {
            Self::PurpleRifle => 5.0,
        }
    }

    pub fn bullet_delay(&self) -> Duration {
        Duration::from_secs_f32(1.0 / self.bullets_per_second())
    }

    pub fn is_full_auto(&self) -> bool {
        match self {
            Self::PurpleRifle => true,
        }
    }
}

#[derive(Component)]
pub struct BulletPoint;

fn spawn_gun_model(
    mut commands: Commands,
    mut new_guns: Query<(Entity, &mut Gun), Added<Gun>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut gun) in new_guns.iter_mut() {
        match gun.gun_type {
            GunType::PurpleRifle => {
                info!("spawning purple");
                commands.entity(entity).with_children(|parent| {
                    parent
                        .spawn((
                            SceneBundle {
                                scene: asset_server.load(
                                    GltfAssetLabel::Scene(0)
                                        .from_asset("kenney-weapons/blasterD.glb"),
                                ),
                                transform: Transform::from_translation(Vec3::new(0.2, -0.2, -0.9))
                                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, 3.1, 0.0)),
                                ..default()
                            },
                            Name::new("Purple Rifle"),
                        ))
                        .with_children(|parent| {
                            let bullet_point = parent.spawn((
                                SpatialBundle::from_transform(Transform::from_translation(
                                    Vec3::new(-0.15, 0.04, 0.28),
                                )),
                                BulletPoint,
                                Name::new("Bullet Point"),
                            ));
                            gun.bullet_point = Some(bullet_point.id());
                        });
                });
            }
        }
    }
}
