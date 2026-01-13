//! Entity behaviour system for type-specific entity data and NBT handling.
//!
//! This module provides a trait-based system for defining entity-specific behaviour,
//! following the same pattern as block behaviours in steel-registry.

mod display;
mod slime;
mod zombie;

pub use display::{DISPLAY_BEHAVIOUR, DisplayBehaviour};
pub use slime::{SLIME_BEHAVIOUR, SlimeBehaviour};
pub use zombie::{ZOMBIE_BEHAVIOUR, ZombieBehaviour};

use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::REGISTRY;
use steel_utils::Identifier;

use crate::chunk::chunk_map::ChunkMap;
use super::entity_tracker::EntityTracker;
use super::EntityData;

// =============================================================================
// NBT Helper Functions
// =============================================================================

/// Helper to parse boolean from NBT (accepts Byte or Int)
#[must_use]
pub fn nbt_bool(tag: &NbtTag) -> Option<bool> {
    match tag {
        NbtTag::Byte(b) => Some(*b != 0),
        NbtTag::Int(i) => Some(*i != 0),
        _ => None,
    }
}

/// Helper to parse i32 from NBT
#[must_use]
pub fn nbt_i32(tag: &NbtTag) -> Option<i32> {
    match tag {
        NbtTag::Byte(b) => Some(i32::from(*b)),
        NbtTag::Short(s) => Some(i32::from(*s)),
        NbtTag::Int(i) => Some(*i),
        _ => None,
    }
}

/// Parses a `block_state` NBT compound and returns the state ID
#[must_use]
pub fn parse_block_state(block_state: &NbtCompound) -> Option<i32> {
    let name = match block_state.get("Name")? {
        NbtTag::String(s) => s.to_str().to_string(),
        _ => return None,
    };

    let identifier = name.parse::<Identifier>().ok()?;

    let properties: Vec<(String, String)> =
        if let Some(NbtTag::Compound(props)) = block_state.get("Properties") {
            props
                .iter()
                .filter_map(|(k, v)| match v {
                    NbtTag::String(s) => Some((k.to_str().to_string(), s.to_str().to_string())),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };

    let props_refs: Vec<(&str, &str)> = properties
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let state_id = REGISTRY
        .blocks
        .state_id_from_properties(&identifier, &props_refs)?;
    Some(i32::from(state_id.0))
}

// =============================================================================
// EntityBehaviour Trait
// =============================================================================

/// Trait for entity-type-specific behavior.
///
/// Each entity type can have a behaviour that defines:
/// - Entity data fields (synched to client)
/// - NBT reading/writing
/// - Tick behaviour (AI, etc.)
///
/// Behaviours are looked up from the registry by `entity_type_id`.
pub trait EntityBehaviour: Send + Sync {
    /// Define entity-type-specific synched data.
    /// Called once when entity is created.
    fn define_entity_data(&self, _data: &mut EntityData) {}

    /// Read entity-type-specific NBT.
    /// Called after common NBT (position, rotation, flags) is applied.
    fn read_nbt(&self, _data: &mut EntityData, _nbt: &NbtCompound) {}

    /// Write entity-type-specific NBT.
    fn write_nbt(&self, _data: &EntityData, _nbt: &mut NbtCompound) {}

    /// Called every game tick to update entity-specific behavior.
    ///
    /// This is where AI goals are ticked, movement is processed, etc.
    /// The context provides access to the mob's position, rotation, and AI state.
    fn tick(&self, _ctx: &mut EntityTickContext<'_>) {}

    /// Returns true if this behaviour has a custom tick implementation.
    ///
    /// If false, the entity tracker can skip ticking this entity for performance.
    fn has_tick(&self) -> bool {
        false
    }
}

/// Context passed to entity behaviours during tick.
pub struct EntityTickContext<'a> {
    /// The entity ID
    pub entity_id: i32,
    /// The entity type ID
    pub entity_type_id: i32,
    /// Current position (mutable)
    pub position: &'a mut steel_utils::math::Vector3<f64>,
    /// Current rotation (yaw, pitch) (mutable)
    pub rotation: &'a mut (f32, f32),
    /// Current velocity (mutable)
    pub velocity: &'a mut steel_utils::math::Vector3<f64>,
    /// Entity data for synced fields (uses interior mutability)
    pub entity_data: &'a EntityData,
    /// Current game tick
    pub tick: u64,
    /// Random number generator
    pub random: &'a mut rand::rngs::StdRng,
    /// AI state (for mobs with AI)
    pub ai_state: Option<&'a mut super::ai::AiState>,
    /// Entity tracker for querying nearby entities
    pub entity_tracker: &'a EntityTracker,
    /// Chunk map for block queries (pathfinding)
    pub chunk_map: &'a ChunkMap,
}

// =============================================================================
// Default Behaviour
// =============================================================================

/// Default behaviour for entities without custom implementation.
pub struct DefaultEntityBehaviour;

impl EntityBehaviour for DefaultEntityBehaviour {}

/// Static instance of the default behaviour.
pub static DEFAULT_BEHAVIOUR: DefaultEntityBehaviour = DefaultEntityBehaviour;
