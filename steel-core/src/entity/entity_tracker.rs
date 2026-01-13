//! Entity tracking system
//!
//! Manages which entities are visible to which players based on distance and chunks.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use uuid::Uuid;

use super::{Entity, TrackedEntity};
use crate::player::Player;
use steel_utils::ChunkPos;
use steel_utils::locks::SyncRwLock;

/// Default entity tracking range in blocks (4 chunks)
pub const DEFAULT_ENTITY_TRACKING_RANGE_BLOCKS: i32 = 64;

/// Entity tracker that manages visibility between entities and players
pub struct EntityTracker {
    /// Map of entity ID to tracked entity
    tracked_entities: SyncRwLock<FxHashMap<i32, Arc<TrackedEntity>>>,

    /// Map of player UUID to their visible entities
    player_tracking: SyncRwLock<FxHashMap<Uuid, FxHashSet<i32>>>,

    /// Global entity ID counter (uses `AtomicI32` for lock-free allocation)
    next_entity_id: std::sync::atomic::AtomicI32,
}

impl EntityTracker {
    /// Creates a new entity tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracked_entities: SyncRwLock::new(FxHashMap::default()),
            player_tracking: SyncRwLock::new(FxHashMap::default()),
            next_entity_id: std::sync::atomic::AtomicI32::new(0),
        }
    }

    /// Allocates a new unique entity ID
    pub fn allocate_entity_id(&self) -> i32 {
        self.next_entity_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Adds an entity to tracking
    ///
    /// `tracking_range_blocks` should be the tracking range in blocks.
    /// Use `EntityType::tracking_range_blocks()` to convert from chunks.
    pub fn add_entity(&self, entity: Arc<dyn Entity>, tracking_range_blocks: Option<i32>) {
        let entity_id = entity.entity_id();
        let range = tracking_range_blocks.unwrap_or(DEFAULT_ENTITY_TRACKING_RANGE_BLOCKS);

        let tracked = Arc::new(TrackedEntity::new(entity, range));
        self.tracked_entities.write().insert(entity_id, tracked);
    }

    /// Removes an entity from tracking
    pub fn remove_entity(&self, entity_id: i32) {
        if let Some(tracked) = self.tracked_entities.write().remove(&entity_id) {
            // Remove from all players' tracking sets
            tracked.broadcast_removal();

            let mut player_tracking = self.player_tracking.write();
            for visible_set in player_tracking.values_mut() {
                visible_set.remove(&entity_id);
            }
        }
    }

    /// Updates entity visibility for all players
    ///
    /// This should be called every tick to update which entities players can see
    pub fn tick(&self, players: &[Arc<Player>]) {
        let tracked_entities = self.tracked_entities.read();
        let mut player_tracking = self.player_tracking.write();

        for player in players {
            let player_uuid = player.gameprofile.id;
            let player_pos = *player.position.lock();
            let player_chunk = *player.last_chunk_pos.lock();

            // Get or create tracking set for this player
            let visible_entities = player_tracking.entry(player_uuid).or_default();

            // Check each tracked entity
            for tracked in tracked_entities.values() {
                let entity_id = tracked.entity.entity_id();

                // Don't track self
                if tracked.entity.uuid() == player_uuid {
                    continue;
                }

                let should_track =
                    Self::should_track_entity(player, player_pos, player_chunk, tracked);
                let currently_tracked = visible_entities.contains(&entity_id);

                if should_track && !currently_tracked {
                    // Start tracking
                    visible_entities.insert(entity_id);
                    tracked.add_player(player.clone());
                } else if !should_track && currently_tracked {
                    // Stop tracking
                    visible_entities.remove(&entity_id);
                    tracked.remove_player(player_uuid);
                }
            }
        }

        // Send updates for entities that moved or changed
        for tracked in tracked_entities.values() {
            tracked.send_changes();
        }
    }

    /// Checks if an entity should be tracked by a player based on distance and chunk visibility
    fn should_track_entity(
        player: &Player,
        player_pos: steel_utils::math::Vector3<f64>,
        player_chunk: ChunkPos,
        tracked: &TrackedEntity,
    ) -> bool {
        let entity_pos = tracked.entity.position();

        // Calculate squared distance (avoid sqrt)
        let dx = player_pos.x - entity_pos.x;
        let dy = player_pos.y - entity_pos.y;
        let dz = player_pos.z - entity_pos.z;
        let distance_squared = dx * dx + dy * dy + dz * dz;

        let tracking_range_squared = f64::from(tracked.tracking_range_blocks).powi(2);

        distance_squared <= tracking_range_squared
            && Self::is_chunk_tracked(player, player_chunk, &entity_pos)
    }

    /// Checks if a chunk is being tracked by the player
    fn is_chunk_tracked(
        player: &Player,
        player_chunk: ChunkPos,
        entity_pos: &steel_utils::math::Vector3<f64>,
    ) -> bool {
        #[allow(clippy::cast_possible_truncation)]
        let entity_chunk = ChunkPos::new((entity_pos.x as i32) >> 4, (entity_pos.z as i32) >> 4);

        // Get player's chunk view
        let tracking_view = player.last_tracking_view.lock();
        if let Some(view) = tracking_view.as_ref() {
            view.contains(entity_chunk)
        } else {
            // If no tracking view yet, just check if same chunk
            player_chunk == entity_chunk
        }
    }

    /// Gets a tracked entity by ID
    pub fn get_entity(&self, entity_id: i32) -> Option<Arc<TrackedEntity>> {
        self.tracked_entities.read().get(&entity_id).cloned()
    }

    /// Gets a tracked entity by UUID
    pub fn get_entity_by_uuid(&self, uuid: Uuid) -> Option<Arc<TrackedEntity>> {
        self.tracked_entities
            .read()
            .values()
            .find(|e| e.entity.uuid() == uuid)
            .cloned()
    }

    /// Removes an entity by UUID
    pub fn remove_entity_by_uuid(&self, uuid: Uuid) -> bool {
        let entity_id = {
            let entities = self.tracked_entities.read();
            entities
                .values()
                .find(|e| e.entity.uuid() == uuid)
                .map(|e| e.entity.entity_id())
        };

        if let Some(id) = entity_id {
            self.remove_entity(id);
            true
        } else {
            false
        }
    }

    /// Removes a player from all entity tracking
    pub fn remove_player(&self, player_uuid: Uuid) {
        // Remove player from all tracked entities
        let tracked_entities = self.tracked_entities.read();
        for tracked in tracked_entities.values() {
            tracked.remove_player(player_uuid);
        }

        // Remove player's tracking set
        self.player_tracking.write().remove(&player_uuid);
    }

    /// Gets the number of tracked entities
    pub fn entity_count(&self) -> usize {
        self.tracked_entities.read().len()
    }

    /// Ticks all entity behaviors
    ///
    /// This updates entity AI, movement, and other behaviors.
    /// Should be called every game tick (20 times per second).
    pub fn tick_entities(&self, tick_count: u64, chunk_map: &crate::chunk::chunk_map::ChunkMap) {
        let tracked_entities = self.tracked_entities.read();

        for tracked in tracked_entities.values() {
            // Only tick entities that have custom tick behavior
            if tracked.entity.has_tick() {
                tracked.entity.tick(tick_count, self, chunk_map);
            }
        }
    }

    /// Finds all entities within a radius of a position.
    ///
    /// Returns entities sorted by distance (closest first).
    /// The `exclude_id` parameter can be used to exclude the querying entity itself.
    pub fn get_entities_in_radius(
        &self,
        center: steel_utils::math::Vector3<f64>,
        radius: f64,
        exclude_id: Option<i32>,
    ) -> Vec<(i32, steel_utils::math::Vector3<f64>, f64)> {
        let radius_squared = radius * radius;
        let tracked_entities = self.tracked_entities.read();

        let mut results: Vec<(i32, steel_utils::math::Vector3<f64>, f64)> = tracked_entities
            .values()
            .filter_map(|tracked| {
                let entity_id = tracked.entity.entity_id();

                // Skip excluded entity
                if exclude_id == Some(entity_id) {
                    return None;
                }

                let pos = tracked.entity.position();
                let dx = pos.x - center.x;
                let dy = pos.y - center.y;
                let dz = pos.z - center.z;
                let distance_squared = dx * dx + dy * dy + dz * dz;

                if distance_squared <= radius_squared {
                    Some((entity_id, pos, distance_squared))
                } else {
                    None
                }
            })
            .collect();

        // Sort by distance (closest first)
        results.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Finds all entities of a specific type within a radius.
    ///
    /// Returns entities sorted by distance (closest first).
    pub fn get_entities_by_type_in_radius(
        &self,
        center: steel_utils::math::Vector3<f64>,
        radius: f64,
        entity_type_id: i32,
        exclude_id: Option<i32>,
    ) -> Vec<(i32, steel_utils::math::Vector3<f64>, f64)> {
        let radius_squared = radius * radius;
        let tracked_entities = self.tracked_entities.read();

        let mut results: Vec<(i32, steel_utils::math::Vector3<f64>, f64)> = tracked_entities
            .values()
            .filter_map(|tracked| {
                let entity_id = tracked.entity.entity_id();

                // Skip excluded entity
                if exclude_id == Some(entity_id) {
                    return None;
                }

                // Check entity type
                if tracked.entity.entity_type_id() != entity_type_id {
                    return None;
                }

                let pos = tracked.entity.position();
                let dx = pos.x - center.x;
                let dy = pos.y - center.y;
                let dz = pos.z - center.z;
                let distance_squared = dx * dx + dy * dy + dz * dz;

                if distance_squared <= radius_squared {
                    Some((entity_id, pos, distance_squared))
                } else {
                    None
                }
            })
            .collect();

        // Sort by distance (closest first)
        results.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Finds the nearest entity matching a filter.
    ///
    /// The filter function receives the entity type ID and returns true if the entity should be considered.
    pub fn get_nearest_entity<F>(
        &self,
        center: steel_utils::math::Vector3<f64>,
        radius: f64,
        exclude_id: Option<i32>,
        filter: F,
    ) -> Option<(i32, steel_utils::math::Vector3<f64>, f64)>
    where
        F: Fn(i32) -> bool,
    {
        let radius_squared = radius * radius;
        let tracked_entities = self.tracked_entities.read();

        let mut nearest: Option<(i32, steel_utils::math::Vector3<f64>, f64)> = None;

        for tracked in tracked_entities.values() {
            let entity_id = tracked.entity.entity_id();

            // Skip excluded entity
            if exclude_id == Some(entity_id) {
                continue;
            }

            // Apply filter
            if !filter(tracked.entity.entity_type_id()) {
                continue;
            }

            let pos = tracked.entity.position();
            let dx = pos.x - center.x;
            let dy = pos.y - center.y;
            let dz = pos.z - center.z;
            let distance_squared = dx * dx + dy * dy + dz * dz;

            if distance_squared <= radius_squared {
                if nearest.is_none() || distance_squared < nearest.as_ref().unwrap().2 {
                    nearest = Some((entity_id, pos, distance_squared));
                }
            }
        }

        nearest
    }

    /// Immediately updates visibility for a specific player (called when player joins)
    /// This matches vanilla's behavior of calling `updatePlayers()` right after adding an entity
    pub fn update_player_visibility(&self, player: &Arc<Player>) {
        let player_uuid = player.gameprofile.id;
        let player_pos = *player.position.lock();

        #[allow(clippy::cast_possible_truncation)]
        let player_chunk = ChunkPos::new((player_pos.x as i32) >> 4, (player_pos.z as i32) >> 4);

        // IMPORTANT: Lock ordering must match tick() to prevent deadlock
        // Always: tracked_entities first, then player_tracking
        let tracked_entities = self.tracked_entities.read();
        let mut player_tracking = self.player_tracking.write();
        let visible_entities = player_tracking.entry(player_uuid).or_default();

        for tracked in tracked_entities.values() {
            let entity_id = tracked.entity.entity_id();

            // Don't track self
            if tracked.entity.uuid() == player_uuid {
                continue;
            }

            if Self::should_track_entity(player, player_pos, player_chunk, tracked) {
                visible_entities.insert(entity_id);
                tracked.add_player(player.clone());
            }
        }
    }
}

impl Default for EntityTracker {
    fn default() -> Self {
        Self::new()
    }
}
