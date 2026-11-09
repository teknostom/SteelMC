//! Beardifier: terrain density modification around structure pieces.
//!
//! Matches vanilla's `Beardifier` class. Modifies terrain density at cell corners
//! using a gaussian kernel falloff around rigid structure pieces and jigsaw junctions.
//! This creates the terrain adaptation effects like carving out space for villages
//! or burying ancient cities.

use std::ops::Range;
use std::sync::LazyLock;

use glam::IVec3;
use steel_math::map_clamped;
use steel_registry::structure::TerrainAdjustment;
use steel_registry::template_pool::Projection;
use steel_utils::BoundingBox;

use crate::structure::StructureStart;
use crate::structure::jigsaw::JigsawJunction;

/// A rigid structure piece that modifies terrain density.
#[derive(Debug)]
struct Rigid {
    bounding_box: BoundingBox,
    terrain_adjustment: TerrainAdjustment,
    ground_level_delta: i32,
}

const KERNEL_RADIUS: i32 = 12;
const KERNEL_SIZE: usize = 24;
const KERNEL_TOTAL: usize = KERNEL_SIZE * KERNEL_SIZE * KERNEL_SIZE; // 13824
const BURY_RADIUS: f32 = 6.0;

/// Pre-computed gaussian beard kernel.
/// Layout: `[z][x][y]` where indices go from 0..24, representing offsets -12..+11.
static BEARD_KERNEL: LazyLock<[f32; KERNEL_TOTAL]> = LazyLock::new(|| {
    let mut kernel = [0.0f32; KERNEL_TOTAL];
    for zi in 0..KERNEL_SIZE {
        let dz = zi as i32 - KERNEL_RADIUS;
        for xi in 0..KERNEL_SIZE {
            let dx = xi as i32 - KERNEL_RADIUS;
            for yi in 0..KERNEL_SIZE {
                let dy = yi as i32 - KERNEL_RADIUS;
                // dy + 0.5 matches vanilla's computeBeardContribution(int, int, int)
                let dy_f = f64::from(dy) + 0.5;
                let dist_sq = f64::from(dx * dx) + dy_f * dy_f + f64::from(dz * dz);
                let value = (-dist_sq / 16.0).exp();
                kernel[zi * KERNEL_SIZE * KERNEL_SIZE + xi * KERNEL_SIZE + yi] = value as f32;
            }
        }
    }
    kernel
});

/// Vanilla's `Mth.fastInvSqrt` — the Quake III fast inverse square root, ported exactly.
#[inline]
fn fast_inv_sqrt(x: f64) -> f64 {
    let xhalf = 0.5f64 * x;
    let i = f64::to_bits(x) as i64;
    let i = 0x5FE6_EB50_C7B5_37A9_i64 - (i >> 1);
    let mut x = f64::from_bits(i as u64);
    x *= 1.5f64 - xhalf * x * x;
    x
}

#[inline]
fn is_in_kernel_range(index: i32) -> bool {
    (0..KERNEL_SIZE as i32).contains(&index)
}

/// Computes the beard density contribution for a point near a structure piece.
///
/// `dx`, `dy`, `dz` are the distances from the query point to the piece for kernel lookup.
/// `y_to_ground` is the vertical distance from query point to the piece's ground level.
fn get_beard_contribution(dx: i32, dy: i32, dz: i32, y_to_ground: i32) -> f32 {
    let xi = dx + KERNEL_RADIUS;
    let yi = dy + KERNEL_RADIUS;
    let zi = dz + KERNEL_RADIUS;

    if !is_in_kernel_range(xi) || !is_in_kernel_range(yi) || !is_in_kernel_range(zi) {
        return 0.0;
    }

    let dy_with_offset = y_to_ground as f32 + 0.5;
    let dist_sq = (dx * dx) as f32 + dy_with_offset * dy_with_offset + (dz * dz) as f32;
    let value = -dy_with_offset * fast_inv_sqrt(f64::from(dist_sq / 2.0)) as f32 / 2.0;
    let kernel_idx =
        zi as usize * KERNEL_SIZE * KERNEL_SIZE + xi as usize * KERNEL_SIZE + yi as usize;
    value * BEARD_KERNEL[kernel_idx]
}

