use bevy::prelude::*;
use bevy_renet::renet::{DefaultChannel, RenetServer};
use objects::{gun::GunPlugin, health::HealthPlugin, tracer::TracerPlugin};
use proc::TerrainPlugin;

use crate::message::{client::MessageReaderOnClient, server::ReliableMessageFromServer};

use self::{
    console::ConsolePlugin,
    objects::{
        gizmo::GizmoPlugin, grounded::GroundedPlugin, player::PlayerPlugin, worm::WormPlugin,
        NetworkObject,
    },
    physics::PhysicsPlugin,
};

pub mod console;
pub mod ik;
pub mod objects;
pub mod physics;
pub mod proc;
pub mod render;
pub mod scenes;
pub mod tick;

pub const SERVER_ADDR: &str = "127.0.0.1:5000";

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    MainMenu,
    InGame,
}

/// Order is:
/// - Start
/// - ReadInput
/// - TickAdjust
/// - Spawn
/// - Sync
/// - Game
/// - PreKinematics
/// - Kinematics
/// - End
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameLogic {
    Start,
    ReadInput,
    /// Server will send tick adjustments here, client will read tick adjustments
    /// here.
    TickAdjust,
    /// Server spawns and despawns here, client receives spawns here.
    Spawn,
    /// Server sends data here, client receives data here.
    Sync,
    Game,
    /// Do stuff like updating kinematic velocities here. Can also be done earlier,
    /// this is just convenient for running logic right before kinematics are applied.
    PreKinematics,
    /// Actual kinematics are applied here.
    Kinematics,
    End,
}

fn despawn(
    reader: Res<MessageReaderOnClient>,
    mut commands: Commands,
    query: Query<(Entity, &NetworkObject)>,
) {
    for msg in reader.reliable_messages() {
        let ReliableMessageFromServer::Despawn(network_obj) = msg else {
            continue;
        };
        for (e, obj) in query.iter() {
            if obj == network_obj {
                commands.entity(e).despawn_recursive();
                break;
            }
        }
    }
}

pub struct Game {
    pub is_server: bool,
}

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            PlayerPlugin {
                is_server: self.is_server,
            },
            PhysicsPlugin {
                debug: self.is_server,
            },
            GizmoPlugin,
            ConsolePlugin,
            GroundedPlugin,
            TerrainPlugin,
            GunPlugin,
            HealthPlugin {
                is_server: self.is_server,
            },
            TracerPlugin,
            WormPlugin {
                is_server: self.is_server,
            },
        ));
        if !self.is_server {
            app.add_systems(FixedUpdate, despawn.in_set(GameLogic::Spawn));
        } else {
            app.init_resource::<IsServer>();
        }
        app.configure_sets(
            FixedUpdate,
            ((
                GameLogic::Start,
                GameLogic::TickAdjust,
                GameLogic::ReadInput,
                GameLogic::Spawn,
                GameLogic::Sync,
                GameLogic::Game,
                GameLogic::PreKinematics,
                GameLogic::Kinematics,
                GameLogic::End,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),),
        );
    }
}

pub fn despawn_recursive_and_broadcast(
    server: &mut RenetServer,
    commands: &mut Commands,
    entity: Entity,
    net_obj: NetworkObject,
) {
    let message = ReliableMessageFromServer::Despawn(net_obj);
    let bytes = bincode::serialize(&message).unwrap();
    server.broadcast_message(DefaultChannel::ReliableUnordered, bytes);
    commands.entity(entity).despawn_recursive();
}

#[derive(Resource, Default)]
pub struct IsServer;
