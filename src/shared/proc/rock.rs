use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

pub struct RockPlugin;

impl Plugin for RockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RockMeshes>();
        app.add_systems(Update, spawn_trees);
    }
}

#[derive(Component)]
pub struct Rock {}

impl Rock {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Resource, Default)]
struct RockMeshes {
    large_a: Option<Handle<Scene>>,
}

/// PERF: This can spawn alot of colliders, which slows the frame rate a little.
/// Might be worth spawning colliders only while near the player.
fn spawn_trees(
    mut rock_meshes: ResMut<RockMeshes>,
    new_trees: Query<Entity, Added<Rock>>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    let tree_mesh = match &rock_meshes.large_a {
        Some(handle) => handle.clone(),
        None => {
            let handle = asset_server
                .load(GltfAssetLabel::Scene(0).from_asset("kenny-nature/rock_largeA.glb"));
            rock_meshes.large_a = Some(handle.clone());
            handle
        }
    };

    for entity in new_trees.iter() {
        if let Some(mut entity) = commands.get_entity(entity) {
            entity.with_children(|parent| {
                parent.spawn(SceneBundle {
                    scene: tree_mesh.clone(),
                    ..Default::default()
                });
                parent.spawn((
                    RigidBody::Fixed,
                    Collider::cylinder(0.5, 0.05),
                    SpatialBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
                ));
            });
        }
    }
}
