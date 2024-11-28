use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

pub struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeMeshes>();
        app.add_systems(Update, spawn_trees);
    }
}

#[derive(Component)]
pub struct Tree {}

impl Tree {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Resource, Default)]
struct TreeMeshes {
    fall_blocky: Option<Handle<Scene>>,
}

fn spawn_trees(
    mut tree_meshes: ResMut<TreeMeshes>,
    new_trees: Query<Entity, Added<Tree>>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    let tree_mesh = match &tree_meshes.fall_blocky {
        Some(handle) => handle.clone(),
        None => {
            let handle =
                asset_server.load(GltfAssetLabel::Scene(0).from_asset("tree_blocks_fall.glb"));
            tree_meshes.fall_blocky = Some(handle.clone());
            handle
        }
    };

    for entity in new_trees.iter() {
        commands.entity(entity).insert((
            RigidBody::Fixed,
            Collider::cylinder(1.0, 0.05),
        )).with_children(|parent| {
            parent.spawn(SceneBundle {
                scene: tree_mesh.clone(),
                ..Default::default()
            });
        });
    }
}
