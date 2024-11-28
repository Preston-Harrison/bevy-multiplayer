/// Most of this is taken from https://github.com/hsaikia/ProceduralTreesBevy
/// Thanks!
use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
        render_asset::RenderAssetUsages,
    },
};

pub struct TreeSet {
    pub handles: Vec<TreeHandles>,
}

impl TreeSet {
    pub fn new(
        params: &[Params],
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let mut handles = vec![];
        for param in params {
            handles.push(TreeHandles::new(param, meshes, materials));
        }
        Self { handles }
    }
}

pub struct TreeHandles {
    pub branch_mesh: Handle<Mesh>,
    pub branch_material: Handle<StandardMaterial>,
    pub leaf_mesh: Handle<Mesh>,
    pub leaf_material: Handle<StandardMaterial>,
}

impl TreeHandles {
    pub fn new(
        params: &Params,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let mut mesh = Sphere::new(params.leaf_radius).mesh().ico(2).unwrap();
        mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
        let leaf_mesh = meshes.add(mesh);
        let leaf_material = materials.add(Color::srgb(0.0, 1.0, 0.0));

        let mut mesh = Cylinder::new(params.base_radius, 1.0)
            .mesh()
            .resolution(6)
            .segments(1)
            .build();
        mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
        let branch_mesh = meshes.add(mesh);
        let branch_material = materials.add(Color::srgb(0.8, 0.7, 0.6));

        Self {
            leaf_material,
            leaf_mesh,
            branch_material,
            branch_mesh,
        }
    }
}

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
    /// Adjusts the height of the trunk. Doesn't affect the rest of the tree.
    pub root_height: f32,
    /// This is calculatable, but I don't know how. Branches stick through the
    /// other side of their parent, so this value just offsets that a little.
    pub branch_correction: f32,
}

impl Params {
    pub fn new_desert_tree() -> Self {
        Self {
            root_transform: Transform::default(),
            children: 4,
            levels: 4,
            child_translation_factor: 0.8,
            angle_from_parent_branch: 1.0,
            child_scale: 0.5,
            base_radius: 0.10,
            leaf_radius: 0.6,
            root_height: 1.0,
            branch_correction: -0.1,
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Self::new_desert_tree()
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
        child_transform.translation += child_transform.local_z()
            * (params.branch_correction
                + params.base_radius
                + params.child_scale * 0.5 * angle_from_root_branch.sin());
        child_transform.translation += child_transform.local_y()
            * ((translation_along_root - 0.5)
                + params.child_scale * 0.5 * angle_from_root_branch.cos());
        child_transform = child_transform.with_scale(Vec3::splat(params.child_scale));
        child_transform.rotate_local_x(angle_from_root_branch);

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

/// Returns the parent entity.
pub fn render_tree(commands: &mut Commands, handles: &TreeHandles, params: &Params) -> Entity {
    // Generate and add the new tree
    let tree = generate(&params);
    let mut entity_parent_indices: Vec<(Entity, Option<usize>)> = Vec::new();
    info!("rendering {} branches", tree.len());

    let TreeHandles {
        branch_mesh,
        branch_material,
        leaf_mesh,
        leaf_material,
    } = handles;

    for branch in &tree {
        if branch.is_leaf {
            let entity_id = commands
                .spawn(PbrBundle {
                    mesh: leaf_mesh.clone(),
                    material: leaf_material.clone(),
                    transform: branch.transform,
                    ..default()
                })
                .id();
            entity_parent_indices.push((entity_id, branch.parent));
        } else {
            let mut transform = branch.transform;
            let entity_id = if branch.parent.is_none() {
                transform.translation.y += params.root_height * 0.5;
                commands
                    .spawn(SpatialBundle::from_transform(transform))
                    .with_children(|parent| {
                        parent.spawn(PbrBundle {
                            mesh: branch_mesh.clone(),
                            transform: Transform::default().with_scale(Vec3::new(
                                1.0,
                                params.root_height,
                                1.0,
                            )),
                            material: branch_material.clone(),
                            ..default()
                        });
                    })
                    .id()
            } else {
                commands
                    .spawn(PbrBundle {
                        mesh: branch_mesh.clone(),
                        transform,
                        material: branch_material.clone(),
                        ..default()
                    })
                    .id()
            };
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
    entity_parent_indices[0].0
}

/// https://gist.github.com/DGriffin91/e63e5f7a90b633250c2cf4bf8fd61ef8
fn combine_meshes(
    meshes: &[Mesh],
    transforms: &[Transform],
    use_normals: bool,
    use_tangents: bool,
    use_uvs: bool,
    use_colors: bool,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut tangets: Vec<[f32; 4]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let mut indices_offset = 0;

    if meshes.len() != transforms.len() {
        panic!(
            "meshes.len({}) != transforms.len({})",
            meshes.len(),
            transforms.len()
        );
    }

    for (mesh, trans) in meshes.iter().zip(transforms) {
        if let Indices::U32(mesh_indices) = &mesh.indices().unwrap() {
            let mat = trans.compute_matrix();

            let positions_len;

            if let Some(VertexAttributeValues::Float32x3(vert_positions)) =
                &mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                positions_len = vert_positions.len();
                for p in vert_positions {
                    positions.push(mat.transform_point3(Vec3::from(*p)).into());
                }
            } else {
                panic!("no positions")
            }

            if use_uvs {
                if let Some(VertexAttributeValues::Float32x2(vert_uv)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_UV_0)
                {
                    for uv in vert_uv {
                        uvs.push(*uv);
                    }
                } else {
                    panic!("no uvs")
                }
            }

            if use_normals {
                // Comment below taken from mesh_normal_local_to_world() in mesh_functions.wgsl regarding
                // transform normals from local to world coordinates:

                // NOTE: The mikktspace method of normal mapping requires that the world normal is
                // re-normalized in the vertex shader to match the way mikktspace bakes vertex tangents
                // and normal maps so that the exact inverse process is applied when shading. Blender, Unity,
                // Unreal Engine, Godot, and more all use the mikktspace method. Do not change this code
                // unless you really know what you are doing.
                // http://www.mikktspace.com/

                let inverse_transpose_model = mat.inverse().transpose();
                let inverse_transpose_model = Mat3 {
                    x_axis: inverse_transpose_model.x_axis.xyz(),
                    y_axis: inverse_transpose_model.y_axis.xyz(),
                    z_axis: inverse_transpose_model.z_axis.xyz(),
                };

                if let Some(VertexAttributeValues::Float32x3(vert_normals)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                {
                    for n in vert_normals {
                        normals.push(
                            inverse_transpose_model
                                .mul_vec3(Vec3::from(*n))
                                .normalize_or_zero()
                                .into(),
                        );
                    }
                } else {
                    panic!("no normals")
                }
            }

            if use_tangents {
                if let Some(VertexAttributeValues::Float32x4(vert_tangets)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
                {
                    for t in vert_tangets {
                        tangets.push(*t);
                    }
                } else {
                    panic!("no tangets")
                }
            }

            if use_colors {
                if let Some(VertexAttributeValues::Float32x4(vert_colors)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                {
                    for c in vert_colors {
                        colors.push(*c);
                    }
                } else {
                    panic!("no colors")
                }
            }

            for i in mesh_indices {
                indices.push(*i + indices_offset);
            }
            indices_offset += positions_len as u32;
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    if use_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }

    if use_tangents {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangets);
    }

    if use_uvs {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }

    if use_colors {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }

    mesh.insert_indices(Indices::U32(indices));

    mesh
}
