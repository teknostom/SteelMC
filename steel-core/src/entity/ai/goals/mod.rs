//! Goal implementations for mob AI.
//!
//! This module contains concrete goal implementations used by various mobs.

mod look_at_player;
mod melee_attack;
mod random_look_around;
mod random_stroll;

pub use look_at_player::LookAtPlayerGoal;
pub use melee_attack::MeleeAttackGoal;
pub use random_look_around::RandomLookAroundGoal;
pub use random_stroll::WaterAvoidingRandomStrollGoal;
