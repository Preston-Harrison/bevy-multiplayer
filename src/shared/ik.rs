use bevy::ecs::query::QueryEntityError;
use bevy::prelude::*;

/// The inverse kinematics plugin.
/// Should be added to your app if you want to use this crate.
pub struct InverseKinematicsPlugin;

/// Inverse kintematics constraint to be added to the tail joint of
/// a chain of bones. The solver will attempt to match the global translation of
/// of this entity to that of the target.
#[derive(Component, Reflect)]
pub struct IkConstraint {
    /// How many bones are included in the IK constraint.
    pub chain_length: usize,
    /// Maximum number of iterations to solve this constraint.
    pub iterations: usize,
    /// Target entity. The target must have a `Transform` and `GlobalTransform`.
    pub target: Entity,
    /// Target entity for pole rotation.
    /// If this would be a person's arm, this is the direction that the elbow will point.
    pub pole_target: Option<Entity>,
    /// Pole target roll offset.
    /// If a pole target is set, the bone will roll toward the pole target.
    /// This angle is the offset to apply to the roll.
    pub pole_angle: f32,
    /// Whether this constraint is enabled. Disabled constraints will be skipped.
    pub enabled: bool,
}

impl Plugin for InverseKinematicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, inverse_kinematics_system);
        app.register_type::<IkConstraint>();
    }
}

struct PoleTarget {
    origin: Vec3,
    tangent: Vec3,
    normal: Vec3,
    angle: f32,
}

pub fn inverse_kinematics_system(
    query: Query<(Entity, &IkConstraint)>,
    parents: Query<&Parent>,
    mut transforms: Query<(&mut Transform, &mut GlobalTransform)>,
    names: Query<&Name>,
) {
    for (entity, constraint) in query.iter() {
        if !constraint.enabled {
            continue;
        }

        if let Err(e) = constraint.solve(entity, &parents, &mut transforms, &names) {
            bevy::log::warn!("Failed to solve IK constraint: {e}");
        }
    }
}

impl IkConstraint {
    pub fn solve(
        &self,
        entity: Entity,
        parents: &Query<&Parent>,
        transforms: &mut Query<(&mut Transform, &mut GlobalTransform)>,
        names: &Query<&Name>,
    ) -> Result<(), QueryEntityError> {
        if self.chain_length == 0 {
            return Ok(());
        }

        let mut joints = Vec::with_capacity(self.chain_length + 2);
        joints.push(entity);
        for i in 0..self.chain_length + 1 {
            joints.push(parents.get(joints[i])?.get());
        }
        info!("joints");

        let target = transforms.get(self.target)?.1.translation();
        info!("target");
        let name = names.get(joints[0]).unwrap();
        info!("name: {}", name);
        let normal = transforms.get(joints[0])?.0.translation;
        info!("normal");

        let pole_target = if let Some(pole_target) = self.pole_target {
            let start = transforms.get(joints[self.chain_length])?.1.translation();
            let pole_target = transforms.get(pole_target)?.1.translation();

            let tangent = (target - start).normalize();
            let axis = (pole_target - start).cross(tangent);
            let normal = tangent.cross(axis).normalize();

            Some(PoleTarget {
                origin: start,
                tangent,
                normal,
                angle: self.pole_angle,
            })
        } else {
            None
        };

        for _ in 0..self.iterations {
            let result = Self::solve_recursive(
                &joints[1..],
                normal,
                target,
                pole_target.as_ref(),
                transforms,
                names,
            )?;

            if result.transform_point(normal).distance_squared(target) < 0.001 {
                return Ok(());
            }
        }

        Ok(())
    }

    fn solve_recursive(
        chain: &[Entity],
        normal: Vec3,
        target: Vec3,
        pole_target: Option<&PoleTarget>,
        transforms: &mut Query<(&mut Transform, &mut GlobalTransform)>,
        names: &Query<&Name>,
    ) -> Result<GlobalTransform, QueryEntityError> {
        let (&transform, &global_transform) = match transforms.get(chain[0]) {
            Ok(value) => value,
            Err(err) => {
                error!("cannot find transform for {:?}", names.get(chain[0]));
                return Err(err);
            }
        };

        if chain.len() == 1 {
            return Ok(global_transform);
        }

        let parent_normal = transform.translation;

        // determine absolute rotation and translation for this bone where the tail touches the
        // target.
        let rotation = if let Some(pt) = pole_target {
            let on_pole = pt.origin
                + (global_transform.translation() - pt.origin).project_onto_normalized(pt.tangent);
            let distance = on_pole.distance(global_transform.translation());
            let from_position = on_pole + pt.normal * distance;

            let base = Quat::from_rotation_arc(normal.normalize(), Vec3::Z);

            let forward = (target - from_position)
                .try_normalize()
                .unwrap_or(pt.tangent);
            let up = forward.cross(pt.normal).normalize();
            let right = up.cross(forward);
            let orientation = Mat3::from_cols(right, up, forward) * Mat3::from_rotation_z(pt.angle);

            (Quat::from_mat3(&orientation) * base).normalize()
        } else {
            Quat::from_rotation_arc(
                normal.normalize(),
                (target - global_transform.translation()).normalize(),
            )
            .normalize()
        };
        let translation = target - rotation.mul_vec3(normal);

        // recurse to target the parent towards the current translation
        let parent_global_transform = Self::solve_recursive(
            &chain[1..],
            parent_normal,
            translation,
            pole_target,
            transforms,
            names,
        )?;

        // apply constraints on the way back from recursing
        let (mut transform, mut global_transform) = transforms.get_mut(chain[0]).unwrap();
        transform.rotation = Quat::from_affine3(&parent_global_transform.affine())
            .inverse()
            .normalize()
            * rotation;
        *global_transform = parent_global_transform.mul_transform(*transform);

        Ok(*global_transform)
    }
}
