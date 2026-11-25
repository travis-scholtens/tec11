//! Axial hex math, `16×16×32` `HexChunk` compressed storage (`Uniform`/`RLE`/`Dense`),
//! and `LocalTerrainVolume` on-demand chunk streaming with `DeltaOverlay` support.

use std::collections::HashMap;
use std::sync::Arc;

use crate::anchor::LocalZoomAnchor;
use crate::delta::DeltaOverlay;
use crate::geomorphology::{sample_cell_at_z, synthesize_column};
use crate::materials::{cell_flags, HexPrismCell};
use crate::rivers::{DrainageNetwork, DrainageReach};
use crate::stratigraphy::RegionalStratigraphyField;
use crate::{LocalTerrainError, LocalVolumeConfig};

pub const CHUNK_Q: i32 = 16;
pub const CHUNK_R: i32 = 16;
pub const CHUNK_Z: i32 = 32;
pub const CHUNK_VOLUME: usize = (CHUNK_Q * CHUNK_R * CHUNK_Z) as usize; // 8,192

pub const SQRT_3: f64 = 1.7320508075688772;

/// Convert axial hex coordinates `(q, r)` to pointy-top 2D metric `(x_m, y_m)`
/// where center-to-center distance between adjacent hexes is `hex_spacing_m`.
#[inline]
pub fn axial_to_xy_m(q: i32, r: i32, hex_spacing_m: f32) -> [f64; 2] {
    let s = hex_spacing_m as f64;
    let x = s * ((q as f64) + 0.5 * (r as f64));
    let y = s * (SQRT_3 * 0.5) * (r as f64);
    [x, y]
}

/// Convert metric `(x_m, y_m)` back to nearest axial hex `(q, r)` using cube rounding.
#[inline]
pub fn xy_m_to_axial(x_m: f64, y_m: f64, hex_spacing_m: f32) -> (i32, i32) {
    let s = (hex_spacing_m as f64).max(1e-6);
    let rf = y_m / (s * SQRT_3 * 0.5);
    let qf = (x_m / s) - 0.5 * rf;
    let sf = -qf - rf;

    let mut rq = qf.round();
    let mut rr = rf.round();
    let rs = sf.round();

    let dq = (rq - qf).abs();
    let dr = (rr - rf).abs();
    let ds = (rs - sf).abs();

    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    (rq as i32, rr as i32)
}

/// 6 axial hex neighbor offsets.
pub const HEX_DIRS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// Chunk coordinates (`data-model.md`: `ChunkCoord`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ChunkCoord {
    pub cq: i32,
    pub cr: i32,
    pub cz: i32,
}

impl ChunkCoord {
    #[inline]
    pub fn from_cell(q: i32, r: i32, z: i32) -> (Self, u16) {
        let cq = q.div_euclid(CHUNK_Q);
        let cr = r.div_euclid(CHUNK_R);
        let cz = z.div_euclid(CHUNK_Z);
        let lq = q.rem_euclid(CHUNK_Q) as usize;
        let lr = r.rem_euclid(CHUNK_R) as usize;
        let lz = z.rem_euclid(CHUNK_Z) as usize;
        let idx = local_index(lq, lr, lz);
        (Self { cq, cr, cz }, idx)
    }
}

#[inline]
pub const fn local_index(lq: usize, lr: usize, lz: usize) -> u16 {
    // Z-major inside column so vertical runs of uniform bedrock/air compress maximally in RLE
    ((lq * (CHUNK_R as usize) + lr) * (CHUNK_Z as usize) + lz) as u16
}

/// Compact chunk storage (`data-model.md`: `HexChunk.storage`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkStorage {
    Uniform(HexPrismCell),
    Rle(Vec<(u16, HexPrismCell)>),
    Dense(Box<[HexPrismCell; CHUNK_VOLUME]>),
}

