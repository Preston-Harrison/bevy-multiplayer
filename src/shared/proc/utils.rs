use bevy::{ecs::system::RunSystemOnce, prelude::*};
use bevy_rapier3d::prelude::*;

pub struct ProcUtilsPlugin;

impl Plugin for ProcUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Events<SnapToFloor>>();
        app.add_systems(FixedUpdate, run_stuff);
    }
}

fn run_stuff(world: &mut World) {
    apply_deferred(world);
    world.run_system_once(snap_to_floor);
}

#[derive(Event)]
pub struct SnapToFloor {
    pub entity_to_move: Entity,
    pub set_visible: bool,
    // pub entity_base: Entity,
}

impl SnapToFloor {
    pub fn new(entity_to_move: Entity) -> Self {
        Self {
            entity_to_move,
            set_visible: false,
        }
    }

    pub fn set_visible(mut self) -> Self {
        self.set_visible = true;
        self
    }
}

fn snap_to_floor(
    context: Res<RapierContext>,
    mut events: ResMut<Events<SnapToFloor>>,
    mut transforms: Query<(&GlobalTransform, &mut Transform, &mut Visibility)>,
) {
    let mut to_add: Vec<SnapToFloor> = vec![];
    for event in events.drain() {
        let Ok((global_t, mut t, mut visibility)) = transforms.get_mut(event.entity_to_move) else {
            to_add.push(event);
            continue;
        };

        let global_pos = global_t.translation();
        let start = Vec3::new(global_pos.x, 100.0, global_pos.z);
        let intersect = context
            .cast_ray_and_get_normal(
                start,
                -Vec3::Y,
                150.0,
                false,
                QueryFilter::default().exclude_collider(event.entity_to_move),
            )
            .map(|v| v.1);
        let Some(intersect) = intersect else {
            warn!("no intersect for snap to floor");
            to_add.push(event);
            continue;
        };

        if event.set_visible {
            *visibility = Visibility::Visible;
        }

        let diff = -global_pos + intersect.point;
        t.translation += diff;
    }

    events.send_batch(to_add);
}
