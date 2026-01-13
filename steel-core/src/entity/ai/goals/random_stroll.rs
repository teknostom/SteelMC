//! Random stroll goal for mobs that wander around.

use crate::entity::ai::goal::{Goal, GoalContext, GoalFlag, GoalFlags};
use steel_utils::math::Vector3;

/// Water avoiding random stroll goal that makes a mob wander randomly while avoiding water.
///
/// This is a port of vanilla's `WaterAvoidingRandomStrollGoal`.
#[derive(Debug)]
pub struct WaterAvoidingRandomStrollGoal {
    /// Speed modifier when walking
    speed_modifier: f64,
    /// Probability of choosing a water-avoiding path (0.0-1.0)
    probability: f32,
    /// Current walk target
    walk_target: Option<Vector3<f64>>,
    /// Whether currently strolling
    is_strolling: bool,
}

impl WaterAvoidingRandomStrollGoal {
    /// Creates a new water avoiding random stroll goal
    #[must_use]
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            probability: 0.001, // Very low probability to start walking
            walk_target: None,
            is_strolling: false,
        }
    }

    /// Creates a new goal with custom probability
    #[must_use]
    pub fn with_probability(speed_modifier: f64, probability: f32) -> Self {
        Self {
            speed_modifier,
            probability,
            walk_target: None,
            is_strolling: false,
        }
    }

}

impl Default for WaterAvoidingRandomStrollGoal {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl Goal for WaterAvoidingRandomStrollGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flag(GoalFlag::Move)
    }

    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        use rand::Rng;

        // Already has a target (attacking or similar)
        if ctx.ai_state.target_entity_id.is_some() {
            return false;
        }

        // Random chance to start strolling
        if ctx.random.random_range(0.0f32..1.0) >= self.probability * 120.0 {
            // Probability is checked over multiple ticks, so scale up
            return false;
        }

        // Try to find a walk target
        // We need mutable access to random, so we can't directly call find_walk_target
        // Instead, inline the logic here
        let max_horizontal = 10.0;
        let max_vertical = 7.0;

        let dx = (ctx.random.random_range(0.0f64..1.0) * 2.0 - 1.0) * max_horizontal;
        let dy = (ctx.random.random_range(0.0f64..1.0) * 2.0 - 1.0) * max_vertical;
        let dz = (ctx.random.random_range(0.0f64..1.0) * 2.0 - 1.0) * max_horizontal;

        self.walk_target = Some(Vector3::new(
            ctx.position.x + dx,
            ctx.position.y + dy,
            ctx.position.z + dz,
        ));

        true
    }

    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        // Stop if we have a target now
        if ctx.ai_state.target_entity_id.is_some() {
            return false;
        }

        // Stop if we reached the destination
        if let Some(target) = self.walk_target {
            let dist_sq = ctx.distance_squared_to(target);
            // Within 1 block of target
            if dist_sq < 1.0 {
                return false;
            }
        }

        // Stop if path is done
        if ctx.ai_state.current_path.as_ref().is_some_and(|p| p.is_done()) {
            return false;
        }

        self.is_strolling
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        self.is_strolling = true;

        // Set up navigation path
        if let Some(target) = self.walk_target {
            ctx.ai_state.speed_modifier = self.speed_modifier;
            ctx.ai_state.look_target = Some(target);
            // TODO: Actually create path via pathfinder
            // ctx.ai_state.current_path = pathfinder.find_path(ctx.position, target);
        }
    }

    fn stop(&mut self, ctx: &mut GoalContext<'_>) {
        self.is_strolling = false;
        self.walk_target = None;
        ctx.ai_state.current_path = None;
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        // Follow path - check if we need to advance first
        let should_advance = ctx
            .ai_state
            .current_path
            .as_ref()
            .and_then(|path| path.next_position())
            .is_some_and(|next_pos| {
                let dx = next_pos.x - ctx.position.x;
                let dy = next_pos.y - ctx.position.y;
                let dz = next_pos.z - ctx.position.z;
                dx * dx + dy * dy + dz * dz < 0.25 // Within 0.5 blocks
            });

        if should_advance {
            if let Some(path) = &mut ctx.ai_state.current_path {
                path.advance();
            }
        }

        // Update look direction to face movement direction
        if let Some(target) = self.walk_target {
            ctx.ai_state.look_target = Some(target);
        }
    }
}