impl ChunkStorage {
    pub fn from_dense(cells: [HexPrismCell; CHUNK_VOLUME]) -> Self {
        let first = cells[0];
        if cells.iter().all(|&c| c == first) {
            return Self::Uniform(first);
        }
        // Build RLE runs; keep RLE if it uses < 60% of dense memory
        let mut runs: Vec<(u16, HexPrismCell)> = Vec::new();
        let mut cur = first;
        let mut count = 1u16;
        for &c in &cells[1..] {
            if c == cur && count < u16::MAX {
                count += 1;
            } else {
                runs.push((count, cur));
                cur = c;
                count = 1;
            }
        }
        runs.push((count, cur));

        if runs.len() * std::mem::size_of::<(u16, HexPrismCell)>() < (CHUNK_VOLUME * 4 * 3) / 5 {
            runs.shrink_to_fit();
            Self::Rle(runs)
        } else {
            Self::Dense(Box::new(cells))
        }
    }

    #[inline]
    pub fn get(&self, idx: u16) -> HexPrismCell {
        match self {
            Self::Uniform(c) => *c,
            Self::Dense(arr) => arr[idx as usize],
            Self::Rle(runs) => {
                let mut acc = 0usize;
                let target = idx as usize;
                for &(len, cell) in runs {
                    acc += len as usize;
                    if target < acc {
                        return cell;
                    }
                }
                HexPrismCell::AIR
            }
        }
    }

    pub fn set(&mut self, idx: u16, cell: HexPrismCell) {
        if let Self::Uniform(u) = self {
            if *u == cell {
                return;
            }
        }
        let dense = match self {
            Self::Dense(arr) => {
                arr[idx as usize] = cell;
                return;
            }
            Self::Uniform(u) => Box::new([*u; CHUNK_VOLUME]),
            Self::Rle(runs) => {
                let mut arr = Box::new([HexPrismCell::AIR; CHUNK_VOLUME]);
                let mut pos = 0usize;
                for &(len, c) in runs.iter() {
                    let end = (pos + len as usize).min(CHUNK_VOLUME);
                    for slot in &mut arr[pos..end] {
                        *slot = c;
                    }
                    pos = end;
                }
                arr
            }
        };
        let mut d = dense;
        d[idx as usize] = cell;
        *self = Self::Dense(d);
    }

    pub fn estimated_bytes(&self) -> usize {
        match self {
            Self::Uniform(_) => std::mem::size_of::<Self>(),
            Self::Rle(v) => std::mem::size_of::<Self>() + v.capacity() * std::mem::size_of::<(u16, HexPrismCell)>(),
            Self::Dense(_) => std::mem::size_of::<Self>() + CHUNK_VOLUME * std::mem::size_of::<HexPrismCell>(),
        }
    }
}

/// Fixed-size `16×16×32` 3D hex-prism chunk (`data-model.md`: `HexChunk`).
#[derive(Clone, Debug)]
pub struct HexChunk {
    pub coord: ChunkCoord,
    pub storage: ChunkStorage,
    pub dirty_mesh: bool,
}

impl HexChunk {
    #[inline]
    pub fn get(&self, lq: usize, lr: usize, lz: usize) -> HexPrismCell {
        self.storage.get(local_index(lq, lr, lz))
    }

    pub fn set(&mut self, lq: usize, lr: usize, lz: usize, cell: HexPrismCell) {
        self.storage.set(local_index(lq, lr, lz), cell);
        self.dirty_mesh = true;
    }

    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.storage.estimated_bytes()
    }
}

/// Detailed click-to-inspect report for a single hex prism `(q, r, z)`.
#[derive(Clone, Debug)]
pub struct CellInspection {
    pub q: i32,
    pub r: i32,
    pub z: i32,
    pub elevation_m: f32,
    pub surface_elev_m: f32,
    pub depth_below_surface_m: f32,
    pub soil_depth_m: f32,
    pub cell: HexPrismCell,
    pub summary: String,
}

