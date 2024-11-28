use bevy::prelude::*;

pub struct GunPlugin;

impl Plugin for GunPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, spawn_gun_model);
    }
}

#[derive(Component)]
pub struct Gun {
    gun_type: GunType,
    bullet_point: Option<Entity>,
}

impl Gun {
    pub fn new(gun_type: GunType) -> Self {
        Self {
            gun_type,
            bullet_point: None,
        }
    }
}

pub enum GunType {
    PurpleRifle,
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
