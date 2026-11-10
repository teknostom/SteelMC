//! Ore targets resolved once per configuration.

use smallvec::SmallVec;
use steel_utils::BlockStateId;

use super::OreConfiguration;
use crate::structure::processor::data::StructureRuleTestData;
use crate::{Registry, RegistryEntry, RegistryExt, TaggedRegistryExt};

/// An ore configuration's targets, with tags and blocks resolved to block ids.
#[derive(Debug, Clone)]
pub struct ResolvedOreTargets {
    targets: SmallVec<[ResolvedOreTarget; 2]>,
}

/// One resolved `OreConfiguration.TargetBlockState`.
#[derive(Debug, Clone)]
pub struct ResolvedOreTarget {
    matcher: ResolvedOreRuleTest,
    /// Per-block-id match result, present when the rule does not depend on Y.
    block_mask: Option<Box<[u64]>>,
    /// Ore state placed when the target matches.
    pub state: BlockStateId,
}

#[derive(Debug, Clone)]
enum ResolvedOreRuleTest {
    AlwaysTrue,
    Block(usize),
    Tag(SmallVec<[usize; 8]>),
    HeightRange(i32, i32),
    Not(Box<Self>),
    AllOf(Vec<Self>),
    AnyOf(Vec<Self>),
}

impl OreConfiguration {
    /// Returns the resolved targets, resolving them on first use.
    ///
    /// # Panics
    ///
    /// Panics if a target references an unknown block or state, or uses a rule
    /// test ore placement does not support.
    pub fn resolved_targets(&self, registry: &Registry) -> &ResolvedOreTargets {
        self.resolved_targets
            .get_or_init(|| ResolvedOreTargets::from_config(registry, self))
    }
}

impl ResolvedOreTargets {
    fn from_config(registry: &Registry, config: &OreConfiguration) -> Self {
        let block_count = registry.blocks.block_to_base_state.len();
        let targets = config
            .targets
            .iter()
            .map(|target| {
                let matcher = ResolvedOreRuleTest::resolve(registry, &target.target);
                let Some(state) = registry.blocks.state_id_from_block_defaulted_properties(
                    target.state.block,
                    target.state.properties.iter().copied(),
                ) else {
                    panic!(
                        "ore feature references unknown or invalid state {}",
                        target.state.block.key
                    );
                };
                let block_mask = (!matcher.depends_on_y()).then(|| {
                    let mut mask = vec![0_u64; block_count.div_ceil(64)].into_boxed_slice();
                    for block_id in 0..block_count {
                        if matcher.matches(block_id, 0) {
                            mask[block_id / 64] |= 1 << (block_id % 64);
                        }
                    }
                    mask
                });
                ResolvedOreTarget {
                    matcher,
                    block_mask,
                    state,
                }
            })
            .collect();

        Self { targets }
    }

    /// The targets in configuration order.
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedOreTarget> {
        self.targets.iter()
    }

    /// The ore state of the first target matching `state` at `y`.
    #[inline]
    #[must_use]
    pub fn matching_replacement(
        &self,
        registry: &Registry,
        state: BlockStateId,
        y: i32,
    ) -> Option<BlockStateId> {
        let block_id = Self::block_id_for_state(registry, state);
        self.targets
            .iter()
            .find_map(|target| target.matches(block_id, y).then_some(target.state))
    }

    /// The block id owning `state`.
    ///
    /// # Panics
    ///
    /// Panics if `state` is not a registered block state.
    #[inline]
    #[must_use]
    pub fn block_id_for_state(registry: &Registry, state: BlockStateId) -> usize {
        let Some(&block_id) = registry.blocks.state_to_block_id.get(state.0 as usize) else {
            panic!("ore feature received invalid block state id {}", state.0);
        };
        block_id
    }
}

impl ResolvedOreTarget {
    /// Whether a block with `block_id` at `y` is replaced by this target.
    #[inline]
    #[must_use]
    pub fn matches(&self, block_id: usize, y: i32) -> bool {
        match &self.block_mask {
            Some(mask) => mask
                .get(block_id / 64)
                .is_some_and(|word| word & (1 << (block_id % 64)) != 0),
            None => self.matcher.matches(block_id, y),
        }
    }
}

impl ResolvedOreRuleTest {
    fn resolve(registry: &Registry, test: &StructureRuleTestData) -> Self {
        match test {
            StructureRuleTestData::AlwaysTrue => Self::AlwaysTrue,
            StructureRuleTestData::BlockMatch { block } => {
                let block_ref = registry
                    .blocks
                    .by_key(block)
                    .unwrap_or_else(|| panic!("unknown ore target block {block}"));
                Self::Block(block_ref.id())
            }
            StructureRuleTestData::TagMatch { tag } => Self::Tag(
                registry
                    .blocks
                    .iter_tag(tag)
                    .map(RegistryEntry::id)
                    .collect(),
            ),
            StructureRuleTestData::HeightMatch {
                min_inclusive,
                max_inclusive,
            } => Self::HeightRange(*min_inclusive, *max_inclusive),
            StructureRuleTestData::Not { rule } => {
                Self::Not(Box::new(Self::resolve(registry, rule)))
            }
            StructureRuleTestData::AllOf { rules } => Self::AllOf(
                rules
                    .iter()
                    .map(|rule| Self::resolve(registry, rule))
                    .collect(),
            ),
            StructureRuleTestData::AnyOf { rules } => Self::AnyOf(
                rules
                    .iter()
                    .map(|rule| Self::resolve(registry, rule))
                    .collect(),
            ),
            other => unreachable!("ore rule test target does not support {other:?}"),
        }
    }

    fn depends_on_y(&self) -> bool {
        match self {
            Self::AlwaysTrue | Self::Block(_) | Self::Tag(_) => false,
            Self::HeightRange(..) => true,
            Self::Not(rule) => rule.depends_on_y(),
            Self::AllOf(rules) | Self::AnyOf(rules) => rules.iter().any(Self::depends_on_y),
        }
    }

    fn matches(&self, block_id: usize, y: i32) -> bool {
        match self {
            Self::AlwaysTrue => true,
            Self::Block(target_block_id) => block_id == *target_block_id,
            Self::Tag(block_ids) => block_ids.contains(&block_id),
            Self::HeightRange(min_inclusive, max_inclusive) => {
                (*min_inclusive..=*max_inclusive).contains(&y)
            }
            Self::Not(rule) => !rule.matches(block_id, y),
            Self::AllOf(rules) => rules.iter().all(|rule| rule.matches(block_id, y)),
            Self::AnyOf(rules) => rules.iter().any(|rule| rule.matches(block_id, y)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ResolvedOreRuleTest;

    /// The block mask is evaluated at a fixed Y, so a nested height rule must
    /// disable it.
    #[test]
    fn nested_height_rule_depends_on_y() {
        let rule = ResolvedOreRuleTest::AllOf(vec![
            ResolvedOreRuleTest::Block(1),
            ResolvedOreRuleTest::Not(Box::new(ResolvedOreRuleTest::HeightRange(0, 8))),
        ]);
        assert!(rule.depends_on_y());
        assert!(!ResolvedOreRuleTest::AnyOf(vec![ResolvedOreRuleTest::AlwaysTrue]).depends_on_y());
    }
}