/// Streamed collection of `HexChunk`s around a local focus point (`data-model.md`: `LocalTerrainVolume`).
pub struct LocalTerrainVolume {
    anchor: LocalZoomAnchor,
    config: LocalVolumeConfig,
    stratigraphy: RegionalStratigraphyField,
    drainage: Arc<DrainageNetwork>,
    pub center_xy_m: [f64; 2],
    pub surface_z_center: i32,
    chunks: HashMap<ChunkCoord, HexChunk>,
    delta: DeltaOverlay,
}

impl LocalTerrainVolume {
    pub fn new(
        anchor: LocalZoomAnchor,
        config: LocalVolumeConfig,
        stratigraphy: RegionalStratigraphyField,
        drainage: Arc<DrainageNetwork>,
        center_xy_m: [f64; 2],
    ) -> Self {
        let center_col =
            synthesize_column(&anchor, &stratigraphy, &drainage, center_xy_m[0], center_xy_m[1]);
        let surface_z_center = (center_col.surface_elev_m / config.z_step_m).round() as i32;
        let anchor_hash = anchor.anchor_hash();
        Self {
            anchor,
            config,
            stratigraphy,
            drainage,
            center_xy_m,
            surface_z_center,
            chunks: HashMap::new(),
            delta: DeltaOverlay::new(anchor_hash),
        }
    }

    /// Maximum surface Z across the centered embark window (`[-half_extent..=half_extent]`) so
    /// Level 3 fits `z_max` above steep coastal or mountain slopes without decapitating terrain.
    pub fn max_surface_z_in_window(&self, half_extent: i32) -> i32 {
        let mut max_z = self.surface_z_center;
        let spacing = self.config.hex_spacing_m;
        let z_step = self.config.z_step_m;
        for q in (-half_extent..=half_extent).step_by(4) {
            for r in (-half_extent..=half_extent).step_by(4) {
                let rel = axial_to_xy_m(q, r, spacing);
                let col = synthesize_column(
                    &self.anchor,
                    &self.stratigraphy,
                    &self.drainage,
                    self.center_xy_m[0] + rel[0],
                    self.center_xy_m[1] + rel[1],
                );
                let top_m = col
                    .water_elev_m
                    .unwrap_or(col.surface_elev_m)
                    .max(col.surface_elev_m + col.ice_thickness_m);
                let sz = (top_m / z_step).ceil() as i32;
                if sz > max_z {
                    max_z = sz;
                }
            }
        }
        max_z
    }

    pub fn config(&self) -> LocalVolumeConfig {
        self.config
    }

    pub fn anchor(&self) -> &LocalZoomAnchor {
        &self.anchor
    }

    pub fn stratigraphy(&self) -> &RegionalStratigraphyField {
        &self.stratigraphy
    }

    pub fn rivers(&self) -> &[DrainageReach] {
        &self.drainage.reaches
    }

    pub fn drainage(&self) -> &DrainageNetwork {
        &self.drainage
    }

    /// Generate (if not cached) and return a `16×16×32` `HexChunk`, applying any `DeltaOverlay` edits.
    pub fn get_or_generate_chunk(&mut self, coord: ChunkCoord) -> &HexChunk {
        if !self.chunks.contains_key(&coord) {
            let chunk = self.generate_chunk(coord);
            self.chunks.insert(coord, chunk);
        }
        self.chunks.get(&coord).unwrap()
    }

