//! Goal trait and types for mob AI.
//!
//! Goals are the building blocks of mob AI behavior. Each goal represents
//! a specific behavior (e.g., attacking, wandering, looking at players).
//! Goals are managed by a `GoalSelector` which handles priority and flag conflicts.

use std::fmt::Debug;

use crate::chunk::chunk_map::ChunkMap;
use crate::entity::EntityTracker;

/// Flags that goals can use to indicate what entity controls they need.
///
/// Goals with conflicting flags cannot run simultaneously. For example,
/// two goals that both require `Move` cannot run at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GoalFlag {
    /// Controls entity movement
    Move = 0,
    /// Controls where the entity looks
    Look = 1,
    /// Controls jumping behavior
    Jump = 2,
    /// Controls target selection
    Target = 3,
}

impl GoalFlag {
    /// Total number of goal flags
    pub const COUNT: usize = 4;

    /// All goal flags
    pub const ALL: [GoalFlag; 4] = [
        GoalFlag::Move,
        GoalFlag::Look,
        GoalFlag::Jump,
        GoalFlag::Target,
    ];
}

/// A set of goal flags, implemented as a bitfield for efficiency.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GoalFlags(u8);

impl GoalFlags {
    /// Creates an empty flag set
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Creates a flag set from a single flag
    #[must_use]
    pub const fn from_flag(flag: GoalFlag) -> Self {
        Self(1 << flag as u8)
    }

    /// Creates a flag set from multiple flags
    #[must_use]
    pub fn from_flags(flags: &[GoalFlag]) -> Self {
        let mut bits = 0u8;
        for flag in flags {
            bits |= 1 << *flag as u8;
        }
        Self(bits)
    }

    /// Adds a flag to the set
    pub fn insert(&mut self, flag: GoalFlag) {
        self.0 |= 1 << flag as u8;
    }

    /// Removes a flag from the set
    pub fn remove(&mut self, flag: GoalFlag) {
        self.0 &= !(1 << flag as u8);
    }

    /// Checks if the set contains a flag
    #[must_use]
    pub const fn contains(&self, flag: GoalFlag) -> bool {
        (self.0 & (1 << flag as u8)) != 0
    }

    /// Checks if this set intersects with another
    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Checks if the set is empty
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Clears all flags
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Returns the raw bits
    #[must_use]
    pub const fn bits(&self) -> u8 {
        self.0
    }

    /// Iterator over contained flags
    pub fn iter(&self) -> impl Iterator<Item = GoalFlag> + '_ {
        GoalFlag::ALL.iter().copied().filter(|f| self.contains(*f))
    }
}

impl From<GoalFlag> for GoalFlags {
    fn from(flag: GoalFlag) -> Self {
        Self::from_flag(flag)
    }
}

impl FromIterator<GoalFlag> for GoalFlags {
    fn from_iter<T: IntoIterator<Item = GoalFlag>>(iter: T) -> Self {
        let mut flags = Self::empty();
        for flag in iter {
            flags.insert(flag);
        }
        flags
    }
}

/// A goal that controls a specific aspect of mob behavior.
///
/// Goals are evaluated and executed by a `GoalSelector`. Each tick, the selector
/// determines which goals should be running based on:
/// 1. Priority (lower number = higher priority)
/// 2. Flag conflicts (goals with conflicting flags cannot run simultaneously)
/// 3. The goal's `can_use()` and `can_continue_to_use()` methods
///
/// # Example
/// ```ignore
/// struct LookAtPlayerGoal {
///     look_distance: f32,
///     // ...
/// }
///
/// impl Goal for LookAtPlayerGoal {
///     fn flags(&self) -> GoalFlags {
///         GoalFlags::from_flag(GoalFlag::Look)
///     }
///
///     fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
///         // Check if there's a player nearby to look at
///         true
///     }
///
///     fn tick(&mut self, ctx: &mut GoalContext<'_>) {
///         // Update look direction toward player
///     }
/// }
/// ```
pub trait Goal: Send + Sync + Debug {
    /// Returns the flags this goal uses.
    ///
    /// Goals with overlapping flags cannot run simultaneously.
    fn flags(&self) -> GoalFlags;

