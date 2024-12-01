use bevy::{
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::f64;

pub fn toggle_cursor_grab_with_esc(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        let mut primary_window = q_windows.single_mut();
        primary_window.cursor.visible = !primary_window.cursor.visible;
        primary_window.cursor.grab_mode = if primary_window.cursor.visible {
            CursorGrabMode::None
        } else {
            CursorGrabMode::Locked
        };
    }
}

pub mod freecam {
    use bevy::{
        input::mouse::MouseMotion,
        prelude::*,
        window::{CursorGrabMode, PrimaryWindow},
    };

    pub struct FreeCameraPlugin;

    impl Plugin for FreeCameraPlugin {
        fn build(&self, app: &mut App) {
            app.add_systems(Update, (free_camera_movement, mouse_look));
        }
    }

    #[derive(Component)]
    pub struct FreeCamera {
        pub speed: f32,
        pub movement_enabled: bool,
    }

    impl FreeCamera {
        pub fn new(speed: f32) -> Self {
            Self {
                speed,
                movement_enabled: true,
            }
        }
    }

    // Free camera system
    fn free_camera_movement(
        time: Res<Time>,
        keys: Res<ButtonInput<KeyCode>>,
        mut query: Query<(&mut Transform, &mut FreeCamera)>,
    ) {
        for (mut transform, camera) in query.iter_mut() {
            if !camera.movement_enabled {
                return;
            }
            let forward = transform.rotation.mul_vec3(Vec3::new(0.0, 0.0, -1.0));
            let right = transform.rotation.mul_vec3(Vec3::new(1.0, 0.0, 0.0));

            let mut wasd_velocity = Vec3::ZERO;
            let mut vertical = 0.0;

            if keys.pressed(KeyCode::KeyW) {
                wasd_velocity += forward;
            }
            if keys.pressed(KeyCode::KeyS) {
                wasd_velocity -= forward;
            }
            if keys.pressed(KeyCode::KeyA) {
                wasd_velocity -= right;
            }
            if keys.pressed(KeyCode::KeyD) {
                wasd_velocity += right;
            }
            if keys.pressed(KeyCode::Space) {
                vertical += 1.0;
            }
            if keys.pressed(KeyCode::ShiftLeft) {
                vertical -= 1.0;
            }

            wasd_velocity.y = 0.0;
            wasd_velocity = wasd_velocity.normalize_or_zero();
            wasd_velocity.y = vertical;
            transform.translation += wasd_velocity * time.delta_seconds() * camera.speed;
        }
    }

    /// Rotates the player based on mouse movement.
    fn mouse_look(
        mut mouse_motion: EventReader<MouseMotion>,
        mut camera: Query<&mut Transform, With<FreeCamera>>,
        q_windows: Query<&Window, With<PrimaryWindow>>,
    ) {
        let primary_window = q_windows.single();
        if primary_window.cursor.grab_mode != CursorGrabMode::Locked {
            return;
        }
        let Ok(mut camera) = camera.get_single_mut() else {
            return;
        };
        for motion in mouse_motion.read() {
            let yaw = -motion.delta.x * 0.003;
            let pitch = -motion.delta.y * 0.002;
            camera.rotate_y(yaw);
            camera.rotate_local_x(pitch);
        }
    }
}

pub mod transform {
    //! TODO: All chat gpt generated, need to check if accurate.
    use bevy::prelude::*;

    /// Gets the yaw (rotation about the Y-axis) from the transform's quaternion rotation.
    ///
    /// # Returns
    /// The yaw angle (in radians) representing the horizontal rotation.
    pub fn get_head_rotation_yaw(transform: &Transform) -> f32 {
        let (_, yaw, _) = transform.rotation.to_euler(EulerRot::YXZ);
        yaw
    }

    /// Gets the pitch (rotation about the X-axis) from the transform's quaternion rotation.
    ///
    /// # Returns
    /// The pitch angle (in radians) representing the vertical rotation.
    pub fn get_body_rotation_pitch(transform: &Transform) -> f32 {
        let (pitch, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        pitch
    }

    /// Sets the yaw (rotation about the Y-axis) in the transform's quaternion rotation.
    ///
    /// This function preserves the pitch and roll values while updating the yaw.
    pub fn set_head_rotation_yaw(transform: &mut Transform, yaw: f32) {
        let (pitch, _, roll) = transform.rotation.to_euler(EulerRot::YXZ);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, pitch, yaw, roll);
    }

