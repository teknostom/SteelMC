//! A* pathfinding implementation.

use std::collections::BinaryHeap;

use rustc_hash::FxHashMap;
use steel_utils::math::Vector3;

use super::{Path, PathNode, PathNodeType};
use crate::entity::ai::pathfinding::path::PathWaypoint;

/// Maximum number of nodes to explore before giving up
const MAX_ITERATIONS: usize = 200;

/// Maximum path length in nodes
const MAX_PATH_LENGTH: usize = 64;

/// A* pathfinder for mob navigation.
pub struct Pathfinder {
    /// Open set (nodes to explore)
    open_set: BinaryHeap<OrderedNode>,
    /// All nodes by position
    nodes: FxHashMap<Vector3<i32>, PathNode>,
    /// Node index counter
    node_count: usize,
}

/// Wrapper for PathNode that implements Ord for the priority queue.
#[derive(Debug)]
struct OrderedNode {
    f_cost: f32,
    node_index: usize,
    pos: Vector3<i32>,
}

impl PartialEq for OrderedNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_cost == other.f_cost
    }
}

impl Eq for OrderedNode {}

impl PartialOrd for OrderedNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap behavior
        other
            .f_cost
            .partial_cmp(&self.f_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Default for Pathfinder {
    fn default() -> Self {
        Self::new()
    }
}

impl Pathfinder {
    /// Creates a new pathfinder
    #[must_use]
    pub fn new() -> Self {
        Self {
            open_set: BinaryHeap::new(),
            nodes: FxHashMap::default(),
            node_count: 0,
        }
    }

    /// Finds a path from start to target.
    ///
    /// `get_node_type` is a callback that returns the node type for a given position.
    /// This allows the pathfinder to query the world for block data.
    pub fn find_path<F>(
        &mut self,
        start: Vector3<f64>,
        target: Vector3<f64>,
        get_node_type: F,
    ) -> Option<Path>
    where
        F: Fn(Vector3<i32>) -> PathNodeType,
    {
        self.clear();

        let start_pos = to_block_pos(start);
        let target_pos = to_block_pos(target);

        // Check if target is reachable at all
        if !get_node_type(target_pos).is_passable() {
            return None;
        }

        // Initialize start node
        let start_node = PathNode::new(start_pos, get_node_type(start_pos));
        let mut start_node = start_node;
        start_node.g_cost = 0.0;
        start_node.h_cost = heuristic(start_pos, target_pos);
        start_node.in_open_set = true;

        self.nodes.insert(start_pos, start_node);
        self.open_set.push(OrderedNode {
            f_cost: heuristic(start_pos, target_pos),
            node_index: 0,
            pos: start_pos,
        });
        self.node_count = 1;

        let mut iterations = 0;

        while let Some(current) = self.open_set.pop() {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                // Give up and return partial path
                return self.reconstruct_partial_path(current.pos, target);
            }

            let current_pos = current.pos;

            // Check if we reached the target
            if current_pos == target_pos {
                return Some(self.reconstruct_path(current_pos, target));
            }

            // Mark as visited
            if let Some(node) = self.nodes.get_mut(&current_pos) {
                if node.visited {
                    continue;
                }
                node.visited = true;
                node.in_open_set = false;
            }

            // Explore neighbors
            for neighbor_pos in get_neighbors(current_pos) {
                let node_type = get_node_type(neighbor_pos);
                if !node_type.is_passable() {
                    continue;
                }

                let current_g = self.nodes.get(&current_pos).map(|n| n.g_cost).unwrap_or(0.0);
                let move_cost = if neighbor_pos.x != current_pos.x && neighbor_pos.z != current_pos.z
                {
                    1.414 // Diagonal
                } else {
                    1.0
                };
                let new_g = current_g + move_cost + node_type.malus();

                let neighbor = self.nodes.entry(neighbor_pos).or_insert_with(|| {
                    let mut node = PathNode::new(neighbor_pos, node_type);
                    node.h_cost = heuristic(neighbor_pos, target_pos);
                    self.node_count += 1;
                    node
                });

                if neighbor.visited {
                    continue;
                }

                if new_g < neighbor.g_cost {
                    neighbor.g_cost = new_g;
                    neighbor.parent = Some(self.node_count - 1); // Store parent position index
                    neighbor.node_type = node_type;

                    if !neighbor.in_open_set {
                        neighbor.in_open_set = true;
                        self.open_set.push(OrderedNode {
                            f_cost: neighbor.f_cost(),
                            node_index: self.node_count,
                            pos: neighbor_pos,
                        });
                    }
                }
            }
        }

        None
    }

    /// Clears the pathfinder state
    fn clear(&mut self) {
        self.open_set.clear();
        self.nodes.clear();
        self.node_count = 0;
    }

    /// Reconstructs the path from start to the given position
    fn reconstruct_path(&self, end_pos: Vector3<i32>, target: Vector3<f64>) -> Path {
        let mut waypoints = Vec::new();
        let mut current_pos = Some(end_pos);
        let mut visited_count = 0;

        while let Some(pos) = current_pos {
            if visited_count > MAX_PATH_LENGTH {
                break;
            }
            visited_count += 1;

            if let Some(node) = self.nodes.get(&pos) {
                waypoints.push(PathWaypoint {
                    position: node.world_pos(),
                    node_type: node.node_type,
                });

                // Find parent by looking for the node with the lowest g_cost that could be parent
                current_pos = self.find_parent_pos(pos);
            } else {
                break;
            }
        }

        waypoints.reverse();
        Path::new(waypoints, target)
    }

    /// Reconstructs a partial path to the closest explored node to target
    fn reconstruct_partial_path(&self, current_pos: Vector3<i32>, target: Vector3<f64>) -> Option<Path> {
        if self.nodes.is_empty() {
            return None;
        }

        // Find the node closest to the target that we've explored
        let best_node = self
            .nodes
            .values()
            .filter(|n| n.visited)
            .min_by(|a, b| a.h_cost.partial_cmp(&b.h_cost).unwrap_or(std::cmp::Ordering::Equal))?;

        Some(self.reconstruct_path(best_node.pos, target))
    }

    /// Finds the parent position for path reconstruction
    fn find_parent_pos(&self, pos: Vector3<i32>) -> Option<Vector3<i32>> {
        let current_node = self.nodes.get(&pos)?;
        let current_g = current_node.g_cost;

        if current_g <= 0.0 {
            return None;
        }

        // Find neighbor with lower g_cost (the parent)
        get_neighbors(pos)
            .into_iter()
            .filter_map(|neighbor_pos| {
                self.nodes.get(&neighbor_pos).and_then(|n| {
                    if n.g_cost < current_g && n.visited {
                        Some((neighbor_pos, n.g_cost))
                    } else {
                        None
                    }
                })
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(pos, _)| pos)
    }
}

/// Converts a world position to block position
fn to_block_pos(pos: Vector3<f64>) -> Vector3<i32> {
    Vector3::new(pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32)
}

/// Manhattan distance heuristic
fn heuristic(from: Vector3<i32>, to: Vector3<i32>) -> f32 {
    let dx = (from.x - to.x).abs();
    let dy = (from.y - to.y).abs();
    let dz = (from.z - to.z).abs();
    (dx + dy + dz) as f32
}

/// Gets the 6 cardinal neighbors + 4 diagonal neighbors on the same Y level
fn get_neighbors(pos: Vector3<i32>) -> [Vector3<i32>; 10] {
    [
        // Cardinal directions
        Vector3::new(pos.x + 1, pos.y, pos.z),
        Vector3::new(pos.x - 1, pos.y, pos.z),
        Vector3::new(pos.x, pos.y, pos.z + 1),
        Vector3::new(pos.x, pos.y, pos.z - 1),
        // Vertical
        Vector3::new(pos.x, pos.y + 1, pos.z),
        Vector3::new(pos.x, pos.y - 1, pos.z),
        // Diagonals (same Y)
        Vector3::new(pos.x + 1, pos.y, pos.z + 1),
        Vector3::new(pos.x + 1, pos.y, pos.z - 1),
        Vector3::new(pos.x - 1, pos.y, pos.z + 1),
        Vector3::new(pos.x - 1, pos.y, pos.z - 1),
    ]
}