    /// Checks if this goal can start being used.
    ///
    /// Called every tick for inactive goals to determine if they should start.
    /// Returns `true` if the goal should start running.
    fn can_use(&mut self, ctx: &mut GoalContext<'_>) -> bool;

    /// Checks if this goal should continue running.
    ///
    /// Called every tick for active goals. Default implementation calls `can_use()`.
    /// Returns `true` if the goal should continue running.
    fn can_continue_to_use(&mut self, ctx: &mut GoalContext<'_>) -> bool {
        self.can_use(ctx)
    }

    /// Returns whether this goal can be interrupted by a higher-priority goal.
    ///
    /// Default is `true`. Set to `false` for goals that should not be interrupted
    /// once started (e.g., critical animations).
    fn is_interruptable(&self) -> bool {
        true
    }

    /// Returns whether this goal needs to be ticked every game tick.
    ///
    /// Default is `false`, meaning the goal is ticked at a reduced rate.
    /// Set to `true` for goals that need precise timing (e.g., attack goals).
    fn requires_update_every_tick(&self) -> bool {
        false
    }

    /// Called when the goal starts running.
    ///
    /// Use this to initialize state, start animations, etc.
    fn start(&mut self, _ctx: &mut GoalContext<'_>) {}

    /// Called when the goal stops running.
    ///
    /// Use this to clean up state, stop animations, etc.
    fn stop(&mut self, _ctx: &mut GoalContext<'_>) {}

    /// Called every tick while the goal is running.
    ///
    /// This is where the main behavior logic goes.
    fn tick(&mut self, _ctx: &mut GoalContext<'_>) {}
}

/// Context passed to goals during evaluation and execution.
///
/// Provides access to the mob, world, and other entities needed for goal logic.
pub struct GoalContext<'a> {
    /// The entity ID of the mob this goal belongs to
    pub entity_id: i32,

    /// The mob's current position
    pub position: steel_utils::math::Vector3<f64>,

    /// The mob's current rotation (yaw, pitch)
    pub rotation: (f32, f32),

    /// The current game tick
    pub tick: u64,

    /// Random number generator for stochastic behavior
    pub random: &'a mut rand::rngs::StdRng,

    /// The mob's current target (if any)
    pub target: Option<TargetInfo>,

    /// Mutable reference to the mob's AI state
    pub ai_state: &'a mut AiState,

    /// Entity tracker for querying nearby entities
    pub entity_tracker: &'a EntityTracker,

    /// Chunk map for block queries (pathfinding)
    pub chunk_map: &'a ChunkMap,
}

/// Information about a target entity.
#[derive(Debug, Clone)]
pub struct TargetInfo {
    /// The target's entity ID
    pub entity_id: i32,
    /// The target's position
    pub position: steel_utils::math::Vector3<f64>,
    /// The target's UUID
    pub uuid: uuid::Uuid,
    /// Distance squared from the mob to the target
    pub distance_squared: f64,
}

/// Shared AI state for a mob.
///
/// This holds state that needs to be shared between goals and persisted
/// between ticks.
#[derive(Debug, Default)]
pub struct AiState {
    /// The current target entity ID (if any)
    pub target_entity_id: Option<i32>,
    /// Ticks since the target was last seen
    pub unseen_target_ticks: u32,
    /// Whether the mob is aggressive (arms raised for zombies)
    pub is_aggressive: bool,
    /// Current navigation path (if any)
    pub current_path: Option<crate::entity::ai::pathfinding::Path>,
    /// Desired movement speed modifier
    pub speed_modifier: f64,
    /// Desired look target position
    pub look_target: Option<steel_utils::math::Vector3<f64>>,
    /// Ticks since last attack
    pub ticks_since_attack: u32,
    /// Last time the mob was hurt (game tick)
    pub last_hurt_time: u64,
    /// Entity ID of the last entity that hurt this mob
    pub last_hurt_by: Option<i32>,
}

impl<'a> GoalContext<'a> {
    /// Creates a new goal context
    pub fn new(
        entity_id: i32,
        position: steel_utils::math::Vector3<f64>,
        rotation: (f32, f32),
        tick: u64,
        random: &'a mut rand::rngs::StdRng,
        ai_state: &'a mut AiState,
        entity_tracker: &'a EntityTracker,
        chunk_map: &'a ChunkMap,
    ) -> Self {
        Self {
            entity_id,
            position,
            rotation,
            tick,
            random,
            target: None,
            ai_state,
            entity_tracker,
            chunk_map,
        }
    }

    /// Sets the target for this context
    pub fn with_target(mut self, target: Option<TargetInfo>) -> Self {
        self.target = target;
        self
    }

    /// Gets the distance squared to a position
    pub fn distance_squared_to(&self, pos: steel_utils::math::Vector3<f64>) -> f64 {
        let dx = self.position.x - pos.x;
        let dy = self.position.y - pos.y;
        let dz = self.position.z - pos.z;
        dx * dx + dy * dy + dz * dz
    }
}

/// Reduces a tick delay for goals that don't require every-tick updates.
///
/// This matches vanilla's `reducedTickDelay` method.
#[must_use]
pub fn reduced_tick_delay(ticks: u32) -> u32 {
    (ticks + 1) / 2
}

/// Adjusts a tick delay based on whether the goal requires every-tick updates.
#[must_use]
pub fn adjusted_tick_delay(ticks: u32, requires_every_tick: bool) -> u32 {
    if requires_every_tick {
        ticks
    } else {
        reduced_tick_delay(ticks)
    }
}
