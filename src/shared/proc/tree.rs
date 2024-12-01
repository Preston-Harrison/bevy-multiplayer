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
pub struct Tree {
    tree_type: TreeType,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            tree_type: TreeType::Default,
        }
    }

    pub fn rand(seed: u64) -> Self {
        let tree_type = match seed % 9 {
            0 => TreeType::Default,
            1 => TreeType::Cone,
            2 => TreeType::Fat,
            3 => TreeType::Oak,
            4 => TreeType::Simple,
            5 => TreeType::Small,
            6 => TreeType::Thin,
            7 => TreeType::Plateau,
            8 => TreeType::Detailed,
            _ => panic!("seed mod 9 not < 8"),
        };
        Self { tree_type }
    }
}

pub enum TreeType {
    Default,
    Cone,
    Fat,
    Oak,
    Simple,
    Small,
    Thin,
    Plateau,
    Detailed,
}

impl TreeType {
    fn scene_path(&self) -> &'static str {
        match self {
            TreeType::Default => "kenny-nature/tree_default_dark.glb",
            TreeType::Cone => "kenny-nature/tree_cone_dark.glb",
            TreeType::Fat => "kenny-nature/tree_fat_dark.glb",
            TreeType::Oak => "kenny-nature/tree_oak_dark.glb",
            TreeType::Simple => "kenny-nature/tree_simple_dark.glb",
            TreeType::Small => "kenny-nature/tree_small_dark.glb",
            TreeType::Thin => "kenny-nature/tree_thin_dark.glb",
            TreeType::Plateau => "kenny-nature/tree_plateau_dark.glb",
            TreeType::Detailed => "kenny-nature/tree_detailed_dark.glb",
        }
    }

    fn to_mesh(&self, meshes: &mut TreeMeshes, asset_server: &AssetServer) -> Handle<Scene> {
        let load_mesh =
            || asset_server.load(GltfAssetLabel::Scene(0).from_asset(self.scene_path()));
        let mesh_handle = match self {
            Self::Default => &mut meshes.dark_default,
            Self::Cone => &mut meshes.dark_cone,
            Self::Fat => &mut meshes.dark_fat,
            Self::Oak => &mut meshes.dark_oak,
            Self::Simple => &mut meshes.dark_simple,
            Self::Small => &mut meshes.dark_small,
            Self::Thin => &mut meshes.dark_thin,
            Self::Plateau => &mut meshes.dark_plateau,
            Self::Detailed => &mut meshes.dark_detailed,
        };
        mesh_handle.get_or_insert_with(load_mesh).clone()
    }
}

#[derive(Resource, Default)]
struct TreeMeshes {
    dark_default: Option<Handle<Scene>>,
    dark_cone: Option<Handle<Scene>>,
    dark_fat: Option<Handle<Scene>>,
    dark_oak: Option<Handle<Scene>>,
    dark_simple: Option<Handle<Scene>>,
    dark_small: Option<Handle<Scene>>,
    dark_thin: Option<Handle<Scene>>,
    dark_plateau: Option<Handle<Scene>>,
    dark_detailed: Option<Handle<Scene>>,
}

/// PERF: This can spawn alot of colliders, which slows the frame rate a little.
/// Might be worth spawning colliders only while near the player.
fn spawn_trees(
    mut tree_meshes: ResMut<TreeMeshes>,
    new_trees: Query<(&Tree, Entity), Added<Tree>>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    for (tree, entity) in new_trees.iter() {
        if let Some(mut entity) = commands.get_entity(entity) {
            entity.with_children(|parent| {
                parent.spawn(SceneBundle {
                    scene: tree.tree_type.to_mesh(&mut tree_meshes, &asset_server),
                    transform: Transform::default().with_scale(Vec3::splat(4.0)),
                    ..Default::default()
                });
                // parent.spawn((
                //     RigidBody::Fixed,
                //     Collider::cylinder(0.5, 0.05),
                //     SpatialBundle::from_transform(Transform::from_xyz(0.0, 0.5, 0.0)),
                // ));
            });
        }
    }
}
