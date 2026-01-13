//! Path representation for navigation.

use steel_utils::math::Vector3;

/// A navigation path consisting of waypoints.
#[derive(Debug, Clone)]
pub struct Path {
    /// The waypoints in this path
    nodes: Vec<PathWaypoint>,
    /// Current index in the path
    current_index: usize,
    /// The final target position
    target: Vector3<f64>,
}

/// A single waypoint in a path.
#[derive(Debug, Clone, Copy)]
pub struct PathWaypoint {
    /// The position of this waypoint
    pub position: Vector3<f64>,
    /// The type of node (walkable, water, etc.)
    pub node_type: super::PathNodeType,
}

impl Path {
    /// Creates a new path from waypoints
    #[must_use]
    pub fn new(nodes: Vec<PathWaypoint>, target: Vector3<f64>) -> Self {
        Self {
            nodes,
            current_index: 0,
            target,
        }
    }

    /// Creates an empty path (no waypoints)
    #[must_use]
    pub fn empty(target: Vector3<f64>) -> Self {
        Self {
            nodes: Vec::new(),
            current_index: 0,
            target,
        }
    }

    /// Returns the final target position
    #[must_use]
    pub fn target(&self) -> Vector3<f64> {
        self.target
    }

    /// Returns the current waypoint, if any
    #[must_use]
    pub fn current_node(&self) -> Option<&PathWaypoint> {
        self.nodes.get(self.current_index)
    }

    /// Returns the next waypoint position to move toward
    #[must_use]
    pub fn next_position(&self) -> Option<Vector3<f64>> {
        self.current_node().map(|n| n.position)
    }

    /// Advances to the next waypoint
    pub fn advance(&mut self) {
        if self.current_index < self.nodes.len() {
            self.current_index += 1;
        }
    }

    /// Checks if the path is complete (reached the end)
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.current_index >= self.nodes.len()
    }

    /// Returns the number of remaining waypoints
    #[must_use]
    pub fn remaining_nodes(&self) -> usize {
        self.nodes.len().saturating_sub(self.current_index)
    }

    /// Returns the total number of nodes in the path
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Checks if this path has any nodes
    #[must_use]
    pub fn has_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// Returns all remaining waypoints
    #[must_use]
    pub fn remaining_path(&self) -> &[PathWaypoint] {
        if self.current_index < self.nodes.len() {
            &self.nodes[self.current_index..]
        } else {
            &[]
        }
    }

    /// Gets the distance squared from a position to the current waypoint
    #[must_use]
    pub fn distance_to_current_squared(&self, pos: Vector3<f64>) -> Option<f64> {
        self.current_node().map(|node| {
            let dx = pos.x - node.position.x;
            let dy = pos.y - node.position.y;
            let dz = pos.z - node.position.z;
            dx * dx + dy * dy + dz * dz
        })
    }
}
