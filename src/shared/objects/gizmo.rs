use std::time::Duration;

use bevy::{
    color::palettes::css::{GREEN, YELLOW},
    prelude::*,
};

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
    is_arrow: bool,
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
            if ray.is_arrow {
                gizmos.arrow(ray.from, ray.to, ray.color);
            } else {
                gizmos.line(ray.from, ray.to, ray.color);
            }
        }
    }
}

pub fn spawn_bullet_tracer(
    commands: &mut Commands,
    ray_pos: Vec3,
    ray_dir: Vec3,
    length: f32,
    hit: bool,
) {
    let to = ray_pos + (ray_dir * length);
    let cast = RaycastVisual {
        despawn_timer: Timer::new(Duration::from_millis(100), TimerMode::Once),
        from: ray_pos,
        is_arrow: false,
        to,
        color: if hit { GREEN.into() } else { YELLOW.into() },
    };
    commands.spawn(cast);
}
