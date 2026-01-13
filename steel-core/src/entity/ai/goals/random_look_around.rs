//! Random look around goal for mobs that look around randomly.

use crate::entity::ai::goal::{Goal, GoalContext, GoalFlag, GoalFlags};

/// Random look around goal that makes a mob look in random directions.
///
/// This is a port of vanilla's `RandomLookAroundGoal`.
#[derive(Debug)]
pub struct RandomLookAroundGoal {
    /// Target delta yaw
    delta_yaw: f32,
    /// Target delta pitch
    delta_pitch: f32,
    /// Ticks remaining to look in current direction
    look_time: u32,
}

impl Default for RandomLookAroundGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomLookAroundGoal {
    /// Creates a new random look around goal
    #[must_use]
    pub fn new() -> Self {
        Self {
            delta_yaw: 0.0,
            delta_pitch: 0.0,
            look_time: 0,
        }
    }
}

impl Goal for RandomLookAroundGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flag(GoalFlag::Move) // Uses MOVE flag in vanilla (prevents movement while looking)
    }

    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        use rand::Rng;

        // 1 in 80 chance per tick (approximately every 4 seconds on average)
        ctx.random.random_range(0.0f32..1.0) < 0.0125
    }

    fn can_continue_to_use(&mut self, _ctx: &mut GoalContext<'_>) -> bool {
        self.look_time > 0
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        use rand::Rng;

        // Random rotation offset
        let pi2 = std::f32::consts::PI * 2.0;
        self.delta_yaw = (ctx.random.random_range(0.0f32..1.0) - 0.5) * pi2;
        self.delta_pitch = (ctx.random.random_range(0.0f32..1.0) - 0.5) * 0.5; // Limited pitch range

        // Random duration (20-60 ticks, or 1-3 seconds)
        self.look_time = 20 + ctx.random.random_range(0u32..40);
    }

    fn stop(&mut self, _ctx: &mut GoalContext<'_>) {
        self.look_time = 0;
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        self.look_time = self.look_time.saturating_sub(1);

        // Calculate look target position based on current rotation + delta
        let (current_yaw, current_pitch) = ctx.rotation;
        let target_yaw = current_yaw + self.delta_yaw;
        let target_pitch = (current_pitch + self.delta_pitch).clamp(-89.0, 89.0);

        // Convert to look target position (10 blocks ahead in that direction)
        let distance = 10.0;
        let yaw_rad = target_yaw.to_radians();
        let pitch_rad = target_pitch.to_radians();

        let dx = -yaw_rad.sin() as f64 * pitch_rad.cos() as f64 * distance;
        let dy = -pitch_rad.sin() as f64 * distance;
        let dz = yaw_rad.cos() as f64 * pitch_rad.cos() as f64 * distance;

        ctx.ai_state.look_target = Some(steel_utils::math::Vector3::new(
            ctx.position.x + dx,
            ctx.position.y + 1.62 + dy, // Eye height
            ctx.position.z + dz,
        ));
    }
}
