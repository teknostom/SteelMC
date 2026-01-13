//! Navigation system for mob pathfinding and movement.
//!
//! This module provides the `GroundNavigation` struct which handles:
//! - Creating paths using A* pathfinding
//! - Following path waypoints
//! - Determining block walkability for pathfinding

use steel_utils::math::Vector3;

use crate::chunk::chunk_map::ChunkMap;

use super::goal::GoalContext;
use super::pathfinding::{Path, PathNodeType, Pathfinder};

/// Ground navigation for mobs that walk on the ground.
///
/// Handles pathfinding and path following for ground-based mobs.
#[derive(Debug)]
pub struct GroundNavigation {
    /// Current path being followed
    path: Option<Path>,
    /// Speed modifier for movement
    speed_modifier: f64,
    /// Distance threshold for considering a waypoint reached
    reach_threshold: f64,
    /// Ticks until path needs to be recalculated
    path_recalculate_time: u32,
    /// Last target position (to detect if target moved)
    last_target: Option<Vector3<f64>>,
}

impl Default for GroundNavigation {
    fn default() -> Self {
        Self::new()
    }
}

impl GroundNavigation {
    /// Creates a new ground navigation instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: None,
            speed_modifier: 1.0,
            reach_threshold: 0.5,
            path_recalculate_time: 0,
            last_target: None,
        }
    }

    /// Creates a path to the target position
    ///
    /// This will compute an A* path from the current position to the target.
    pub fn move_to(&mut self, target: Vector3<f64>, speed: f64, ctx: &GoalContext<'_>) {
        // Compute path (pathfinder takes f64 positions and converts internally)
        let mut pathfinder = Pathfinder::new();
        self.path = pathfinder.find_path(ctx.position, target, |pos| {
            Self::get_node_type(pos, ctx.chunk_map)
        });

        self.speed_modifier = speed;
        self.last_target = Some(target);
        self.path_recalculate_time = 20; // Recalculate every second
    }

    /// Checks if navigation should recalculate path to the target
    #[must_use]
    pub fn should_recalculate(&self, target: Vector3<f64>) -> bool {
        // Recalculate if no path
        if self.path.is_none() {
            return true;
        }

        // Recalculate if target moved significantly
        if let Some(last) = self.last_target {
            let dx = target.x - last.x;
            let dy = target.y - last.y;
            let dz = target.z - last.z;
            let dist_sq = dx * dx + dy * dy + dz * dz;
            if dist_sq > 1.0 {
                return true;
            }
        }

        // Recalculate periodically
        self.path_recalculate_time == 0
    }

    /// Ticks the navigation, advancing along the path
    ///
    /// Returns the desired movement direction and speed, or None if no movement needed
    pub fn tick(&mut self, ctx: &GoalContext<'_>) -> Option<(Vector3<f64>, f64)> {
        // Decrement recalculate timer
        self.path_recalculate_time = self.path_recalculate_time.saturating_sub(1);

        let path = self.path.as_mut()?;

        // Check if path is complete
        if path.is_done() {
            self.path = None;
            return None;
        }

        // Get next waypoint
        let next_pos = path.next_position()?;

        // Calculate distance to waypoint (horizontal only for ground navigation)
        let dx = next_pos.x - ctx.position.x;
        let dz = next_pos.z - ctx.position.z;
        let horizontal_dist = (dx * dx + dz * dz).sqrt();

        // Check if we've reached the waypoint
        if horizontal_dist < self.reach_threshold {
            path.advance();

            // Get next waypoint after advancing
            if let Some(new_next) = path.next_position() {
                let new_dx = new_next.x - ctx.position.x;
                let new_dz = new_next.z - ctx.position.z;
                let new_dist = (new_dx * new_dx + new_dz * new_dz).sqrt();

                if new_dist > 0.01 {
                    return Some((
                        Vector3::new(new_dx / new_dist, 0.0, new_dz / new_dist),
                        self.speed_modifier,
                    ));
                }
            }
            return None;
        }

        // Return direction to waypoint
        Some((
            Vector3::new(dx / horizontal_dist, 0.0, dz / horizontal_dist),
            self.speed_modifier,
        ))
    }

    /// Checks if the navigation has an active path
    #[must_use]
    pub fn has_path(&self) -> bool {
        self.path.as_ref().is_some_and(|p: &Path| !p.is_done())
    }

    /// Clears the current path
    pub fn stop(&mut self) {
        self.path = None;
        self.last_target = None;
    }

    /// Determines the path node type for a block position
    ///
    /// This queries the world to determine if a position is walkable.
    fn get_node_type(pos: Vector3<i32>, chunk_map: &ChunkMap) -> PathNodeType {
        // Get block at feet level, head level, and below
        let below = chunk_map.get_block_state(pos.x, pos.y - 1, pos.z);
        let feet = chunk_map.get_block_state(pos.x, pos.y, pos.z);
        let head = chunk_map.get_block_state(pos.x, pos.y + 1, pos.z);

        // If we can't get block data, treat as blocked (unloaded chunk)
        let Some(below_state) = below else {
            return PathNodeType::Blocked;
        };
        let Some(feet_state) = feet else {
            return PathNodeType::Blocked;
        };
        let Some(head_state) = head else {
            return PathNodeType::Blocked;
        };

        // Air has state ID 0
        let below_is_air = below_state.0 == 0;
        let feet_is_air = feet_state.0 == 0;
        let head_is_air = head_state.0 == 0;

        // Need solid block below, air at feet and head
        if below_is_air {
            // No ground to stand on
            return PathNodeType::Blocked;
        }

        if !feet_is_air || !head_is_air {
            // Something blocking at feet or head level
            return PathNodeType::Blocked;
        }

        // Position is walkable
        PathNodeType::Open
    }
}