/// Computes the bury density contribution for a point near a structure piece.
///
/// Simple linear falloff: 1.0 at distance 0, 0.0 at distance 6.
fn get_bury_contribution(dx: f32, dy: f32, dz: f32) -> f32 {
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    map_clamped(distance, 0.0, BURY_RADIUS, 1.0, 0.0)
}

/// Horizontal reach of any piece or junction contribution: beard kernels are
/// `KERNEL_RADIUS` wide, bury falls to zero at `BURY_RADIUS` and encapsulate at
/// `2 * BURY_RADIUS` (= `KERNEL_RADIUS`).
const CONTRIBUTION_REACH: i32 = KERNEL_RADIUS;

/// Computes terrain density contributions from nearby structure pieces and junctions.
///
/// Built per-chunk from the chunk's own starts plus referenced neighbor starts (mirrors
/// vanilla's `StructureManager.startsForStructure`). Queried per-block by `NoiseChunk::fill`
/// after the outer density-function ops, matching vanilla's `cacheAllInCell(add(final_density,
/// beardifier))` integration.
pub struct Beardifier {
    rigids: Vec<Rigid>,
    junctions: Vec<JigsawJunction>,
    /// Union of all piece/junction bounding boxes inflated by 24.
    /// Points outside this box get 0.0 without iterating pieces.
    affected_box: Option<BoundingBox>,
    chunk_start_x: i32,
    chunk_start_z: i32,
    columns: BeardColumns,
}

/// Per-column (16x16) indices of the rigids and junctions within
/// [`CONTRIBUTION_REACH`] horizontally, in their original order.
///
/// Everything else contributes exactly `+0.0`, and the running sum is never
/// `-0.0`, so summing only these is bit-identical to vanilla's full loop.
struct BeardColumns {
    rigid_offsets: Box<[u32]>,
    rigid_indices: Vec<u32>,
    junction_offsets: Box<[u32]>,
    junction_indices: Vec<u32>,
}

impl BeardColumns {
    fn new(
        rigids: &[Rigid],
        junctions: &[JigsawJunction],
        chunk_start_x: i32,
        chunk_start_z: i32,
    ) -> Self {
        let mut rigid_offsets = Vec::with_capacity(257);
        let mut rigid_indices = Vec::new();
        let mut junction_offsets = Vec::with_capacity(257);
        let mut junction_indices = Vec::new();
        rigid_offsets.push(0);
        junction_offsets.push(0);
        let reach = -CONTRIBUTION_REACH..CONTRIBUTION_REACH;
        for local_z in 0..16 {
            let block_z = chunk_start_z + local_z;
            for local_x in 0..16 {
                let block_x = chunk_start_x + local_x;
                for (index, rigid) in (0u32..).zip(rigids) {
                    let (dx, dz) = rigid.horizontal_distance(block_x, block_z);
                    if dx < CONTRIBUTION_REACH && dz < CONTRIBUTION_REACH {
                        rigid_indices.push(index);
                    }
                }
                for (index, junction) in (0u32..).zip(junctions) {
                    if reach.contains(&(block_x - junction.source_pos.x))
                        && reach.contains(&(block_z - junction.source_pos.z))
                    {
                        junction_indices.push(index);
                    }
                }
                rigid_offsets.push(Self::offset(rigid_indices.len()));
                junction_offsets.push(Self::offset(junction_indices.len()));
            }
        }
        Self {
            rigid_offsets: rigid_offsets.into_boxed_slice(),
            rigid_indices,
            junction_offsets: junction_offsets.into_boxed_slice(),
            junction_indices,
        }
    }

    fn offset(len: usize) -> u32 {
        let Ok(offset) = u32::try_from(len) else {
            panic!("beardifier column index count {len} exceeds u32");
        };
        offset
    }

