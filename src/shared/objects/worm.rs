use std::time::Duration;

use bevy::{
    color::palettes::css::BROWN,
    ecs::component::{ComponentHooks, StorageType},
    prelude::*,
};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};

use crate::{
    message::{client::MessageReaderOnClient, server::UnreliableMessageFromServer},
    shared::{physics::VelocityCalculator, tick::Tick, GameLogic, IsServer, NetworkObject},
};

use super::{grounded::Grounded, health::Health, LastSyncTracker};

pub struct WormPlugin {
    pub is_server: bool,
}

impl Plugin for WormPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                setup.in_set(GameLogic::Spawn),
                tick_kinematics.in_set(GameLogic::PreKinematics),
            ),
        );
        if self.is_server {
            app.add_systems(FixedUpdate, send_transforms.in_set(GameLogic::Sync));
        } else {
            app.add_systems(FixedUpdate, recv_transforms.in_set(GameLogic::Sync));
        }
    }
}

pub struct Worm {
    kinematics: WormKinematics,
    spawn_tick: Tick,
}

impl VelocityCalculator for Worm {
    fn get_velocity(&self) -> Vec3 {
        self.kinematics.get_velocity()
    }
}

impl Component for Worm {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_add(|mut world, entity, _component_id| {
            let is_server = world.get_resource::<IsServer>().is_some();
            let asset_server = world.resource::<AssetServer>();
            let mesh = asset_server.add(Sphere::new(0.5).mesh().build());
            let material = asset_server.add(StandardMaterial {
                base_color: BROWN.into(),
                ..default()
            });
            if is_server {
                world.commands().entity(entity).insert((
                    mesh,
                    material,
                    RigidBody::KinematicPositionBased,
                    KinematicCharacterController::default(),
                    Collider::ball(0.5),
                    Health::new(50.0),
                    Grounded::default(),
                ));
            } else {
                let spawn_tick = world.get::<Worm>(entity).unwrap().spawn_tick.clone();
                world.commands().entity(entity).insert((
                    mesh,
                    material,
                    RigidBody::KinematicPositionBased,
                    Collider::ball(0.5),
                    Health::new(50.0),
                    LastSyncTracker::<Transform>::new(spawn_tick),
                ));
            }
        });
    }
}

impl Default for Worm {
    fn default() -> Self {
        Worm {
            kinematics: WormKinematics {
                time_in_air: AirTime::Grounded,
            },
            spawn_tick: Tick::new(0),
        }
    }
}

#[derive(Default)]
struct DidRun(bool);

fn setup(mut did_run: Local<DidRun>, mut commands: Commands) {
    if did_run.0 {
        return;
    }
    did_run.0 = true;

    commands.spawn((
        Worm::default(),
        SpatialBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 10.0, 0.0))),
        NetworkObject::new_static(1),
    ));
}

#[derive(Debug, Clone)]
pub enum AirTime {
    Grounded,
    Airborne(Duration),
}

#[derive(Debug, Clone)]
pub struct WormKinematics {
    time_in_air: AirTime,
}

impl Default for WormKinematics {
    fn default() -> Self {
        Self {
            time_in_air: AirTime::Grounded,
        }
    }
}

impl WormKinematics {
    pub fn tick(&mut self, delta: Duration) {
        self.time_in_air = match self.time_in_air {
            AirTime::Grounded => AirTime::Airborne(delta),
            AirTime::Airborne(time) => AirTime::Airborne(time + delta),
        };
    }

    pub fn update(&mut self, is_grounded: bool) {
        if is_grounded {
            self.time_in_air = AirTime::Grounded;
        }
    }

    pub fn get_velocity(&self) -> Vec3 {
        let gravity = match self.time_in_air {
            AirTime::Airborne(duration) => Vec3::Y * -10.0 * duration.as_secs_f32(),
            AirTime::Grounded => Vec3::ZERO,
        };
        gravity
    }
}

fn tick_kinematics(mut worms: Query<(&mut Worm, &Grounded)>, time: Res<Time>) {
    for (mut worm, grounded) in worms.iter_mut() {
        worm.kinematics.update(grounded.is_grounded());
        worm.kinematics.tick(time.delta());
    }
}

fn send_transforms(
    mut server: ResMut<RenetServer>,
    worms: Query<(&Transform, &NetworkObject), With<Worm>>,
    tick: Res<Tick>,
) {
    for (worm_t, net_obj) in worms.iter() {
        let message =
            UnreliableMessageFromServer::TransformSync(net_obj.clone(), *worm_t, tick.clone());
        let bytes = bincode::serialize(&message).unwrap();
        server.broadcast_message(DefaultChannel::Unreliable, bytes);
    }
}

fn recv_transforms(
    reader: Res<MessageReaderOnClient>,
    mut worms: Query<
        (
            &mut Transform,
            &NetworkObject,
            &mut LastSyncTracker<Transform>,
        ),
        With<Worm>,
    >,
) {
    for msg in reader.unreliable_messages() {
        let UnreliableMessageFromServer::TransformSync(net_obj, new_t, tick) = msg else {
            continue;
        };
        for (mut t, obj, mut sync) in worms.iter_mut() {
            if obj == net_obj && sync.should_update(*tick) {
                *t = *new_t;
            }
        }
    }
}
