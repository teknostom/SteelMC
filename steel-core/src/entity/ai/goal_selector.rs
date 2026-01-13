//! Goal selector for managing and executing mob AI goals.
//!
//! The goal selector is responsible for:
//! - Managing a list of goals with priorities
//! - Determining which goals should be active based on flag conflicts
//! - Executing active goals each tick

use super::goal::{Goal, GoalContext, GoalFlag, GoalFlags};

/// A wrapped goal with priority and state tracking.
struct WrappedGoal {
    /// The goal instance
    goal: Box<dyn Goal>,
    /// Priority (lower = higher priority)
    priority: i32,
    /// Whether this goal is currently running
    is_running: bool,
}

impl WrappedGoal {
    fn new(priority: i32, goal: Box<dyn Goal>) -> Self {
        Self {
            goal,
            priority,
            is_running: false,
        }
    }

    fn flags(&self) -> GoalFlags {
        self.goal.flags()
    }

    fn can_be_replaced_by(&self, other: &Self) -> bool {
        // Can be replaced if:
        // 1. This goal is interruptable
        // 2. The other goal has higher priority (lower number)
        self.goal.is_interruptable() && other.priority < self.priority
    }
}

/// Manages a set of goals and determines which should be active.
///
/// Goals are added with priorities, and the selector ensures that:
/// - Higher priority goals (lower numbers) take precedence
/// - Goals with conflicting flags don't run simultaneously
/// - Goals are started/stopped cleanly
pub struct GoalSelector {
    /// All available goals, sorted by priority
    goals: Vec<WrappedGoal>,
    /// Flags that are currently disabled
    disabled_flags: GoalFlags,
    /// Current goal holding each flag
    locked_flags: [Option<usize>; GoalFlag::COUNT],
}

impl Default for GoalSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl GoalSelector {
    /// Creates a new empty goal selector
    #[must_use]
    pub fn new() -> Self {
        Self {
            goals: Vec::new(),
            disabled_flags: GoalFlags::empty(),
            locked_flags: [None; GoalFlag::COUNT],
        }
    }

    /// Adds a goal with the specified priority.
    ///
    /// Lower priority numbers are executed first. Goals with the same priority
    /// are executed in the order they were added.
    pub fn add_goal(&mut self, priority: i32, goal: Box<dyn Goal>) {
        let wrapped = WrappedGoal::new(priority, goal);

        // Insert in sorted order by priority
        let pos = self
            .goals
            .iter()
            .position(|g| g.priority > priority)
            .unwrap_or(self.goals.len());
        self.goals.insert(pos, wrapped);
    }

    /// Removes all goals matching the predicate
    pub fn remove_goals_if<F>(&mut self, predicate: F)
    where
        F: Fn(&dyn Goal) -> bool,
    {
        self.goals.retain(|wrapped| !predicate(&*wrapped.goal));
    }

    /// Disables a control flag, preventing any goals using it from running.
    pub fn disable_flag(&mut self, flag: GoalFlag) {
        self.disabled_flags.insert(flag);
    }

    /// Enables a control flag, allowing goals using it to run again.
    pub fn enable_flag(&mut self, flag: GoalFlag) {
        self.disabled_flags.remove(flag);
    }

    /// Sets whether a flag is enabled
    pub fn set_flag(&mut self, flag: GoalFlag, enabled: bool) {
        if enabled {
            self.enable_flag(flag);
        } else {
            self.disable_flag(flag);
        }
    }

    /// Ticks the goal selector, updating which goals are running.
    ///
    /// This implements vanilla's algorithm:
    /// 1. Cleanup: Stop goals that can't continue or have disabled flags
    /// 2. Update: Start new goals that can run and don't conflict
    /// 3. Tick: Execute all running goals
    pub fn tick(&mut self, ctx: &mut GoalContext<'_>) {
        // Phase 1: Cleanup - stop goals that shouldn't continue
        for (i, wrapped) in self.goals.iter_mut().enumerate() {
            if wrapped.is_running {
                // Check if goal should stop
                let should_stop =
                    // Goal has disabled flags
                    wrapped.flags().intersects(&self.disabled_flags) ||
                    // Goal can't continue
                    !wrapped.goal.can_continue_to_use(ctx);

                if should_stop {
                    wrapped.goal.stop(ctx);
                    wrapped.is_running = false;

                    // Release locked flags
                    for flag in wrapped.flags().iter() {
                        if self.locked_flags[flag as usize] == Some(i) {
                            self.locked_flags[flag as usize] = None;
                        }
                    }
                }
            }
        }

        // Phase 2: Update - start new goals
        for i in 0..self.goals.len() {
            let wrapped = &self.goals[i];

            // Skip if already running
            if wrapped.is_running {
                continue;
            }

            // Skip if has disabled flags
            if wrapped.flags().intersects(&self.disabled_flags) {
                continue;
            }

            // Check if we can take all required flags
            let can_take_flags = self.can_take_flags(i);
            if !can_take_flags {
                continue;
            }

            // Check if goal wants to run
            let wrapped = &mut self.goals[i];
            if !wrapped.goal.can_use(ctx) {
                continue;
            }

            // Stop conflicting goals and take their flags
            let flags = wrapped.flags();
            for flag in flags.iter() {
                if let Some(holder_idx) = self.locked_flags[flag as usize] {
                    if holder_idx != i {
                        let holder = &mut self.goals[holder_idx];
                        holder.goal.stop(ctx);
                        holder.is_running = false;
                    }
                }
                self.locked_flags[flag as usize] = Some(i);
            }

            // Start the goal
            let wrapped = &mut self.goals[i];
            wrapped.goal.start(ctx);
            wrapped.is_running = true;
        }

        // Phase 3: Tick - execute running goals
        self.tick_running_goals(ctx, true);
    }

    /// Ticks all running goals.
    ///
    /// If `tick_all` is false, only goals that require every-tick updates are ticked.
    pub fn tick_running_goals(&mut self, ctx: &mut GoalContext<'_>, tick_all: bool) {
        for wrapped in &mut self.goals {
            if wrapped.is_running && (tick_all || wrapped.goal.requires_update_every_tick()) {
                wrapped.goal.tick(ctx);
            }
        }
    }

    /// Checks if a goal at the given index can take all its required flags.
    fn can_take_flags(&self, goal_idx: usize) -> bool {
        let goal = &self.goals[goal_idx];
        let flags = goal.flags();

        for flag in flags.iter() {
            if let Some(holder_idx) = self.locked_flags[flag as usize] {
                if holder_idx != goal_idx {
                    let holder = &self.goals[holder_idx];
                    if !holder.can_be_replaced_by(goal) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Returns the number of goals in this selector
    #[must_use]
    pub fn goal_count(&self) -> usize {
        self.goals.len()
    }

    /// Returns the number of currently running goals
    #[must_use]
    pub fn running_count(&self) -> usize {
        self.goals.iter().filter(|g| g.is_running).count()
    }

    /// Returns debug info about running goals
    pub fn debug_info(&self) -> Vec<String> {
        self.goals
            .iter()
            .filter(|g| g.is_running)
            .map(|g| format!("[{}] {:?}", g.priority, g.goal))
            .collect()
    }
}

impl std::fmt::Debug for GoalSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoalSelector")
            .field("goal_count", &self.goals.len())
            .field("running_count", &self.running_count())
            .finish()
    }
}