    const fn range(offsets: &[u32], column: usize) -> Range<usize> {
        offsets[column] as usize..offsets[column + 1] as usize
    }
}

impl Rigid {
    /// Horizontal distance to the bounding box, 0 when inside it.
    fn horizontal_distance(&self, block_x: i32, block_z: i32) -> (i32, i32) {
        let bb = &self.bounding_box;
        let dx = 0.max((bb.min_x() - block_x).max(block_x - bb.max_x()));
        let dz = 0.max((bb.min_z() - block_z).max(block_z - bb.max_z()));
        (dx, dz)
    }

    fn contribution(&self, block_x: i32, block_y: i32, block_z: i32) -> f32 {
        let bb = &self.bounding_box;
        let (dx, dz) = self.horizontal_distance(block_x, block_z);
        let ground_y = bb.min_y() + self.ground_level_delta;
        let dy_to_ground = block_y - ground_y;

        match self.terrain_adjustment {
            TerrainAdjustment::None => 0.0,
            TerrainAdjustment::Bury => {
                get_bury_contribution(dx as f32, dy_to_ground as f32 / 2.0, dz as f32)
            }
            TerrainAdjustment::BeardThin => {
                get_beard_contribution(dx, dy_to_ground, dz, dy_to_ground) * 0.8
            }
            TerrainAdjustment::BeardBox => {
                let dy = 0.max((ground_y - block_y).max(block_y - bb.max_y()));
                get_beard_contribution(dx, dy, dz, dy_to_ground) * 0.8
            }
            TerrainAdjustment::Encapsulate => {
                let dy = 0.max((bb.min_y() - block_y).max(block_y - bb.max_y()));
                get_bury_contribution(dx as f32 / 2.0, dy as f32 / 2.0, dz as f32 / 2.0) * 0.8
            }
        }
    }
}

fn junction_contribution(
    junction: &JigsawJunction,
    block_x: i32,
    block_y: i32,
    block_z: i32,
) -> f32 {
    let dx = block_x - junction.source_pos.x;
    let dy = block_y - junction.source_pos.y;
    let dz = block_z - junction.source_pos.z;
    get_beard_contribution(dx, dy, dz, dy) * 0.4
}