    fn generate_chunk(&self, coord: ChunkCoord) -> HexChunk {
        let mut cells = [HexPrismCell::AIR; CHUNK_VOLUME];
        let base_q = coord.cq * CHUNK_Q;
        let base_r = coord.cr * CHUNK_R;
        let base_z = coord.cz * CHUNK_Z;
        let z_step = self.config.z_step_m;

        for lq in 0..(CHUNK_Q as usize) {
            let q = base_q + (lq as i32);
            for lr in 0..(CHUNK_R as usize) {
                let r = base_r + (lr as i32);
                let rel_xy = axial_to_xy_m(q, r, self.config.hex_spacing_m);
                let wx = self.center_xy_m[0] + rel_xy[0];
                let wy = self.center_xy_m[1] + rel_xy[1];
                let col = synthesize_column(&self.anchor, &self.stratigraphy, &self.drainage, wx, wy);

                let col_offset = (lq * (CHUNK_R as usize) + lr) * (CHUNK_Z as usize);
                for lz in 0..(CHUNK_Z as usize) {
                    let z_m = ((base_z + (lz as i32)) as f32) * z_step;
                    cells[col_offset + lz] =
                        sample_cell_at_z(&self.anchor, &self.stratigraphy, &col, wx, wy, z_m);
                }
            }
        }

        let mut storage = ChunkStorage::from_dense(cells);
        if let Some(edits) = self.delta.get_chunk_edits(coord) {
            for (&idx, &cell) in edits {
                storage.set(idx, cell);
            }
        }

        HexChunk {
            coord,
            storage,
            dirty_mesh: true,
        }
    }

    /// Query a single hex prism at `(q, r, z)`.
    pub fn get_cell(&mut self, q: i32, r: i32, z: i32) -> HexPrismCell {
        let (coord, idx) = ChunkCoord::from_cell(q, r, z);
        self.get_or_generate_chunk(coord).storage.get(idx)
    }

    /// Mutate a single hex prism (e.g., digging or building) and record in `DeltaOverlay`.
    pub fn set_cell(&mut self, q: i32, r: i32, z: i32, mut cell: HexPrismCell) {
        if cell.is_air() {
            cell.flags |= cell_flags::EXCAVATED_BY_DELTA;
        }
        let (coord, idx) = ChunkCoord::from_cell(q, r, z);
        self.delta.set_cell(coord, idx, cell);
        if let Some(chunk) = self.chunks.get_mut(&coord) {
            chunk.storage.set(idx, cell);
            chunk.dirty_mesh = true;
        }
    }

    /// Inspect a hex prism at `(q, r, z)` for click-to-inspect UI or downstream queries.
    pub fn inspect_column(&mut self, q: i32, r: i32, z: i32) -> CellInspection {
        let cell = self.get_cell(q, r, z);
        let rel_xy = axial_to_xy_m(q, r, self.config.hex_spacing_m);
        let wx = self.center_xy_m[0] + rel_xy[0];
        let wy = self.center_xy_m[1] + rel_xy[1];
        let col = synthesize_column(&self.anchor, &self.stratigraphy, &self.drainage, wx, wy);
        let elev_m = (z as f32) * self.config.z_step_m;
        let depth_below = (col.surface_elev_m - elev_m).max(0.0);
        let summary = format!(
            "{} ({}) at ({}, {}, z={} m) · Depth {:.1} m · Soil {:.1} m",
            cell.material.name(),
            cell.material.category_name(),
            q,
            r,
            elev_m.round() as i32,
            depth_below,
            col.soil_depth_m
        );
        CellInspection {
            q,
            r,
            z,
            elevation_m: elev_m,
            surface_elev_m: col.surface_elev_m,
            depth_below_surface_m: depth_below,
            soil_depth_m: col.soil_depth_m,
            cell,
            summary,
        }
    }

    pub fn export_delta_bytes(&self) -> Vec<u8> {
        self.delta.encode_bytes()
    }

    pub fn import_delta_bytes(&mut self, bytes: &[u8]) -> Result<(), LocalTerrainError> {
        let decoded = DeltaOverlay::decode_bytes(bytes, self.anchor.anchor_hash())?;
        for (&coord, edits) in &decoded.chunks {
            if let Some(chunk) = self.chunks.get_mut(&coord) {
                for (&idx, &cell) in edits {
                    chunk.storage.set(idx, cell);
                }
                chunk.dirty_mesh = true;
            }
        }
        self.delta = decoded;
        Ok(())
    }

    pub fn cached_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn estimated_memory_bytes(&self) -> usize {
        let chunk_bytes: usize = self.chunks.values().map(|c| c.estimated_bytes()).sum();
        let delta_bytes = self.delta.modified_cell_count() * 12;
        chunk_bytes + delta_bytes
    }
}
