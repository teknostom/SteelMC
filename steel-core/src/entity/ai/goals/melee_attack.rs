//! Melee attack goal for mobs that attack with melee weapons.

use crate::entity::ai::goal::{adjusted_tick_delay, Goal, GoalContext, GoalFlag, GoalFlags};
use crate::entity::ai::navigation::GroundNavigation;

/// Attack interval in ticks (20 ticks = 1 second)
const ATTACK_INTERVAL: u32 = 20;

/// Cooldown between canUse checks in ticks
const CAN_USE_COOLDOWN: u64 = 20;

/// Melee attack goal that makes a mob attack its target in close range.
///
/// This is a port of vanilla's `MeleeAttackGoal`.
#[derive(Debug)]
pub struct MeleeAttackGoal {
    /// Speed modifier when moving toward target
    speed_modifier: f64,
    /// Whether to follow target even if not seen
    follow_even_if_not_seen: bool,
    /// Ticks until next attack
    ticks_until_next_attack: u32,
    /// Last time canUse was checked
    last_can_use_check: u64,
    /// Navigation for pathfinding
    navigation: GroundNavigation,
}

impl MeleeAttackGoal {
    /// Creates a new melee attack goal
    #[must_use]
    pub fn new(speed_modifier: f64, follow_even_if_not_seen: bool) -> Self {
        Self {
            speed_modifier,
            follow_even_if_not_seen,
            ticks_until_next_attack: 0,
            last_can_use_check: 0,
            navigation: GroundNavigation::new(),
        }
    }

    /// Creates a new melee attack goal with default speed
    #[must_use]
    pub fn default_speed(follow_even_if_not_seen: bool) -> Self {
        Self::new(1.0, follow_even_if_not_seen)
    }

    /// Checks if the mob is within melee attack range of the target
    fn is_within_melee_range(&self, ctx: &GoalContext<'_>) -> bool {
        if let Some(target) = &ctx.target {
            // Melee range is approximately 2.0 blocks squared (1.414 blocks)
            // Vanilla uses entity width * 2 + target width, but we simplify
            let melee_range_sq = 4.0; // 2.0^2
            target.distance_squared < melee_range_sq
        } else {
            false
        }
    }

    /// Resets the attack cooldown
    fn reset_attack_cooldown(&mut self) {
        self.ticks_until_next_attack = adjusted_tick_delay(ATTACK_INTERVAL, true);
    }

    /// Gets the attack interval
    fn get_attack_interval(&self) -> u32 {
        adjusted_tick_delay(ATTACK_INTERVAL, true)
    }
}

impl Goal for MeleeAttackGoal {
    fn flags(&self) -> GoalFlags {
        GoalFlags::from_flags(&[GoalFlag::Move, GoalFlag::Look])
    }

    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        // Check cooldown
        if ctx.tick - self.last_can_use_check < CAN_USE_COOLDOWN {
            return false;
        }
        self.last_can_use_check = ctx.tick;

        // Need a target
        let target = match &ctx.target {
            Some(t) => t,
            None => return false,
        };

        // Check if we can path to target or are in melee range
        // For now, we just check if target exists and is within follow range
        // TODO: Actually create path and check reachability
        let follow_range_sq = 35.0 * 35.0; // Default zombie follow range
        if target.distance_squared > follow_range_sq {
            return false;
        }

        // In range or can path
        true
    }

    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        let target = match &ctx.target {
            Some(t) => t,
            None => return false,
        };

        // Check if we should continue following
        if !self.follow_even_if_not_seen {
            // Would check if navigation is done
            // For now, just check distance
            let follow_range_sq = 35.0 * 35.0;
            if target.distance_squared > follow_range_sq {
                return false;
            }
        }

        true
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn start(&mut self, ctx: &mut GoalContext<'_>) {
        ctx.ai_state.is_aggressive = true;
        self.ticks_until_next_attack = 0;
    }

    fn stop(&mut self, ctx: &mut GoalContext<'_>) {
        ctx.ai_state.is_aggressive = false;
        ctx.ai_state.look_target = None;
        self.navigation.stop();
    }

    fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        let target = match &ctx.target {
            Some(t) => t.clone(),
            None => return,
        };

        // Look at target
        ctx.ai_state.look_target = Some(target.position);

        // Check if we need to recalculate path
        if self.navigation.should_recalculate(target.position) {
            self.navigation.move_to(target.position, self.speed_modifier, ctx);
        }

        // Tick navigation to follow path
        if let Some((direction, speed)) = self.navigation.tick(ctx) {
            // Calculate next waypoint position for movement
            let next_pos = steel_utils::math::Vector3::new(
                ctx.position.x + direction.x,
                ctx.position.y + direction.y,
                ctx.position.z + direction.z,
            );

            // Store path in AI state for ZombieBehaviour to apply movement
            ctx.ai_state.speed_modifier = speed;

            // Create a simple path with just the next waypoint
            use crate::entity::ai::pathfinding::{Path, PathNodeType, PathWaypoint};
            let waypoint = PathWaypoint {
                position: next_pos,
                node_type: PathNodeType::Open,
            };
            let path = Path::new(vec![waypoint], target.position);
            ctx.ai_state.current_path = Some(path);
        }

        // Update attack timer
        self.ticks_until_next_attack = self.ticks_until_next_attack.saturating_sub(1);

        // Check if we can attack
        if self.ticks_until_next_attack == 0 && self.is_within_melee_range(ctx) {
            // TODO: Check line of sight
            // Perform attack
            self.reset_attack_cooldown();
            ctx.ai_state.ticks_since_attack = 0;
            // TODO: Actually deal damage via world/entity system
        }
    }
}
