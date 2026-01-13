//! Mob AI system for SteelMC.
//!
//! This module provides the AI behavior system for mobs, including:
//! - Goals: Individual behaviors (attacking, wandering, looking, etc.)
//! - Goal Selector: Priority-based goal scheduling with flag conflict resolution
//! - Target Selector: Finding and tracking targets
//! - Pathfinding: A* navigation for mob movement
//! - Navigation: Path following and movement control

pub mod goal;
pub mod goal_selector;
pub mod goals;
pub mod move_control;
pub mod navigation;
pub mod pathfinding;
pub mod target;

pub use goal::{AiState, Goal, GoalContext, GoalFlag, GoalFlags, TargetInfo};
pub use goal_selector::GoalSelector;
pub use move_control::MoveControl;
pub use navigation::GroundNavigation;
