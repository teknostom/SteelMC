//! Hurt by target goal for mob AI.

use crate::entity::ai::goal::{Goal, GoalContext, GoalFlag, GoalFlags};

/// Hurt by target goal that targets entities that hurt this mob.
///
/// This is a port of vanilla's `HurtByTargetGoal`.
#[derive(Debug)]
pub struct HurtByTargetGoal {
    /// Whether to alert others of the same type
    alert_same_type: bool,
    /// Entity types to exclude from targeting (even if they hurt us)
    ignore_entity_types: Vec<&'static str>,
    /// Entity types to exclude from alerting
    exclude_from_alert: Vec<&'static str>,
    /// Last recorded hurt time
    last_hurt_timestamp: u64,
    /// The attacker entity ID
    attacker_id: Option<i32>,
    /// Ticks to remember unseen attacker
    unseen_memory_ticks: u32,
}

impl Default for HurtByTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl HurtByTargetGoal {
    /// Creates a new hurt by target goal
    #[must_use]
    pub fn new() -> Self {
        Self {
            alert_same_type: false,
            ignore_entity_types: Vec::new(),
            exclude_from_alert: Vec::new(),
            last_hurt_timestamp: 0,
            attacker_id: None,
            unseen_memory_ticks: 300, // 15 seconds
        }
    }

    /// Sets whether to alert others of the same type
    #[must_use]
    pub fn alert_others(mut self) -> Self {
        self.alert_same_type = true;
        self
    }

    /// Adds entity types to exclude from being alerted
    #[must_use]
    pub fn exclude_from_alert(mut self, types: Vec<&'static str>) -> Self {
        self.exclude_from_alert = types;
        self
    }

    /// Adds entity types to ignore (won't target even if they hurt us)
    #[must_use]
    pub fn ignore_types(mut self, types: Vec<&'static str>) -> Self {
        self.ignore_entity_types = types;
        self
    }

    /// Alerts nearby entities of the same type about the attacker
    fn alert_others_fn(&self, ctx: &mut GoalContext<'_>) {
        if !self.alert_same_type {
            return;
        }

        // TODO: Implement when we have entity queries
        // 1. Get all entities within range (FOLLOW_RANGE + 10 blocks vertical)
        // 2. Filter to same mob type
        // 3. Filter out entities that already have a target
        // 4. Filter out excluded types
        // 5. Set attacker as their target

        let _attacker_id = match self.attacker_id {
            Some(id) => id,
            None => return,
        };

        // Placeholder - in reality we would:
        // for nearby_mob in world.get_nearby_entities(ctx.position, follow_range) {
        //     if nearby_mob.type == self.mob_type && nearby_mob.target.is_none() {
        //         if !self.exclude_from_alert.contains(&nearby_mob.type) {
        //             nearby_mob.set_target(attacker_id);
        //         }
        //     }
        // }

        // For now, we can't do this without world access
        let _ = ctx;
    }
}

impl Goal for HurtByTargetGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flag(GoalFlag::Target)
    }

    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        // Check if we were recently hurt
        let last_hurt = ctx.ai_state.last_hurt_time;
        if last_hurt == 0 || last_hurt == self.last_hurt_timestamp {
            return false;
        }

        // Check if we have an attacker
        let attacker = match ctx.ai_state.last_hurt_by {
            Some(id) => id,
            None => return false,
        };

        // Check if attacker is valid target
        // TODO: Check if attacker is ignored type
        // TODO: Check targeting conditions

        self.attacker_id = Some(attacker);
        self.last_hurt_timestamp = last_hurt;

        true
    }

    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        // Check if target still valid
        let target = match &ctx.target {
            Some(t) => t,
            None => return false,
        };

        // Check follow range
        let follow_range_sq = 35.0 * 35.0;
        if target.distance_squared > follow_range_sq {
            return false;
        }

        // Check unseen memory
        if ctx.ai_state.unseen_target_ticks > self.unseen_memory_ticks {
            return false;
        }

        true
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        // Set the target to our attacker
        ctx.ai_state.target_entity_id = self.attacker_id;
        ctx.ai_state.unseen_target_ticks = 0;

        // Alert nearby entities
        self.alert_others_fn(ctx);
    }

    fn stop(&mut self, ctx: &mut GoalContext<'_>) {
        ctx.ai_state.target_entity_id = None;
        self.attacker_id = None;
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        // Update unseen ticks
        let can_see = ctx.target.is_some();
        if can_see {
            ctx.ai_state.unseen_target_ticks = 0;
        } else {
            ctx.ai_state.unseen_target_ticks += 1;
        }
    }
}
