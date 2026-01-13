//! Pathfinding system for mob AI.
//!
//! This module provides weighted A* pathfinding for mobs, allowing them to
//! navigate around obstacles to reach their targets.

mod node;
mod path;
mod pathfinder;

pub use node::{PathNode, PathNodeType};
pub use path::{Path, PathWaypoint};
pub use pathfinder::Pathfinder;
