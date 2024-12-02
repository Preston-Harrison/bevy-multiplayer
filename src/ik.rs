use crate::shared::ik::*;
use bevy::{color::palettes::css, prelude::*, window::WindowResolution};

#[derive(Component)]
pub struct ManuallyTarget(Vec4);

pub fn run() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(800.0, 600.0),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(InverseKinematicsPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (setup_ik, manually_target))
        .run();
}

#[derive(Component)]
struct Leg;

#[derive(Component)]
struct Foot;

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn(SpatialBundle::default())
        .with_children(|parent| {
            parent.spawn(Camera3dBundle {
                transform: Transform::from_xyz(-0.5, 1.5, 2.5)
                    .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
                projection: bevy::render::camera::Projection::Perspective(PerspectiveProjection {
                    fov: std::f32::consts::FRAC_PI_4,
                    aspect_ratio: 1.0,
                    near: 0.1,
                    far: 100.0,
                }),
                ..default()
            });
        });

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: css::WHITE.into(),
            illuminance: 10000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(-8.0, 8.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(Plane3d::default().mesh().size(5.0, 5.0))),
        material: materials.add(StandardMaterial {
            base_color: css::WHITE.into(),
            ..default()
        }),
        ..default()
    });

    commands
        .spawn(PbrBundle {
            mesh: meshes.add(Sphere::new(0.1)),
            ..default()
        })
        .insert(Name::new("1"))
        .with_children(|parent| {
            parent
                .spawn(PbrBundle {
                    mesh: meshes.add(Sphere::new(0.1)),
                    ..default()
                })
                .insert(Name::new("2"))
                .with_children(|parent| {
                    parent
                        .spawn(PbrBundle {
                            mesh: meshes.add(Sphere::new(0.1)),
                            ..default()
                        })
                        .insert(Name::new("3"))
                        .with_children(|parent| {
                            parent
                                .spawn(PbrBundle {
                                    mesh: meshes.add(Sphere::new(0.1)),
                                    ..default()
                                })
                                .insert(Name::new("4"))
                                .insert(Foot);
                        });
                });
        });
}

fn setup_ik(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    foot: Query<Entity, Added<Foot>>,
) {
    // Try to get the entity for the right hand joint.
    let Ok(foot) = foot.get_single() else {
        return;
    };
    info!("setting up ik");
    let target = commands
        .spawn((
            PbrBundle {
                transform: Transform::from_xyz(0.3, 0.8, 0.2),
                mesh: meshes.add(Sphere::new(0.05).mesh().uv(7, 7)),
                material: materials.add(StandardMaterial {
                    base_color: css::RED.into(),
                    ..default()
                }),
                ..default()
            },
            ManuallyTarget(Vec4::new(0.0, 0.0, 1.0, 0.3)),
        ))
        .id();

    let pole_target = commands
        .spawn(PbrBundle {
            transform: Transform::from_xyz(-1.0, 0.4, -0.2),
            mesh: meshes.add(Sphere::new(0.05).mesh().uv(7, 7)),
            material: materials.add(StandardMaterial {
                base_color: css::LIME.into(),
                ..default()
            }),
            ..default()
        })
        .id();

    // Add an IK constraint to the right hand, using the targets that were created earlier.
    commands.entity(foot).insert(IkConstraint {
        chain_length: 2,
        iterations: 20,
        target,
        pole_target: Some(pole_target),
        pole_angle: -std::f32::consts::FRAC_PI_2,
        enabled: true,
    });
}

fn manually_target(
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut target_query: Query<(&ManuallyTarget, &mut Transform)>,
    mut cursor: EventReader<CursorMoved>,
) {
    let (camera, transform) = camera_query.single();

    if let Some(event) = cursor.read().last() {
        let view = transform.compute_matrix();
        let viewport_rect = camera.logical_viewport_rect().unwrap();
        let viewport_size = viewport_rect.size();
        let adj_cursor_pos = event.position - Vec2::new(viewport_rect.min.x, viewport_rect.min.y);

        let projection = camera.clip_from_view();
        let far_ndc = projection.project_point3(Vec3::NEG_Z).z;
        let near_ndc = projection.project_point3(Vec3::Z).z;
        let cursor_ndc =
            ((adj_cursor_pos / viewport_size) * 2.0 - Vec2::ONE) * Vec2::new(1.0, -1.0);
        let ndc_to_world: Mat4 = view * projection.inverse();
        let near = ndc_to_world.project_point3(cursor_ndc.extend(near_ndc));
        let far = ndc_to_world.project_point3(cursor_ndc.extend(far_ndc));
        let ray_direction = far - near;

        for (&ManuallyTarget(plane), mut transform) in target_query.iter_mut() {
            let normal = plane.truncate();
            let d = plane.w;
            let denom = normal.dot(ray_direction);
            if denom.abs() > 0.0001 {
                let t = (normal * d - near).dot(normal) / denom;
                transform.translation = near + ray_direction * t;
            }
        }
    }
}