impl Beardifier {
    /// Collect rigid pieces and junctions from structure starts that affect this chunk.
    ///
    /// `starts` should yield every `StructureStart` whose pieces could affect this chunk —
    /// typically the chunk's own starts plus all referenced neighbor starts (vanilla collects
    /// these via `StructureManager.startsForStructure`).
    ///
    /// `chunk_x` and `chunk_z` are chunk coordinates (not block coordinates).
    ///
    /// Mirrors vanilla's `forStructuresInChunk`:
    /// - Non-jigsaw pieces (`projection: None`) → added as rigid with `ground_level_delta = 0`.
    /// - Jigsaw RIGID pieces → added as rigid with stored `ground_level_delta`, junctions collected.
    /// - Jigsaw `TERRAIN_MATCHING` pieces → not added as rigid; junctions still collected.
    #[must_use]
    pub fn for_structures_in_chunk<'a, I>(starts: I, chunk_x: i32, chunk_z: i32) -> Self
    where
        I: IntoIterator<Item = &'a StructureStart>,
    {
        let chunk_start_x = chunk_x * 16;
        let chunk_start_z = chunk_z * 16;

        let mut rigids = Vec::new();
        let mut junctions: Vec<JigsawJunction> = Vec::new();
        let mut encompassing: Option<BoundingBox> = None;

        for start in starts {
            let terrain_adj = start.terrain_adjustment;
            if terrain_adj == TerrainAdjustment::None {
                continue;
            }

            for piece in &start.pieces {
                let bb = &piece.bounding_box;

                // Vanilla: piece.isCloseToChunk(chunkPos, 12)
                if !is_close_to_chunk(bb, chunk_x, chunk_z, 12) {
                    continue;
                }

                let is_jigsaw = piece.projection.is_some();
                let is_rigid = matches!(piece.projection, Some(Projection::Rigid));

                // Vanilla: only non-jigsaw pieces and jigsaw-RIGID pieces become rigids.
                // Jigsaw TERRAIN_MATCHING pieces are skipped here (junctions still collected
                // below, and the encompassing box only gets junction positions for them).
                if !is_jigsaw || is_rigid {
                    encompassing = Some(match encompassing {
                        Some(enc) => BoundingBox::encapsulating(&enc, bb),
                        None => *bb,
                    });

                    rigids.push(Rigid {
                        bounding_box: *bb,
                        terrain_adjustment: terrain_adj,
                        // Vanilla uses 0 for non-jigsaw pieces regardless of any stored value;
                        // jigsaw RIGID pieces use the projection-derived delta.
                        ground_level_delta: if is_jigsaw {
                            piece.ground_level_delta
                        } else {
                            0
                        },
                    });
                }

                // Junctions: vanilla collects them only on jigsaw pieces, and bounds are
                // strict — exclusive on both sides:
                // `(chunkStartBlockX - 12, chunkStartBlockX + 15 + 12)`, same for Z.
                if is_jigsaw {
                    for junction in &piece.junctions {
                        let jx = junction.source_pos.x;
                        let jz = junction.source_pos.z;
                        if jx > chunk_start_x - 12
                            && jz > chunk_start_z - 12
                            && jx < chunk_start_x + 15 + 12
                            && jz < chunk_start_z + 15 + 12
                        {
                            let jy = junction.source_pos.y;
                            let junction_bb =
                                BoundingBox::new(IVec3::new(jx, jy, jz), IVec3::new(jx, jy, jz));
                            encompassing = Some(match encompassing {
                                Some(enc) => BoundingBox::encapsulating(&enc, &junction_bb),
                                None => junction_bb,
                            });
                            junctions.push(junction.clone());
                        }
                    }
                }
            }
        }

        let affected_box = encompassing
            .map(|bb| bb.inflate_xyz(KERNEL_SIZE as i32, KERNEL_SIZE as i32, KERNEL_SIZE as i32));

        let columns = BeardColumns::new(&rigids, &junctions, chunk_start_x, chunk_start_z);
        Self {
            rigids,
            junctions,
            affected_box,
            chunk_start_x,
            chunk_start_z,
            columns,
        }
    }

    /// Returns true if there are no pieces or junctions affecting terrain.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rigids.is_empty() && self.junctions.is_empty()
    }

    /// Whether [`Self::compute`] can be non-zero anywhere in the inclusive
    /// block box `min..=max`.
    #[must_use]
    pub fn may_affect(&self, min: IVec3, max: IVec3) -> bool {
        self.affected_box
            .is_some_and(|affected| affected.intersects_bounds(min, max))
    }

    /// Compute the total density contribution at a world-space block position.
    ///
    /// Returns 0.0 if no structures are nearby.
    #[must_use]
    pub fn compute(&self, block_x: i32, block_y: i32, block_z: i32) -> f64 {
        let Some(affected) = &self.affected_box else {
            return 0.0;
        };
        if !affected.contains_xyz(block_x, block_y, block_z) {
            return 0.0;
        }

        let local_x = block_x - self.chunk_start_x;
        let local_z = block_z - self.chunk_start_z;
        if !(0..16).contains(&local_x) || !(0..16).contains(&local_z) {
            return self.compute_all(block_x, block_y, block_z);
        }

        let column = (local_z * 16 + local_x) as usize;
        let columns = &self.columns;
        let mut value = 0.0f32;
        for &index in &columns.rigid_indices[BeardColumns::range(&columns.rigid_offsets, column)] {
            value += self.rigids[index as usize].contribution(block_x, block_y, block_z);
        }
        for &index in
            &columns.junction_indices[BeardColumns::range(&columns.junction_offsets, column)]
        {
            value +=
                junction_contribution(&self.junctions[index as usize], block_x, block_y, block_z);
        }
        f64::from(value)
    }

    /// Vanilla's loop over every rigid and junction; used outside the chunk.
    fn compute_all(&self, block_x: i32, block_y: i32, block_z: i32) -> f64 {
        let mut value = 0.0f32;
        for rigid in &self.rigids {
            value += rigid.contribution(block_x, block_y, block_z);
        }
        for junction in &self.junctions {
            value += junction_contribution(junction, block_x, block_y, block_z);
        }
        f64::from(value)
    }
}

