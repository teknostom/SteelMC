//! Nearest attackable target goal for mob AI.

use crate::entity::ai::goal::{Goal, GoalContext, GoalFlag, GoalFlags};
use steel_registry::vanilla_entities;

/// Target type to search for
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    /// Target players
    Player,
    /// Target villagers
    Villager,
    /// Target iron golems
    IronGolem,
    /// Target turtles (specifically babies on land)
    Turtle,
    /// Target any mob
    AnyMob,
}

impl TargetType {
    /// Returns the entity type ID for this target type, if applicable
    fn entity_type_id(&self) -> Option<i32> {
        match self {
            TargetType::Player => Some(vanilla_entities::PLAYER.id),
            TargetType::Villager => Some(vanilla_entities::VILLAGER.id),
            TargetType::IronGolem => Some(vanilla_entities::IRON_GOLEM.id),
            TargetType::Turtle => Some(vanilla_entities::TURTLE.id),
            TargetType::AnyMob => None,
        }
    }
}

/// Nearest attackable target goal that finds the nearest valid target of a specific type.
///
/// This is a port of vanilla's `NearestAttackableTargetGoal`.
#[derive(Debug)]
pub struct NearestAttackableTargetGoal {
    /// Type of target to search for
    target_type: TargetType,
    /// Random interval for searching (reduces CPU usage)
    random_interval: u32,
    /// Whether the target must be seen
    must_see: bool,
    /// Whether the target must be reachable
    must_reach: bool,
    /// Number of ticks to remember unseen target
    unseen_memory_ticks: u32,
    /// Ticks until next search
    ticks_until_search: u32,
    /// Found target entity ID
    found_target: Option<i32>,
}

impl NearestAttackableTargetGoal {
    /// Creates a new nearest attackable target goal
    #[must_use]
    pub fn new(target_type: TargetType, must_see: bool) -> Self {
        Self {
            target_type,
            random_interval: 10,
            must_see,
            must_reach: false,
            unseen_memory_ticks: 60,
            ticks_until_search: 0,
            found_target: None,
        }
    }

    /// Creates a new goal with custom random interval
    #[must_use]
    pub fn with_interval(target_type: TargetType, random_interval: u32, must_see: bool) -> Self {
        Self {
            target_type,
            random_interval,
            must_see,
            must_reach: false,
            unseen_memory_ticks: 60,
            ticks_until_search: 0,
            found_target: None,
        }
    }

    /// Sets the must_reach flag
    #[must_use]
    pub fn must_reach(mut self, must_reach: bool) -> Self {
        self.must_reach = must_reach;
        self
    }

    /// Sets the unseen memory duration
    #[must_use]
    pub fn unseen_memory_ticks(mut self, ticks: u32) -> Self {
        self.unseen_memory_ticks = ticks;
        self
    }

    /// Finds the nearest target of the specified type
    fn find_target(&self, ctx: &GoalContext<'_>) -> Option<i32> {
        let follow_range = 35.0; // Default zombie follow range

        // Search based on target type
        if let Some(type_id) = self.target_type.entity_type_id() {
            // Find nearest entity of specific type
            if let Some((entity_id, _pos, _dist)) = ctx.entity_tracker.get_nearest_entity(
                ctx.position,
                follow_range,
                Some(ctx.entity_id),
                |eid| eid == type_id,
            ) {
                return Some(entity_id);
            }
        } else {
            // AnyMob: Find any entity (for now, just return first in range)
            let results = ctx
                .entity_tracker
                .get_entities_in_radius(ctx.position, follow_range, Some(ctx.entity_id));
            if let Some((entity_id, _pos, _dist)) = results.first() {
                return Some(*entity_id);
            }
        }

        None
    }

    /// Checks if a target is valid
    fn is_valid_target(&self, ctx: &GoalContext<'_>) -> bool {
        let target = match &ctx.target {
            Some(t) => t,
            None => return false,
        };

        let follow_range_sq = 35.0 * 35.0;

        // Check distance
        if target.distance_squared > follow_range_sq {
            return false;
        }

        // Check visibility
        if self.must_see {
            // TODO: Actual line of sight check
            let unseen = ctx.ai_state.unseen_target_ticks;
            if unseen > self.unseen_memory_ticks {
                return false;
            }
        }

        true
    }
}

impl Goal for NearestAttackableTargetGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flag(GoalFlag::Target)
    }

    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        // Check random interval
        if self.random_interval > 0 {
            self.ticks_until_search = self.ticks_until_search.saturating_sub(1);
            if self.ticks_until_search > 0 {
                return false;
            }

            use rand::Rng;
            self.ticks_until_search = ctx.random.random_range(0u32..self.random_interval);
        }

        // Try to find a target
        self.found_target = self.find_target(ctx);
        self.found_target.is_some()
    }

    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        self.is_valid_target(ctx)
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        // Set the target
        if let Some(target_id) = self.found_target {
            ctx.ai_state.target_entity_id = Some(target_id);
        }
        ctx.ai_state.unseen_target_ticks = 0;
    }

    fn stop(&mut self, ctx: &mut GoalContext<'_>) {
        ctx.ai_state.target_entity_id = None;
        self.found_target = None;
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        // Update unseen ticks
        // TODO: Actual line of sight check
        let can_see = ctx.target.is_some();
        if can_see {
            ctx.ai_state.unseen_target_ticks = 0;
        } else {
            ctx.ai_state.unseen_target_ticks += 1;
        }
    }
}
