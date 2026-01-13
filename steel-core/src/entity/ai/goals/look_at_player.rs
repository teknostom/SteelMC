//! Look at player goal for mobs that look at nearby players.

use crate::entity::ai::goal::{Goal, GoalContext, GoalFlag, GoalFlags};
use steel_registry::vanilla_entities;

/// Look at player goal that makes a mob look at a nearby player.
///
/// This is a port of vanilla's `LookAtPlayerGoal`.
#[derive(Debug)]
pub struct LookAtPlayerGoal {
    /// Maximum distance to look at player
    look_distance: f32,
    /// Probability of looking at player each tick (0.0-1.0)
    probability: f32,
    /// Current look target (player entity ID)
    look_at: Option<i32>,
    /// Ticks remaining to look at current target
    look_time: u32,
}

impl LookAtPlayerGoal {
    /// Creates a new look at player goal
    #[must_use]
    pub fn new(look_distance: f32) -> Self {
        Self {
            look_distance,
            probability: 0.02, // 2% chance per tick
            look_at: None,
            look_time: 0,
        }
    }

    /// Creates a new look at player goal with custom probability
    #[must_use]
    pub fn with_probability(look_distance: f32, probability: f32) -> Self {
        Self {
            look_distance,
            probability,
            look_at: None,
            look_time: 0,
        }
    }
}

impl Goal for LookAtPlayerGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flag(GoalFlag::Look)
    }

    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        use rand::Rng;

        // Random check
        if ctx.random.random_range(0.0f32..1.0) >= self.probability {
            return false;
        }

        // Search for nearby players using entity queries
        if let Some((player_id, _pos, _dist)) = ctx.entity_tracker.get_nearest_entity(
            ctx.position,
            f64::from(self.look_distance),
            Some(ctx.entity_id),
            |type_id| type_id == vanilla_entities::PLAYER.id,
        ) {
            self.look_at = Some(player_id);
            return true;
        }

        false
    }

    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        if self.look_time == 0 {
            return false;
        }

        // Check if the player we're looking at still exists and is in range
        if let Some(player_id) = self.look_at {
            if let Some(tracked) = ctx.entity_tracker.get_entity(player_id) {
                let player_pos = tracked.entity.position();
                let dx = ctx.position.x - player_pos.x;
                let dy = ctx.position.y - player_pos.y;
                let dz = ctx.position.z - player_pos.z;
                let distance_squared = dx * dx + dy * dy + dz * dz;
                let distance = distance_squared.sqrt();

                return distance <= f64::from(self.look_distance);
            }
        }

        false
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        use rand::Rng;

        // Random duration to look (40-80 ticks, or 2-4 seconds)
        self.look_time = 40 + ctx.random.random_range(0u32..40);
    }

    fn stop(&mut self, ctx: &mut GoalContext<'_>) {
        self.look_at = None;
        ctx.ai_state.look_target = None;
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        self.look_time = self.look_time.saturating_sub(1);

        // Update look target by getting player's current position from entity tracker
        if let Some(player_id) = self.look_at {
            if let Some(tracked) = ctx.entity_tracker.get_entity(player_id) {
                let player_pos = tracked.entity.position();
                // Look at target's eye position (head height)
                let eye_height = 1.62; // Approximate player eye height
                ctx.ai_state.look_target = Some(steel_utils::math::Vector3::new(
                    player_pos.x,
                    player_pos.y + eye_height,
                    player_pos.z,
                ));
            }
        }
    }
}
