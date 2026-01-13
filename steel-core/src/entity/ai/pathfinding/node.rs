//! Path nodes for A* pathfinding.

use steel_utils::math::Vector3;

/// Type of path node, affecting movement cost and behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum PathNodeType {
    /// Blocked - cannot pass through
    Blocked = 0,
    /// Open air - can walk through
    #[default]
    Open = 1,
    /// Water - swimming required
    Water = 2,
    /// Walkable water (shallow)
    WaterBorder = 3,
    /// Lava - dangerous
    Lava = 4,
    /// Door - may be openable
    Door = 5,
    /// Fence - cannot pass
    Fence = 6,
    /// Fence gate - may be openable
    FenceGate = 7,
    /// Trapdoor - may be openable
    Trapdoor = 8,
    /// Rail - minecart track
    Rail = 9,
    /// Damage - causes harm (fire, cactus)
    Damage = 10,
    /// Danger - should avoid (magma)
    Danger = 11,
    /// Leaves - may be walkable
    Leaves = 12,
    /// Sticky - slows movement (honey, soul sand)
    Sticky = 13,
    /// Powder snow - special movement
    PowderSnow = 14,
}

impl PathNodeType {
    /// Returns the base movement cost for this node type.
    ///
    /// Higher values make the pathfinder avoid this type.
    #[must_use]
    pub fn malus(&self) -> f32 {
        match self {
            Self::Blocked => f32::MAX,
            Self::Open => 0.0,
            Self::Water => 8.0,
            Self::WaterBorder => 4.0,
            Self::Lava => f32::MAX,
            Self::Door => 0.0,
            Self::Fence => f32::MAX,
            Self::FenceGate => 0.0,
            Self::Trapdoor => 0.0,
            Self::Rail => 0.0,
            Self::Damage => 16.0,
            Self::Danger => 8.0,
            Self::Leaves => 0.0,
            Self::Sticky => 8.0,
            Self::PowderSnow => 4.0,
        }
    }

    /// Checks if this node type is passable at all
    #[must_use]
    pub fn is_passable(&self) -> bool {
        self.malus() < f32::MAX
    }
}

/// A node in the A* pathfinding grid.
#[derive(Debug, Clone)]
pub struct PathNode {
    /// Block position of this node
    pub pos: Vector3<i32>,
    /// Type of this node
    pub node_type: PathNodeType,
    /// Cost from start to this node (g)
    pub g_cost: f32,
    /// Estimated cost from this node to target (h)
    pub h_cost: f32,
    /// Parent node index in the open/closed set
    pub parent: Option<usize>,
    /// Whether this node has been visited
    pub visited: bool,
    /// Whether this node is in the open set
    pub in_open_set: bool,
    /// Additional cost penalty for this specific node
    pub cost_malus: f32,
}

impl PathNode {
    /// Creates a new path node
    #[must_use]
    pub fn new(pos: Vector3<i32>, node_type: PathNodeType) -> Self {
        Self {
            pos,
            node_type,
            g_cost: f32::MAX,
            h_cost: 0.0,
            parent: None,
            visited: false,
            in_open_set: false,
            cost_malus: node_type.malus(),
        }
    }

    /// Returns the total estimated cost (f = g + h)
    #[must_use]
    pub fn f_cost(&self) -> f32 {
        self.g_cost + self.h_cost
    }

    /// Returns the world position (center of block)
    #[must_use]
    pub fn world_pos(&self) -> Vector3<f64> {
        Vector3::new(
            f64::from(self.pos.x) + 0.5,
            f64::from(self.pos.y),
            f64::from(self.pos.z) + 0.5,
        )
    }

    /// Checks if this node is passable
    #[must_use]
    pub fn is_passable(&self) -> bool {
        self.node_type.is_passable()
    }

    /// Gets the combined cost including malus
    #[must_use]
    pub fn combined_cost(&self) -> f32 {
        self.g_cost + self.cost_malus
    }
}

impl PartialEq for PathNode {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}

impl Eq for PathNode {}

impl std::hash::Hash for PathNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.pos.hash(state);
    }
}
