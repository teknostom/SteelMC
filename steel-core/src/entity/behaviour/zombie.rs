//! Zombie entity behaviour with AI goals.

use super::{EntityBehaviour, EntityTickContext};
use crate::entity::ai::goal::{AiState, GoalContext, TargetInfo};
use rand::SeedableRng;
use crate::entity::ai::goal_selector::GoalSelector;
use crate::entity::ai::goals::{
    LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType};
use crate::entity::EntityData;

use simdnbt::owned::NbtCompound;
use steel_utils::locks::SyncMutex;

/// Zombie entity behaviour.
///
/// Implements the AI goals and targeting for zombies, matching vanilla behavior:
/// - Attacks players, villagers, iron golems, and turtle babies
/// - Alerts other zombies when hurt
/// - Burns in sunlight (TODO)
/// - Converts to drowned in water (TODO)
pub struct ZombieBehaviour {
    /// Goal selector for behavior goals
    goal_selector: SyncMutex<GoalSelector>,
    /// Target selector for target goals
    target_selector: SyncMutex<GoalSelector>,
    /// AI state
    ai_state: SyncMutex<AiState>,
    /// Random number generator
    random: SyncMutex<rand::rngs::StdRng>,
}

impl ZombieBehaviour {
    /// Creates a new zombie behaviour with registered goals
    #[must_use]
    pub fn new() -> Self {
        let mut goal_selector = GoalSelector::new();
        let mut target_selector = GoalSelector::new();

        // Register behavior goals (same as vanilla Zombie.addBehaviourGoals())
        // Priority 2: Attack goal
        goal_selector.add_goal(2, Box::new(MeleeAttackGoal::new(1.0, false)));

        // Priority 6: Move through village (TODO: implement this goal)
        // goal_selector.add_goal(6, Box::new(MoveThroughVillageGoal::new(1.0, true, 4)));

        // Priority 7: Water avoiding random stroll
        goal_selector.add_goal(7, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));

        // Priority 8: Look at player
        goal_selector.add_goal(8, Box::new(LookAtPlayerGoal::new(8.0)));

        // Priority 8: Random look around
        goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::new()));

        // Register target goals
        // Priority 1: Hurt by target (alerts other zombies except zombified piglins)
        target_selector.add_goal(
            1,
            Box::new(
                HurtByTargetGoal::new()
                    .alert_others()
                    .exclude_from_alert(vec!["minecraft:zombified_piglin"]),
            ),
        );

        // Priority 2: Target players
        target_selector.add_goal(
            2,
            Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
        );

        // Priority 3: Target villagers
        target_selector.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(TargetType::Villager, false)),
        );

        // Priority 3: Target iron golems
        target_selector.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(TargetType::IronGolem, true)),
        );

        // Priority 5: Target baby turtles on land
        target_selector.add_goal(
            5,
            Box::new(NearestAttackableTargetGoal::with_interval(
                TargetType::Turtle,
                10,
                true,
            )),
        );

        Self {
            goal_selector: SyncMutex::new(goal_selector),
            target_selector: SyncMutex::new(target_selector),
            ai_state: SyncMutex::new(AiState::default()),
            random: SyncMutex::new(rand::rngs::StdRng::from_rng(&mut rand::rng())),
        }
    }
}

impl Default for ZombieBehaviour {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityBehaviour for ZombieBehaviour {
    fn define_entity_data(&self, data: &mut EntityData) {
        use crate::entity::EntityDataAccessor;

        // Zombie-specific entity data
        // Index 16: Is baby (bool)
        data.define(EntityDataAccessor::ZOMBIE_IS_BABY, false);
        // Index 17: Special type (int) - unused in modern versions
        data.define(EntityDataAccessor::ZOMBIE_SPECIAL_TYPE, 0i32);
        // Index 18: Is converting to drowned (bool)
        data.define(EntityDataAccessor::ZOMBIE_DROWNED_CONVERSION, false);
    }

    fn read_nbt(&self, data: &mut EntityData, nbt: &NbtCompound) {
        use crate::entity::EntityDataAccessor;
        use super::nbt_bool;

        // Read IsBaby
        if let Some(v) = nbt.get("IsBaby").and_then(nbt_bool) {
            data.set(EntityDataAccessor::ZOMBIE_IS_BABY, v);
        }

        // Read DrownedConversionTime (presence indicates converting)
        if nbt.get("DrownedConversionTime").is_some() {
            data.set(EntityDataAccessor::ZOMBIE_DROWNED_CONVERSION, true);
        }
    }

