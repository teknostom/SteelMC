//! Base target goal for mob AI.

use crate::entity::ai::goal::{Goal, GoalContext, GoalFlag, GoalFlags};

/// Default number of ticks to remember an unseen target
pub const DEFAULT_UNSEEN_MEMORY_TICKS: u32 = 60;

/// Base target goal that provides common target tracking functionality.
///
/// This is a port of vanilla's `TargetGoal`.
#[derive(Debug)]
pub struct TargetGoal {
    /// Whether the target must be seen to continue tracking
    must_see: bool,
    /// Whether the target must be reachable (pathfindable)
    must_reach: bool,
    /// Number of ticks to remember an unseen target
    unseen_memory_ticks: u32,
}

impl TargetGoal {
    /// Creates a new target goal
    #[must_use]
    pub fn new(must_see: bool, must_reach: bool) -> Self {
        Self {
            must_see,
            must_reach,
            unseen_memory_ticks: DEFAULT_UNSEEN_MEMORY_TICKS,
        }
    }

    /// Sets the unseen memory duration
    #[must_use]
    pub fn with_unseen_memory_ticks(mut self, ticks: u32) -> Self {
        self.unseen_memory_ticks = ticks;
        self
    }

    /// Checks if the current target is still valid
    pub fn can_continue_to_use_base(&self, ctx: &GoalContext<'_>) -> bool {
        // Need a target
        let target = match &ctx.target {
            Some(t) => t,
            None => return false,
        };

        // Check follow range
        let follow_range = 35.0; // Default zombie follow range
        let follow_range_sq = follow_range * follow_range;
        if target.distance_squared > follow_range_sq {
            return false;
        }

        // Check unseen memory
        if self.must_see {
            // TODO: Check actual line of sight
            let unseen_ticks = ctx.ai_state.unseen_target_ticks;
            if unseen_ticks > self.unseen_memory_ticks {
                return false;
            }
        }

        // Check reachability
        if self.must_reach {
            // TODO: Check if path exists
            // For now, assume reachable if within range
        }

        true
    }

    /// Validates a potential target
    pub fn can_attack(&self, ctx: &GoalContext<'_>, target_distance_sq: f64) -> bool {
        // Check follow range
        let follow_range = 35.0; // Default zombie follow range
        let follow_range_sq = follow_range * follow_range;
        if target_distance_sq > follow_range_sq {
            return false;
        }

        // TODO: Check team affiliation
        // TODO: Check if target is attackable
        // TODO: Check home boundary

        true
    }

    /// Whether this goal requires sight to the target
    #[must_use]
    pub fn must_see(&self) -> bool {
        self.must_see
    }

    /// Whether this goal requires the target to be reachable
    #[must_use]
    pub fn must_reach(&self) -> bool {
        self.must_reach
    }
}

impl Goal for TargetGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flag(GoalFlag::Target)
    }

    fn can_use(&mut self, _ctx: &mut GoalContext<'_>) -> bool {
        // Base target goal doesn't do anything by itself
        // Subclasses override this
        false
    }

    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        self.can_continue_to_use_base(ctx)
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        // Reset unseen ticks
        ctx.ai_state.unseen_target_ticks = 0;
    }

    fn stop(&mut self, ctx: &mut GoalContext<'_>) {
        // Clear target
        ctx.ai_state.target_entity_id = None;
        ctx.ai_state.unseen_target_ticks = 0;
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        // Update unseen ticks
        // TODO: Actually check line of sight
        let can_see = ctx.target.is_some(); // Simplified - assume can see if target exists
        if can_see {
            ctx.ai_state.unseen_target_ticks = 0;
        } else {
            ctx.ai_state.unseen_target_ticks += 1;
        }
    }
}
