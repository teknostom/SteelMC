use super::super::instrumentation::OreFeatureProfile;
use super::super::prelude::*;
use super::super::runner::FeatureDecorationRunner;
use smallvec::SmallVec;
use std::f32::consts::PI;
use std::time::Instant;
use steel_math::trig;
use steel_registry::feature::{ResolvedOreTarget, ResolvedOreTargets};
use steel_utils::PackedSectionBlockPos;

impl FeatureDecorationRunner {
    pub(in crate::worldgen::feature) fn place_ore_feature(
        region: &mut WorldGenRegion<'_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        origin: BlockPos,
    ) -> bool {
        if config.size <= 0 {
            return false;
        }
        let direction = random.next_f32() * PI;
        let spread_xz = config.size as f32 / 8.0;
        let spread_xz_ceil = spread_xz.ceil() as i32;
        let max_radius = f32::midpoint(config.size as f32 / 16.0 * 2.0, 1.0).ceil() as i32;
        let sin = f64::from(direction).sin();
        let cos = f64::from(direction).cos();
        let x0 = f64::from(origin.x()) + sin * f64::from(spread_xz);
        let x1 = f64::from(origin.x()) - sin * f64::from(spread_xz);
        let z0 = f64::from(origin.z()) + cos * f64::from(spread_xz);
        let z1 = f64::from(origin.z()) - cos * f64::from(spread_xz);
        let y0 = f64::from(origin.y() + random.next_i32_bounded(3) - 2);
        let y1 = f64::from(origin.y() + random.next_i32_bounded(3) - 2);
        let x_start = origin.x() - spread_xz_ceil - max_radius;
        let y_start = origin.y() - 2 - max_radius;
        let z_start = origin.z() - spread_xz_ceil - max_radius;
        let size_xz = 2 * (spread_xz_ceil + max_radius);
        let size_y = 2 * (2 + max_radius);

        if !region.any_height_at_least(
            HeightmapType::OceanFloorWg,
            (x_start, z_start),
            (x_start + size_xz, z_start + size_xz),
            y_start,
        ) {
            return false;
        }

        Self::do_place_ore(
            region, registry, random, config, x0, x1, z0, z1, y0, y1, x_start, y_start, z_start,
            size_xz, size_y,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "mirrors vanilla ore vein placement inputs"
    )]
    pub(in crate::worldgen::feature) fn do_place_ore(
        region: &mut WorldGenRegion<'_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        x0: f64,
        x1: f64,
        z0: f64,
        z1: f64,
        y0: f64,
        y1: f64,
        x_start: i32,
        y_start: i32,
        z_start: i32,
        size_xz: i32,
        size_y: i32,
    ) -> bool {
        let Ok(size) = usize::try_from(config.size) else {
            return false;
        };
        let mut vein_nodes = SmallVec::<[[f64; 4]; 32]>::from_elem([0.0; 4], size);

        for i in 0..size {
            let step = i as f32 / config.size as f32;
            let size_factor = random.next_f64() * f64::from(config.size) / 16.0;
            let radius_wave = trig::sin(PI * step) + 1.0;
            let radius = f64::from(radius_wave) * size_factor + 1.0;
            vein_nodes[i] = [
                lerp(f64::from(step), x0, x1),
                lerp(f64::from(step), y0, y1),
                lerp(f64::from(step), z0, z1),
                radius / 2.0,
            ];
        }

        for i1 in 0..size.saturating_sub(1) {
            if vein_nodes[i1][3] <= 0.0 {
                continue;
            }

            for i2 in i1 + 1..size {
                if vein_nodes[i2][3] <= 0.0 {
                    continue;
                }

                let dx = vein_nodes[i1][0] - vein_nodes[i2][0];
                let dy = vein_nodes[i1][1] - vein_nodes[i2][1];
                let dz = vein_nodes[i1][2] - vein_nodes[i2][2];
                let dr = vein_nodes[i1][3] - vein_nodes[i2][3];
                if dr * dr > dx * dx + dy * dy + dz * dz {
                    if dr > 0.0 {
                        vein_nodes[i2][3] = -1.0;
                    } else {
                        vein_nodes[i1][3] = -1.0;
                    }
                }
            }
        }

        let Some(search_volume) = OreSearchVolume::new(size_xz, size_y) else {
            return false;
        };
        let profile = OreFeatureProfile::new(config.size);
        let mut placed = 0_u64;
        let mut tested = OreTestedPositions::with_capacity(search_volume.tested_position_count);
        let targets = config.resolved_targets(registry);
        let batch_no_air_exposure = config.discard_chance_on_air_exposure <= 0.0;
        let mut pending_no_air_sections = SmallVec::<[PendingOreSection; 8]>::new();
        let min_y = region.min_y();
        let height = region.height();

        {
            let mut sections = region.bulk_section_access_for_ore(profile.stats());
            let candidate_started_at = profile.stats().map(|_| Instant::now());

            placed += if profile.stats().is_some() {
                Self::collect_ore_candidates::<true>(
                    &mut sections,
                    registry,
                    random,
                    config,
                    targets,
                    search_volume,
                    &vein_nodes,
                    x_start,
                    y_start,
                    z_start,
                    min_y,
                    height,
                    batch_no_air_exposure,
                    &mut tested,
                    &mut pending_no_air_sections,
                )
            } else {
                Self::collect_ore_candidates::<false>(
                    &mut sections,
                    registry,
                    random,
                    config,
                    targets,
                    search_volume,
                    &vein_nodes,
                    x_start,
                    y_start,
                    z_start,
                    min_y,
                    height,
                    batch_no_air_exposure,
                    &mut tested,
                    &mut pending_no_air_sections,
                )
            };
            if let Some(started_at) = candidate_started_at
                && let Some(stats) = profile.stats()
            {
                stats
                    .borrow_mut()
                    .record_candidate_time(started_at.elapsed());
            }

            if batch_no_air_exposure {
                let batch_apply_started_at = profile.stats().map(|_| Instant::now());
                for pending_section in &pending_no_air_sections {
                    placed += sections.replace_ore_target_block_states_in_section(
                        pending_section.key.chunk_x,
                        pending_section.key.chunk_z,
                        pending_section.key.section_index,
                        &pending_section.positions,
                        |block_state, y| targets.matching_replacement(registry, block_state, y),
                    );
                }
                if let Some(started_at) = batch_apply_started_at
                    && let Some(stats) = profile.stats()
                {
                    stats
                        .borrow_mut()
                        .record_batch_apply_time(started_at.elapsed());
                }
            }
        }

        profile.finish(placed);
        placed > 0
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "keeps the vanilla ore candidate loop monomorphized without hiding state"
    )]
    fn collect_ore_candidates<const PROFILE: bool>(
        sections: &mut WorldGenBulkSectionAccess<'_, '_, '_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        targets: &ResolvedOreTargets,
        search_volume: OreSearchVolume,
        vein_nodes: &[[f64; 4]],
        x_start: i32,
        y_start: i32,
        z_start: i32,
        min_y: i32,
        height: i32,
        batch_no_air_exposure: bool,
        tested: &mut OreTestedPositions,
        pending_no_air_sections: &mut SmallVec<[PendingOreSection; 8]>,
    ) -> u64 {
        let mut placed = 0_u64;

        for &node in vein_nodes {
            let radius = node[3];
            if radius < 0.0 {
                continue;
            }

            let x_min = fast_floor(node[0] - radius).max(x_start);
            let z_min = fast_floor(node[2] - radius).max(z_start);
            let x_max = fast_floor(node[0] + radius).max(x_min);
            let z_max = fast_floor(node[2] + radius).max(z_min);
            let raw_y_min = fast_floor(node[1] - radius).max(y_start);
            let raw_y_max = fast_floor(node[1] + radius).max(raw_y_min);
            let y_min = raw_y_min.max(min_y);
            let y_max = raw_y_max.min(min_y + height - 1);
            if y_min > y_max {
                continue;
            }

            // Same per-axis expressions as vanilla's inner loops, evaluated once
            // per node instead of once per (x, y, z); bit-identical.
            let axis_distances_squared = |min: i32, max: i32, center: f64| {
                (min..=max)
                    .map(|coord| {
                        let distance = (f64::from(coord) + 0.5 - center) / radius;
                        distance * distance
                    })
                    .collect::<SmallVec<[f64; 32]>>()
            };
            let y_distances_squared = axis_distances_squared(y_min, y_max, node[1]);
            let z_distances_squared = axis_distances_squared(z_min, z_max, node[2]);

            // Mins are clamped to the search-volume start, so offsets are non-negative.
            let z_first_index = (z_min - z_start) as usize * search_volume.z_stride();
            for x in x_min..=x_max {
                let x_index = (x - x_start) as usize;
                let x_distance = (f64::from(x) + 0.5 - node[0]) / radius;
                let x_distance_squared = x_distance * x_distance;
                if x_distance_squared >= 1.0 {
                    continue;
                }

                for (y, &y_distance_squared) in (y_min..=y_max).zip(&y_distances_squared) {
                    let x_y_index =
                        x_index + (y - y_start) as usize * search_volume.y_stride() + z_first_index;
                    let x_y_distance_squared = x_distance_squared + y_distance_squared;
                    if x_y_distance_squared >= 1.0 {
                        continue;
                    }

                    for (z_step, &z_distance_squared) in z_distances_squared.iter().enumerate() {
                        if x_y_distance_squared + z_distance_squared >= 1.0 {
                            continue;
                        }

                        if PROFILE {
                            sections.record_ore_candidate_position();
                        }
                        let z = z_min + z_step as i32;
                        let tested_index = x_y_index + z_step * search_volume.z_stride();
                        if tested.insert(tested_index) {
                            if PROFILE {
                                sections.record_ore_unique_position();
                            }
                            if batch_no_air_exposure {
                                let section_key =
                                    PendingOreSectionKey::from_in_height_coords(min_y, x, y, z);
                                let Some(pos) = PackedSectionBlockPos::from_local_xyz(
                                    (x & 15) as u8,
                                    (y & 15) as u8,
                                    (z & 15) as u8,
                                ) else {
                                    panic!("masked ore section-local position escaped section");
                                };
                                push_pending_ore_position(
                                    pending_no_air_sections,
                                    section_key,
                                    pos,
                                );
                            } else {
                                let pos = BlockPos::new(x, y, z);
                                if sections.can_write_to_pos(pos) {
                                    if PROFILE {
                                        sections.record_ore_write_allowed_position();
                                    }
                                    if Self::try_place_ore_block_in_bulk(
                                        sections, registry, random, config, targets, pos,
                                    ) {
                                        placed += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        placed
    }

    pub(in crate::worldgen::feature) fn place_scattered_ore_feature(
        region: &mut WorldGenRegion<'_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        origin: BlockPos,
    ) -> bool {
        assert!(
            config.size >= 0,
            "scattered ore size {} is negative",
            config.size
        );

        let targets = config.resolved_targets(registry);
        let tries = random.next_i32_bounded(config.size + 1);
        for i in 0..tries {
            let max_distance = i.min(7);
            let pos = origin.offset(
                Self::random_scattered_ore_offset(random, max_distance),
                Self::random_scattered_ore_offset(random, max_distance),
                Self::random_scattered_ore_offset(random, max_distance),
            );
            let _ =
                Self::try_place_resolved_ore_block(region, registry, random, config, targets, pos);
        }

        true
    }

    pub(in crate::worldgen::feature) fn random_scattered_ore_offset(
        random: &mut WorldgenRandom,
        max_distance: i32,
    ) -> i32 {
        Self::java_round_f32((random.next_f32() - random.next_f32()) * max_distance as f32)
    }

    pub(in crate::worldgen::feature) fn java_round_f32(value: f32) -> i32 {
        (value + 0.5).floor() as i32
    }

    fn try_place_resolved_ore_block(
        region: &mut WorldGenRegion<'_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        targets: &ResolvedOreTargets,
        pos: BlockPos,
    ) -> bool {
        let block_state = region.block_state(pos);
        let block_id = ResolvedOreTargets::block_id_for_state(registry, block_state);
        for target in targets.iter() {
            if Self::can_place_resolved_ore(region, registry, random, config, target, pos, block_id)
            {
                return region.set_block_state(pos, target.state, UpdateFlags::UPDATE_CLIENTS);
            }
        }

        false
    }

    fn try_place_ore_block_in_bulk(
        sections: &mut WorldGenBulkSectionAccess<'_, '_, '_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        targets: &ResolvedOreTargets,
        pos: BlockPos,
    ) -> bool {
        if config.discard_chance_on_air_exposure <= 0.0 {
            return sections.replace_ore_target_block_state(pos, |block_state| {
                targets.matching_replacement(registry, block_state, pos.y())
            });
        }

        let block_state = sections.ore_target_block_state(pos);
        let block_id = ResolvedOreTargets::block_id_for_state(registry, block_state);
        for target in targets.iter() {
            if Self::can_place_resolved_ore_in_bulk(
                sections, registry, random, config, target, pos, block_id,
            ) {
                return sections.set_block_state(pos, target.state);
            }
        }

        false
    }

    fn can_place_resolved_ore(
        region: &WorldGenRegion<'_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        target: &ResolvedOreTarget,
        pos: BlockPos,
        block_id: usize,
    ) -> bool {
        if !target.matches(block_id, pos.y()) {
            return false;
        }

        if Self::should_skip_air_check(random, config.discard_chance_on_air_exposure) {
            return true;
        }

        !Self::is_adjacent_to_air(region, registry, pos)
    }

    fn can_place_resolved_ore_in_bulk(
        sections: &mut WorldGenBulkSectionAccess<'_, '_, '_>,
        registry: &Registry,
        random: &mut WorldgenRandom,
        config: &OreConfiguration,
        target: &ResolvedOreTarget,
        pos: BlockPos,
        block_id: usize,
    ) -> bool {
        if !target.matches(block_id, pos.y()) {
            return false;
        }

        if Self::should_skip_air_check(random, config.discard_chance_on_air_exposure) {
            return true;
        }

        !Self::is_adjacent_to_air_in_bulk(sections, registry, pos)
    }

    pub(in crate::worldgen::feature) fn should_skip_air_check(
        random: &mut WorldgenRandom,
        discard_chance_on_air_exposure: f32,
    ) -> bool {
        if discard_chance_on_air_exposure <= 0.0 {
            true
        } else if discard_chance_on_air_exposure >= 1.0 {
            false
        } else {
            random.next_f32() >= discard_chance_on_air_exposure
        }
    }

    pub(in crate::worldgen::feature) fn is_adjacent_to_air(
        region: &WorldGenRegion<'_>,
        registry: &Registry,
        pos: BlockPos,
    ) -> bool {
        Direction::ALL.into_iter().any(|direction| {
            let neighbor = region.block_state(pos.relative(direction));
            Self::is_air_block_state(registry, neighbor)
        })
    }

    pub(in crate::worldgen::feature) fn is_adjacent_to_air_in_bulk(
        sections: &mut WorldGenBulkSectionAccess<'_, '_, '_>,
        registry: &Registry,
        pos: BlockPos,
    ) -> bool {
        Direction::ALL.into_iter().any(|direction| {
            let neighbor = sections.ore_neighbor_block_state(pos.relative(direction));
            Self::is_air_block_state(registry, neighbor)
        })
    }

    pub(in crate::worldgen::feature) fn is_air_block_state(
        registry: &Registry,
        state: BlockStateId,
    ) -> bool {
        let Some(block) = registry.blocks.by_state_id(state) else {
            panic!("feature received invalid block state id {}", state.0);
        };
        block.config.is_air
    }
}

/// Bitset of tested positions. The inline capacity covers the largest vanilla
/// ore volume (size 64: 26 * 14 * 26 bits), so no ore placement allocates.
struct OreTestedPositions {
    words: SmallVec<[u64; 160]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PendingOreSectionKey {
    chunk_x: i32,
    chunk_z: i32,
    section_index: usize,
}

struct PendingOreSection {
    key: PendingOreSectionKey,
    positions: SmallVec<[PackedSectionBlockPos; 256]>,
}

#[derive(Clone, Copy)]
struct OreSearchVolume {
    size_xz: usize,
    size_xz_y: usize,
    tested_position_count: usize,
}

impl OreSearchVolume {
    fn new(size_xz: i32, size_y: i32) -> Option<Self> {
        let size_xz = usize::try_from(size_xz).ok().filter(|&size| size > 0)?;
        let size_y = usize::try_from(size_y).ok().filter(|&size| size > 0)?;
        let size_xz_y = size_xz.checked_mul(size_y)?;
        let tested_position_count = size_xz_y.checked_mul(size_xz)?;
        Some(Self {
            size_xz,
            size_xz_y,
            tested_position_count,
        })
    }

    // Vanilla OreFeature's BitSet index is `x + y * size_xz + z * size_xz * size_y`
    // over offsets from the volume start.
    const fn y_stride(self) -> usize {
        self.size_xz
    }

    const fn z_stride(self) -> usize {
        self.size_xz_y
    }

    #[cfg(test)]
    const fn index_from_offsets(self, x_offset: usize, y_offset: usize, z_offset: usize) -> usize {
        x_offset + y_offset * self.y_stride() + z_offset * self.z_stride()
    }
}

impl PendingOreSectionKey {
    const fn from_in_height_coords(min_y: i32, x: i32, y: i32, z: i32) -> Self {
        Self {
            chunk_x: SectionPos::block_to_section_coord(x),
            chunk_z: SectionPos::block_to_section_coord(z),
            section_index: ((y - min_y) / 16) as usize,
        }
    }
}

fn push_pending_ore_position(
    sections: &mut SmallVec<[PendingOreSection; 8]>,
    key: PendingOreSectionKey,
    pos: PackedSectionBlockPos,
) {
    if let Some(section) = sections.last_mut()
        && section.key == key
    {
        section.positions.push(pos);
        return;
    }

    if let Some(section) = sections.iter_mut().find(|section| section.key == key) {
        section.positions.push(pos);
        return;
    }

    sections.push(PendingOreSection {
        key,
        positions: smallvec::smallvec![pos],
    });
}

impl OreTestedPositions {
    fn with_capacity(bit_count: usize) -> Self {
        Self {
            words: smallvec::smallvec![0; bit_count.div_ceil(u64::BITS as usize)],
        }
    }

    #[inline]
    fn insert(&mut self, index: usize) -> bool {
        let word_index = index / u64::BITS as usize;
        if word_index >= self.words.len() {
            self.grow(word_index);
        }

        let mask = 1_u64 << (index % u64::BITS as usize);
        let word = &mut self.words[word_index];
        if *word & mask != 0 {
            return false;
        }

        *word |= mask;
        true
    }

    /// Vanilla's `BitSet` grows for the inclusive far edges of the volume.
    #[cold]
    fn grow(&mut self, word_index: usize) {
        self.words.resize(word_index + 1, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::{OreSearchVolume, OreTestedPositions};

    #[test]
    fn ore_tested_position_index_matches_vanilla_layout() {
        let volume = OreSearchVolume::new(4, 6);
        assert_eq!(
            volume.map(|volume| volume.index_from_offsets(2, 3, 1)),
            Some(38)
        );
    }

    #[test]
    fn ore_tested_position_index_keeps_vanilla_inclusive_edge_layout() {
        let volume = OreSearchVolume::new(4, 6);
        assert_eq!(
            volume.map(|volume| volume.index_from_offsets(4, 0, 0)),
            Some(4)
        );
        assert_eq!(
            volume.map(|volume| volume.index_from_offsets(0, 1, 0)),
            Some(4)
        );
    }

    #[test]
    fn ore_tested_positions_deduplicate_and_grow() {
        let mut tested = OreTestedPositions::with_capacity(1);
        assert!(tested.insert(0));
        assert!(!tested.insert(0));
        assert!(tested.insert(130));
        assert!(!tested.insert(130));
    }
}