    fn write_nbt(&self, data: &EntityData, nbt: &mut NbtCompound) {
        use crate::entity::EntityDataAccessor;
        use simdnbt::owned::NbtTag;

        let is_baby: bool = data.get(EntityDataAccessor::ZOMBIE_IS_BABY);
        nbt.insert("IsBaby", NbtTag::Byte(i8::from(is_baby)));
    }

    fn tick(&self, ctx: &mut EntityTickContext<'_>) {
        let mut goal_selector = self.goal_selector.lock();
        let mut target_selector = self.target_selector.lock();
        let mut ai_state = self.ai_state.lock();
        let mut random = self.random.lock();

        // Build target info if we have a target
        let target_info = ai_state.target_entity_id.map(|target_id| {
            // TODO: Get actual target position from world
            // For now, create a placeholder
            TargetInfo {
                entity_id: target_id,
                position: *ctx.position, // Placeholder
                uuid: uuid::Uuid::nil(),
                distance_squared: 0.0,
            }
        });

        // Create goal context
        let mut goal_ctx = GoalContext::new(
            ctx.entity_id,
            *ctx.position,
            *ctx.rotation,
            ctx.tick,
            &mut random,
            &mut ai_state,
            ctx.entity_tracker,
            ctx.chunk_map,
        )
        .with_target(target_info.clone());

        // Tick target selector first (to find targets)
        target_selector.tick(&mut goal_ctx);

        // Update target info after target selector tick - get actual position from tracker
        let target_info = ai_state.target_entity_id.and_then(|target_id| {
            ctx.entity_tracker.get_entity(target_id).map(|tracked| {
                let target_pos = tracked.entity.position();
                let dx = ctx.position.x - target_pos.x;
                let dy = ctx.position.y - target_pos.y;
                let dz = ctx.position.z - target_pos.z;
                TargetInfo {
                    entity_id: target_id,
                    position: target_pos,
                    uuid: tracked.entity.uuid(),
                    distance_squared: dx * dx + dy * dy + dz * dz,
                }
            })
        });

        // Create new context with updated target
        let mut goal_ctx = GoalContext::new(
            ctx.entity_id,
            *ctx.position,
            *ctx.rotation,
            ctx.tick,
            &mut random,
            &mut ai_state,
            ctx.entity_tracker,
            ctx.chunk_map,
        )
        .with_target(target_info);

        // Tick goal selector (to execute behaviors)
        goal_selector.tick(&mut goal_ctx);

        // Apply AI state changes back to entity
        // Update look rotation if there's a look target
        if let Some(look_target) = ai_state.look_target {
            let dx = look_target.x - ctx.position.x;
            let dy = look_target.y - ctx.position.y - 1.62; // Eye height
            let dz = look_target.z - ctx.position.z;

            let horizontal_dist = (dx * dx + dz * dz).sqrt();
            let yaw = (-dx).atan2(dz).to_degrees() as f32;
            let pitch = (-dy).atan2(horizontal_dist).to_degrees() as f32;

            ctx.rotation.0 = yaw;
            ctx.rotation.1 = pitch.clamp(-89.0, 89.0);
        }

        // Apply movement from path if available
        if let Some(path) = &ai_state.current_path {
            if let Some(next_pos) = path.next_position() {
                let speed = ai_state.speed_modifier * 0.1; // Base zombie speed (0.23 attribute * ~0.43 factor)

                let dx = next_pos.x - ctx.position.x;
                let dz = next_pos.z - ctx.position.z;
                let dist = (dx * dx + dz * dz).sqrt();

                if dist > 0.01 {
                    ctx.velocity.x = dx / dist * speed;
                    ctx.velocity.z = dz / dist * speed;
                }
            }
        }
    }

    fn has_tick(&self) -> bool {
        true
    }
}

/// Static instance for the behaviour registry
pub static ZOMBIE_BEHAVIOUR: std::sync::LazyLock<ZombieBehaviour> =
    std::sync::LazyLock::new(ZombieBehaviour::new);
