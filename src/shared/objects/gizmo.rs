use std::time::Duration;

use bevy::prelude::*;

pub struct GizmoPlugin;

impl Plugin for GizmoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw_raycast);
    }
}

#[derive(Component)]
pub struct RaycastVisual {
    despawn_timer: Timer,
    from: Vec3,
    to: Vec3,
    color: Color,
}

fn draw_raycast(
    mut gizmos: Gizmos,
    mut query: Query<(Entity, &mut RaycastVisual)>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (entity, mut ray) in query.iter_mut() {
        ray.despawn_timer.tick(time.delta());
        if ray.despawn_timer.just_finished() {
            commands.entity(entity).despawn();
        } else {
            gizmos.arrow(ray.from, ray.to, ray.color);
        }
    }
}

pub fn spawn_raycast_visual(commands: &mut Commands, ray_pos: Vec3, ray_dir: Vec3, length: f32, color: impl Into<Color>, millis: u64) {
    let to = ray_pos + (ray_dir * length);
    let cast = RaycastVisual {
        despawn_timer: Timer::new(Duration::from_millis(millis), TimerMode::Once),
        from: ray_pos,
        to,
        color: color.into(),
    };
    commands.spawn(cast);
}