    /// Sets the pitch (rotation about the X-axis) in the transform's quaternion rotation.
    ///
    /// This function preserves the yaw and roll values while updating the pitch.
    pub fn set_body_rotation_pitch(transform: &mut Transform, pitch: f32) {
        let (_, yaw, roll) = transform.rotation.to_euler(EulerRot::YXZ);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, pitch, yaw, roll);
    }
}

/// Generates a set of points using Poisson disk sampling in 2D space.
///
/// # Parameters
///
/// - `width`: The width of the domain.
/// - `height`: The height of the domain.
/// - `min_distance`: The minimum distance between points.
/// - `k`: The number of attempts for each active point (typically 30).
/// - `rng`: A mutable reference to a random number generator.
///
/// # Returns
///
/// A vector of sampled points represented as `[f64; 2]` arrays.
pub fn poisson_disk_sampling<R: Rng>(
    width: f64,
    height: f64,
    min_distance: f64,
    k: usize,
    rng: &mut R,
) -> Vec<[f64; 2]> {
    // Compute cell size
    let cell_size = min_distance / (2f64).sqrt();

    // Compute grid dimensions
    let grid_width = (width / cell_size).ceil() as usize;
    let grid_height = (height / cell_size).ceil() as usize;

    // Initialize grid with None
    let mut grid: Vec<Vec<Option<[f64; 2]>>> = vec![vec![None; grid_height]; grid_width];

    // Initialize sample list and active list
    let mut samples = Vec::new();
    let mut active_list = Vec::new();

    // Pick an initial point
    let initial_point = [rng.gen_range(0.0..width), rng.gen_range(0.0..height)];

    // Add initial point to samples and active_list
    samples.push(initial_point);
    active_list.push(initial_point);

    // Update grid
    let grid_x = (initial_point[0] / cell_size).floor() as usize;
    let grid_y = (initial_point[1] / cell_size).floor() as usize;
    grid[grid_x][grid_y] = Some(initial_point);

    // Main loop
    while !active_list.is_empty() {
        // Pick a random index from active_list
        let idx = rng.gen_range(0..active_list.len());
        let point = active_list[idx];
        let mut found = false;
        for _ in 0..k {
            // Generate random point in annulus
            let radius = rng.gen_range(min_distance..2.0 * min_distance);
            let angle = rng.gen_range(0.0..2.0 * f64::consts::PI);
            let new_x = point[0] + radius * angle.cos();
            let new_y = point[1] + radius * angle.sin();
            let new_point = [new_x, new_y];

            // Check if new_point is within the domain
            if new_x >= 0.0 && new_x < width && new_y >= 0.0 && new_y < height {
                // Determine grid cell
                let grid_x = (new_x / cell_size).floor() as isize;
                let grid_y = (new_y / cell_size).floor() as isize;

                // Check neighboring cells
                let mut ok = true;
                for i in (grid_x - 2).max(0)..=(grid_x + 2).min(grid_width as isize - 1) {
                    for j in (grid_y - 2).max(0)..=(grid_y + 2).min(grid_height as isize - 1) {
                        if let Some(other_point) = grid[i as usize][j as usize] {
                            let dx = other_point[0] - new_x;
                            let dy = other_point[1] - new_y;
                            if dx * dx + dy * dy < min_distance * min_distance {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        break;
                    }
                }
                if ok {
                    // Add new_point to samples and active_list
                    samples.push(new_point);
                    active_list.push(new_point);
                    grid[grid_x as usize][grid_y as usize] = Some(new_point);
                    found = true;
                    break;
                }
            }
        }
        if !found {
            // Remove point from active_list
            active_list.swap_remove(idx);
        }
    }

    samples
}

pub fn create_rng_from_seed(seed: [u8; 32]) -> impl Rng {
    ChaCha20Rng::from_seed(seed)
}
