/// Most of this is taken from https://github.com/hsaikia/ProceduralTreesBevy
/// Thanks!
use bevy::prelude::*;

#[derive(Debug, Resource)]
pub struct Params {
    pub root_transform: Transform,
    pub children: u8,                  
    pub levels: u8,                    
    pub child_translation_factor: f32, 
    pub angle_from_parent_branch: f32, 
    pub child_scale: f32,              
    pub base_radius: f32,              
    pub leaf_radius: f32,              
    pub branch_height: f32,
}

impl Params {
    pub fn new_desert_tree() -> Self {
        Self {
            root_transform: Transform::default(),
            children: 4,
            levels: 4,
            child_translation_factor: 1.0,
            angle_from_parent_branch: 1.0,
            child_scale: 0.6,
            base_radius: 0.15,
            leaf_radius: 0.6,
            branch_height: 3.0
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Self {
            root_transform: Transform::default(),
            children: 4,
            levels: 4,
            child_translation_factor: 2.0,
            angle_from_parent_branch: 0.4,
            child_scale: 0.6,
            base_radius: 0.15,
            leaf_radius: 0.4,
            branch_height: 2.0,
        }
    }
}

pub struct Branch {
    pub transform: Transform,
    pub parent: Option<usize>,
    pub is_leaf: bool,
}

fn generate_leaves(parent_idx: usize, all: &mut Vec<Branch>) {
    let mut child_transform = Transform::IDENTITY;
    child_transform = child_transform.with_translation(*child_transform.local_y());
    all.push(Branch {
        transform: child_transform,
        parent: Some(parent_idx),
        is_leaf: true,
    });
}

fn generate_branches(params: &Params, level: u8, parent_idx: usize, all: &mut Vec<Branch>) {
    assert!(level >= 1);
    assert!(params.children > 1);
    for i in 0..params.children {
        let angle_from_root_branch = params.angle_from_parent_branch;
        let child_gap_f32 = f32::from(i) / f32::from(params.children);
        let angle_around_root_branch = 2.0 * std::f32::consts::PI * child_gap_f32;
        let child_idx_f32 = f32::from(i) / f32::from(params.children - 1);

        let translation_along_root =
            (1.0 - child_idx_f32) * params.child_translation_factor + child_idx_f32;

        let mut child_transform = Transform::IDENTITY;
        child_transform.rotate_local_y(angle_around_root_branch);
        child_transform = child_transform.with_translation(
            child_transform.local_z()
                * (params.base_radius + params.child_scale * 0.5 * angle_from_root_branch.sin())
                + child_transform.local_y()
                    * ((translation_along_root - 0.5)
                        + params.child_scale * 0.5 * angle_from_root_branch.cos()),
        );
        child_transform.rotate_local_x(angle_from_root_branch);
        child_transform = child_transform.with_scale(Vec3::splat(params.child_scale));

        let child_idx = all.len();
        all.push(Branch {
            transform: child_transform,
            parent: Some(parent_idx),
            is_leaf: false,
        });
        if level < params.levels {
            generate_branches(params, level + 1, child_idx, all);
        } else {
            generate_leaves(child_idx, all);
        }
    }
}

pub fn generate(params: &Params) -> Vec<Branch> {
    let mut ret: Vec<Branch> = Vec::new();
    ret.push(Branch {
        transform: params.root_transform,
        parent: None,
        is_leaf: false,
    });
    generate_branches(params, 1, 0, &mut ret);
    ret
}

/// This component indicates the root entity for our tree
#[derive(Component)]
pub struct TreeRoot;

pub fn render_tree(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    params: &Params,
) {
    // Generate and add the new tree
    let tree = generate(&params);
    let mut entity_parent_indices: Vec<(Entity, Option<usize>)> = Vec::new();

    let t = params.angle_from_parent_branch * 2.0 / std::f32::consts::PI;
    let color_r = (1.0 - t * 2.0).max(0.0);
    let color_g = if t < 0.5 { 2.0 * t } else { 2.0 - 2.0 * t };
    let color_b = (t * 2.0 - 1.0).max(0.0);

    let leaf_mesh = meshes.add(Sphere::new(params.leaf_radius).mesh().ico(2).unwrap());
    let leaf_material = materials.add(Color::srgb(color_r, color_g, color_b));
    let branch_mesh = meshes.add(Cylinder::new(params.base_radius, params.branch_height));
    let branch_material = materials.add(Color::srgb(0.8, 0.7, 0.6));

    for branch in &tree {
        if branch.is_leaf {
            // leaves are spheres
            let entity_id = commands
                .spawn(PbrBundle {
                    mesh: leaf_mesh.clone(),
                    transform: branch.transform,
                    material: leaf_material.clone(),
                    ..default()
                })
                .id();
            entity_parent_indices.push((entity_id, branch.parent));
        } else {
            // cylinders (tree branches)
            let entity_id = commands
                .spawn(PbrBundle {
                    mesh: branch_mesh.clone(),
                    transform: branch.transform,
                    material: branch_material.clone(),
                    ..default()
                })
                .id();
            entity_parent_indices.push((entity_id, branch.parent));
        }

        //println!("{:?} -> {:?} Parent {:?}", entity_id, branch.0, branch.1);
    }

    for (child_id, par_id) in &entity_parent_indices {
        if par_id.is_some() {
            let parent_id = entity_parent_indices[par_id.unwrap()].0;
            commands.entity(parent_id).push_children(&[*child_id]);
            //println!("Child {:?} -> Parent {:?}", child_id, parent_id);
        }
    }

    // Add the TreeRoot component to the root node
    commands.entity(entity_parent_indices[0].0).insert(TreeRoot);
}