/// Check if a bounding box is within `margin` blocks of a chunk.
///
/// Matches vanilla's `StructurePiece.isCloseToChunk(ChunkPos, int)`.
const fn is_close_to_chunk(bb: &BoundingBox, chunk_x: i32, chunk_z: i32, margin: i32) -> bool {
    let chunk_start_x = chunk_x * 16;
    let chunk_start_z = chunk_z * 16;
    let chunk_end_x = chunk_start_x + 15;
    let chunk_end_z = chunk_start_z + 15;

    bb.max_x() >= chunk_start_x - margin
        && bb.min_x() <= chunk_end_x + margin
        && bb.max_z() >= chunk_start_z - margin
        && bb.min_z() <= chunk_end_z + margin
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rigid(min: IVec3, max: IVec3, terrain_adjustment: TerrainAdjustment) -> Rigid {
        Rigid {
            bounding_box: BoundingBox::new(min, max),
            terrain_adjustment,
            ground_level_delta: 1,
        }
    }

    fn junction(x: i32, y: i32, z: i32) -> JigsawJunction {
        JigsawJunction {
            source_pos: IVec3::new(x, y, z),
            delta_y: 0,
            dest_projection: Projection::Rigid,
        }
    }

    /// The per-column index must sum exactly what vanilla's full loop sums,
    /// including pieces and junctions right at the reach edges.
    #[test]
    fn column_index_matches_full_loop_bit_for_bit() {
        let rigids = vec![
            rigid(
                IVec3::new(2, 10, 3),
                IVec3::new(9, 20, 12),
                TerrainAdjustment::BeardThin,
            ),
            rigid(
                IVec3::new(27, 5, 0),
                IVec3::new(30, 15, 4),
                TerrainAdjustment::BeardBox,
            ),
            rigid(
                IVec3::new(-12, 0, 5),
                IVec3::new(-12, 8, 9),
                TerrainAdjustment::Bury,
            ),
            rigid(
                IVec3::new(4, 0, -13),
                IVec3::new(8, 30, -11),
                TerrainAdjustment::Encapsulate,
            ),
            rigid(
                IVec3::new(-40, 0, -40),
                IVec3::new(-30, 10, -30),
                TerrainAdjustment::BeardBox,
            ),
        ];
        let junctions = vec![
            junction(-12, 12, 7),
            junction(-13, 12, 7),
            junction(27, 14, 8),
            junction(28, 14, 8),
            junction(8, 6, 8),
        ];
        let columns = BeardColumns::new(&rigids, &junctions, 0, 0);
        let beardifier = Beardifier {
            rigids,
            junctions,
            affected_box: Some(BoundingBox::new(
                IVec3::new(-100, -100, -100),
                IVec3::new(100, 100, 100),
            )),
            chunk_start_x: 0,
            chunk_start_z: 0,
            columns,
        };

        let mut nonzero = 0;
        for y in -16..48 {
            for z in 0..16 {
                for x in 0..16 {
                    let indexed = beardifier.compute(x, y, z);
                    let full = beardifier.compute_all(x, y, z);
                    assert_eq!(indexed.to_bits(), full.to_bits(), "at ({x}, {y}, {z})");
                    nonzero += usize::from(indexed != 0.0);
                }
            }
        }
        assert!(nonzero > 0, "fixture should produce contributions");
    }
}
