use bevy_hanabi::prelude::*;
use std::time::Duration;

use bevy::{
    color::palettes::css::{WHITE, YELLOW},
    ecs::component::{ComponentHooks, StorageType},
    pbr::NotShadowCaster,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

pub struct TracerPlugin;

impl Plugin for TracerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HanabiPlugin>() {
            app.add_plugins(HanabiPlugin);
        }
        app.add_plugins(MaterialPlugin::<TracerShader>::default());
        app.add_systems(Startup, setup_muzzle_flash_particle_system);
        app.add_systems(Update, despawn_tracers);
    }
}

#[derive(Resource, Deref)]
struct MuzzleFlashEffect(Handle<EffectAsset>);

pub struct Tracer {
    /// End is in global world space.
    pub end: Vec3,
}

impl Component for Tracer {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_add(|mut world, entity, _component_id| {
            let despawn_after = DespawnAfter {
                spawned_at: world.resource::<Time>().elapsed(),
                lifetime: Duration::from_millis(50),
            };

            let tracer = world.get::<Self>(entity).unwrap();
            let tracer_start = world.get::<Transform>(entity).unwrap().translation;
            let asset_server = world.resource::<AssetServer>();
            let muzzle_flash = world.resource::<MuzzleFlashEffect>();
            let muzzle_flash_handle = muzzle_flash.0.clone();

            // Calculate the midpoint and rotation for the tracer
            let direction = tracer.end - tracer_start;
            let distance = direction.length();
            let cylinder = Cylinder::new(0.02, distance).mesh().build();
            let tracer_mesh = asset_server.add(cylinder);
            let tracer_material = asset_server.add(TracerShader {
                tracer_start: WHITE.into(),
                tracer_end: YELLOW.into(),
            });

            // Calculate the rotation to align the tracer with the direction vector
            let rotation = Quat::from_rotation_arc(Vec3::Y, direction.normalize());
            let mut transform = Transform::from_rotation(rotation);
            transform.translation += direction / 2.0;

            let particle_rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction.normalize());

            world
                .commands()
                .entity(entity)
                .insert(despawn_after)
                .with_children(|parent| {
                    parent.spawn((
                        MaterialMeshBundle {
                            material: tracer_material,
                            mesh: tracer_mesh,
                            transform,
                            ..default()
                        },
                        NotShadowCaster,
                    ));
                    parent.spawn((PointLightBundle {
                        point_light: PointLight {
                            color: YELLOW.into(),
                            shadows_enabled: true,
                            intensity: 40_000.0,
                            ..default()
                        },
                        ..default()
                    },));
                    parent.spawn((ParticleEffectBundle {
                        effect: ParticleEffect::new(muzzle_flash_handle),
                        transform: Transform::from_rotation(particle_rotation),
                        ..default()
                    },));
                });
        });
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct TracerShader {
    #[uniform(0)]
    tracer_start: LinearRgba,
    #[uniform(1)]
    tracer_end: LinearRgba,
}

impl Material for TracerShader {
    fn fragment_shader() -> ShaderRef {
        "shaders/tracer.wgsl".into()
    }
}

#[derive(Component, Clone)]
struct DespawnAfter {
    spawned_at: Duration,
    lifetime: Duration,
}

fn despawn_tracers(
    mut commands: Commands,
    tracers: Query<(Entity, &DespawnAfter)>,
    time: Res<Time>,
) {
    for (entity, tracer) in tracers.iter() {
        if tracer.spawned_at + tracer.lifetime < time.elapsed() {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn setup_muzzle_flash_particle_system(
    mut effects: ResMut<Assets<EffectAsset>>,
    mut commands: Commands,
) {
    let writer = ExprWriter::new();

    // Position the particle laterally within a small radius.
    let init_xz_pos = SetPositionCircleModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        axis: writer.lit(Vec3::Z).expr(),
        radius: writer.lit(0.16).expr(),
        dimension: ShapeDimension::Volume,
    };

    // Set up the age and lifetime.
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(3.0).expr());

    // Vary the size a bit.
    let init_size = SetAttributeModifier::new(
        Attribute::F32_0,
        (writer.rand(ScalarType::Float) * writer.lit(0.05) + writer.lit(0.07)).expr(),
    );

    // Make the particles move backwards at a constant speed.
    let init_velocity = SetAttributeModifier::new(
        Attribute::VELOCITY,
        (writer.rand(ScalarType::Float) * writer.lit(Vec3::new(0.0, 0.0, -2.0))).expr(),
    );

    // Make the particles shrink over time.
    let update_size = SetAttributeModifier::new(
        Attribute::SIZE,
        writer
            .attr(Attribute::F32_0)
            .mul(
                writer
                    .lit(1.0)
                    .sub((writer.attr(Attribute::AGE)).mul(writer.lit(0.75)))
                    .max(writer.lit(0.0)),
            )
            .expr(),
    );

    let module = writer.finish();

    // Add the effect.
    let handle = effects.add(
        EffectAsset::new(256, Spawner::burst(16.0.into(), 0.45.into()), module)
            .with_simulation_space(SimulationSpace::Local)
            .with_name("cartoon explosion")
            .init(init_xz_pos)
            .init(init_age)
            .init(init_lifetime)
            .init(init_size)
            .init(init_velocity)
            .update(update_size),
    );
    commands.insert_resource(MuzzleFlashEffect(handle));
}
