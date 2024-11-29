use bevy::{
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

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
