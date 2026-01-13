//! Target selection goals for mob AI.
//!
//! Target goals are responsible for finding and tracking targets for mobs to attack.

mod hurt_by;
mod nearest_attackable;
mod target_goal;

pub use hurt_by::HurtByTargetGoal;
pub use nearest_attackable::{NearestAttackableTargetGoal, TargetType};
pub use target_goal::TargetGoal;
