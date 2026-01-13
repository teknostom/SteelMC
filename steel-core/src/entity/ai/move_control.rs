//! Movement control system for mob AI.
//!
//! This module provides the `MoveControl` struct which handles:
//! - Converting target positions to velocity
//! - Smooth rotation interpolation toward movement direction
//! - Speed management

use steel_utils::math::Vector3;

/// Movement control for mobs.
///
/// Handles converting a wanted position into actual velocity and rotation,
/// with smooth interpolation for natural-looking movement.
#[derive(Debug, Default)]
pub struct MoveControl {
    /// Target position to move toward
    wanted_position: Option<Vector3<f64>>,
    /// Speed modifier for movement (1.0 = base speed)
    speed_modifier: f64,
    /// Whether the mob is currently strafing
    is_strafing: bool,
}

impl MoveControl {
    /// Creates a new movement control instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            wanted_position: None,
            speed_modifier: 1.0,
            is_strafing: false,
        }
    }

    /// Sets the wanted position to move toward
    pub fn set_wanted_position(&mut self, pos: Vector3<f64>, speed: f64) {
        self.wanted_position = Some(pos);
        self.speed_modifier = speed;
        self.is_strafing = false;
    }

    /// Clears the wanted position (stops movement)
    pub fn stop(&mut self) {
        self.wanted_position = None;
    }

    /// Checks if movement control has a target
    #[must_use]
    pub fn has_wanted_position(&self) -> bool {
        self.wanted_position.is_some()
    }

    /// Gets the current speed modifier
    #[must_use]
    pub fn speed(&self) -> f64 {
        self.speed_modifier
    }

    /// Processes movement control for one tick.
    ///
    /// Returns `Some((velocity, new_yaw))` if movement should occur,
    /// or `None` if no movement is needed.
    ///
    /// # Arguments
    /// * `current_pos` - Current entity position
    /// * `current_yaw` - Current entity yaw in degrees
    /// * `base_speed` - Base movement speed (e.g., 0.23 for zombies)
    #[must_use]
    pub fn tick(
        &mut self,
        current_pos: Vector3<f64>,
        current_yaw: f32,
        base_speed: f64,
    ) -> Option<(Vector3<f64>, f32)> {
        let target = self.wanted_position?;

        // Calculate direction to target
        let dx = target.x - current_pos.x;
        let dz = target.z - current_pos.z;
        let horizontal_dist = (dx * dx + dz * dz).sqrt();

        // Check if we've arrived
        if horizontal_dist < 0.1 {
            self.wanted_position = None;
            return None;
        }

        // Calculate target yaw (direction to move)
        // Minecraft convention: yaw 0 = south (+Z), 90 = west (-X)
        let target_yaw = (-dx).atan2(dz).to_degrees() as f32;

        // Smooth rotation (max 30 degrees per tick)
        let new_yaw = rotate_toward(current_yaw, target_yaw, 30.0);

        // Calculate velocity
        let speed = base_speed * self.speed_modifier;
        let velocity = Vector3::new(dx / horizontal_dist * speed, 0.0, dz / horizontal_dist * speed);

        Some((velocity, new_yaw))
    }

    /// Processes movement for strafing (moving sideways while facing target).
    ///
    /// # Arguments
    /// * `forward` - Forward movement input (-1.0 to 1.0)
    /// * `strafe` - Strafe movement input (-1.0 to 1.0)
    /// * `current_yaw` - Current entity yaw in degrees
    /// * `base_speed` - Base movement speed
    #[must_use]
    pub fn tick_strafe(
        &self,
        forward: f64,
        strafe: f64,
        current_yaw: f32,
        base_speed: f64,
    ) -> Vector3<f64> {
        let speed = base_speed * self.speed_modifier;

        // Convert yaw to radians and get sin/cos
        let yaw_rad = current_yaw.to_radians();
        let sin_yaw = f64::from(yaw_rad.sin());
        let cos_yaw = f64::from(yaw_rad.cos());

        // Rotate input by yaw
        let vx = (strafe * cos_yaw - forward * sin_yaw) * speed;
        let vz = (forward * cos_yaw + strafe * sin_yaw) * speed;

        Vector3::new(vx, 0.0, vz)
    }
}

/// Rotates `current` toward `target` by at most `max_delta` degrees.
///
/// Handles wrapping around 360 degrees correctly.
fn rotate_toward(current: f32, target: f32, max_delta: f32) -> f32 {
    // Calculate shortest angle difference
    let mut diff = target - current;

    // Wrap to [-180, 180]
    while diff > 180.0 {
        diff -= 360.0;
    }
    while diff < -180.0 {
        diff += 360.0;
    }

    // Clamp the rotation
    let clamped_diff = diff.clamp(-max_delta, max_delta);

    // Apply rotation
    let mut result = current + clamped_diff;

    // Normalize to [0, 360)
    while result < 0.0 {
        result += 360.0;
    }
    while result >= 360.0 {
        result -= 360.0;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotate_toward_simple() {
        // Rotating from 0 to 30 should give 30
        assert!((rotate_toward(0.0, 30.0, 30.0) - 30.0).abs() < 0.001);

        // Rotating from 0 to 90 with max 30 should give 30
        assert!((rotate_toward(0.0, 90.0, 30.0) - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_rotate_toward_wrap() {
        // Rotating from 350 to 10 should go through 0, not backwards
        let result = rotate_toward(350.0, 10.0, 30.0);
        // Should be around 20 (350 + 30 = 380 -> 20)
        assert!((result - 20.0).abs() < 0.001);

        // Rotating from 10 to 350 should go backwards through 0
        let result = rotate_toward(10.0, 350.0, 30.0);
        // Should be around 340 (10 - 30 = -20 -> 340)
        assert!((result - 340.0).abs() < 0.001);
    }
}
