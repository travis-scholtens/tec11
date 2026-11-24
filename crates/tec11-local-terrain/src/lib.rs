//! Pure-Rust local 3D hex-prism terrain & underground zoom crate (`tec11-local-terrain`).
//!
//! Consumes immutable `.tec11` `PlanetSnapshot` bytes (Tectonics, Prehistory, or History)
//! with zero browser/WASM dependencies, providing 4-level progressive drill-down patches,
//! streamable `16×16×32` 3D hex-prism chunks, laterally continuous stratigraphy, monotonic
//! river carving, 6 composable geomorphology operators, a face-culled Z-slicing mesher,
//! and a self-contained binary `DeltaOverlay` (`T11D`) codec.

pub mod anchor;
pub mod chunk;
pub mod delta;
pub mod geomorphology;
pub mod materials;
pub mod mesher;
pub mod rivers;
pub mod stratigraphy;

use std::fmt;

pub use anchor::{CoarseRiverEdge, LayerMineralHint, LocalZoomAnchor, NeighborTileSample, SnapshotStage};
pub use chunk::{
    axial_to_xy_m, xy_m_to_axial, CellInspection, ChunkCoord, ChunkStorage, HexChunk,
    LocalTerrainVolume, CHUNK_Q, CHUNK_R, CHUNK_VOLUME, CHUNK_Z,
};
pub use delta::{DeltaOverlay, DELTA_FORMAT_VERSION};
pub use geomorphology::{sample_cell_at_z, synthesize_column, ColumnSynthesis};
pub use materials::{cell_flags, HexPrismCell, MaterialId};
pub use mesher::{HexMeshBuffers, LocalTerrainMesher};
pub use rivers::{build_river_splines, query_rivers, RiverQueryResult, RiverSplineSegment};
pub use stratigraphy::{
    CorrelatedStratum, InterpolatedLocalContext, LocalStratumSlice, MineralVeinSpec,
    RegionalStratigraphyField,
};

#[derive(Clone, Debug)]
pub enum LocalTerrainError {
    SnapshotDecode(String),
    MissingStageData(&'static str),
    InvalidTileId { tile_id: usize, tile_count: usize },
    TileCountMismatch { expected: usize, actual: usize },
    InvalidDelta(&'static str),
    AnchorHashMismatch { expected: u64, actual: u64 },
}

impl fmt::Display for LocalTerrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnapshotDecode(e) => write!(f, "Failed to decode PlanetSnapshot: {e}"),
            Self::MissingStageData(msg) => write!(f, "Missing stage data: {msg}"),
            Self::InvalidTileId { tile_id, tile_count } => {
                write!(f, "Tile ID {tile_id} out of range (planet has {tile_count} tiles)")
            }
            Self::TileCountMismatch { expected, actual } => {
                write!(f, "Expected {expected} tiles for grid_size, found {actual}")
            }
            Self::InvalidDelta(msg) => write!(f, "Invalid T11D delta: {msg}"),
            Self::AnchorHashMismatch { expected, actual } => {
                write!(f, "Delta anchor hash mismatch (expected {expected:#x}, got {actual:#x})")
            }
        }
    }
}

impl std::error::Error for LocalTerrainError {}

/// Configuration for Level 3 local 3D hex-prism volumes (`data-model.md`: `ZoomLevel`).
#[derive(Copy, Clone, Debug)]
pub struct LocalVolumeConfig {
    /// Horizontal center-to-center hex spacing at Level 3 leaf scale (default `1.0 m`, range `0.5..=2.0`).
    pub hex_spacing_m: f32,
    /// Vertical z-level height at Level 3 leaf scale (default `1.0 m`).
    pub z_step_m: f32,
}

impl Default for LocalVolumeConfig {
    fn default() -> Self {
        Self {
            hex_spacing_m: 1.0,
            z_step_m: 1.0,
        }
    }
}

/// Single hex column in an intermediate progressive drill-down patch (`Levels 0..=2`).
#[derive(Clone, Debug)]
pub struct PatchColumn {
    pub q: i32,
    pub r: i32,
    pub local_xy_m: [f64; 2],
    pub surface_elev_m: f32,
    pub water_elev_m: Option<f32>,
    pub surface_material: MaterialId,
    pub soil_depth_m: f32,
    pub is_river: bool,
}

/// 2.5D hex-column patch rendered at progressive drill-down Levels 0–2 (`data-model.md`).
#[derive(Clone, Debug)]
pub struct DrillDownPatch {
    pub level: u8,
    pub center_xy_m: [f64; 2],
    pub hex_spacing_m: f32,
    pub radius_hex: i32,
    pub columns: Vec<PatchColumn>,
}

/// Automatically discovered physical Point of Interest inside a tile's local neighborhood.
#[derive(Clone, Debug)]
pub struct LocalPointOfInterest {
    pub kind: &'static str,
    pub label: String,
    pub local_xy_m: [f64; 2],
    pub elevation_m: f32,
    pub description: String,
}

/// Reusable pure-Rust local terrain world anchored to a `.tec11` planet tile.
pub struct LocalTerrainWorld {
    anchor: LocalZoomAnchor,
    config: LocalVolumeConfig,
    stratigraphy: RegionalStratigraphyField,
    rivers: Vec<RiverSplineSegment>,
}

impl LocalTerrainWorld {
    pub fn new(anchor: LocalZoomAnchor, mut config: LocalVolumeConfig) -> Self {
        config.hex_spacing_m = config.hex_spacing_m.clamp(0.5, 2.0);
        config.z_step_m = config.z_step_m.clamp(0.5, 2.0);
        let stratigraphy = RegionalStratigraphyField::from_anchor(&anchor);
        let rivers = build_river_splines(&anchor, &stratigraphy);
        Self {
            anchor,
            config,
            stratigraphy,
            rivers,
        }
    }

    pub fn from_snapshot_bytes(
        snapshot_bytes: &[u8],
        parent_tile_id: usize,
        config: LocalVolumeConfig,
    ) -> Result<Self, LocalTerrainError> {
        let anchor = LocalZoomAnchor::from_snapshot_bytes(snapshot_bytes, parent_tile_id)?;
        Ok(Self::new(anchor, config))
    }

    pub fn anchor(&self) -> &LocalZoomAnchor {
        &self.anchor
    }

    pub fn config(&self) -> LocalVolumeConfig {
        self.config
    }

    pub fn stratigraphy(&self) -> &RegionalStratigraphyField {
        &self.stratigraphy
    }

    pub fn rivers(&self) -> &[RiverSplineSegment] {
        &self.rivers
    }

    /// Horizontal hex spacing (meters) for progressive drill-down level `0..=3`.
    pub fn hex_spacing_for_level(&self, level: u8) -> f32 {
        match level {
            0 => ((self.anchor.parent_tile_radius_m as f32) / 18.0).clamp(1500.0, 6000.0),
            1 => ((self.anchor.parent_tile_radius_m as f32) / 160.0).clamp(200.0, 750.0),
            2 => 32.0,
            _ => self.config.hex_spacing_m,
        }
    }

    /// Generate a 2.5D hex-column patch at zoom level `0..=2` centered at `center_xy_m`.
    pub fn generate_drilldown_patch(
        &self,
        level: u8,
        center_xy_m: [f64; 2],
        radius_hex: i32,
    ) -> DrillDownPatch {
        let clamped_lvl = level.min(2);
        let spacing = self.hex_spacing_for_level(clamped_lvl);
        let r_hex = radius_hex.clamp(4, 40);
        let mut columns = Vec::with_capacity(((2 * r_hex + 1) * (2 * r_hex + 1)) as usize);

        for q in -r_hex..=r_hex {
            let r_min = (-r_hex).max(-q - r_hex);
            let r_max = r_hex.min(-q + r_hex);
            for r in r_min..=r_max {
                let rel = axial_to_xy_m(q, r, spacing);
                let wx = center_xy_m[0] + rel[0];
                let wy = center_xy_m[1] + rel[1];
                let col = geomorphology::synthesize_column_with_river_min_half_w(
                    &self.anchor,
                    &self.stratigraphy,
                    &self.rivers,
                    wx,
                    wy,
                    spacing * 0.55,
                );
                let surface_material = if col.ice_thickness_m > 0.0 {
                    MaterialId::SNOW
                } else {
                    col.surficial_material
                };
                columns.push(PatchColumn {
                    q,
                    r,
                    local_xy_m: [wx, wy],
                    surface_elev_m: col.surface_elev_m,
                    water_elev_m: col.water_elev_m,
                    surface_material,
                    soil_depth_m: col.soil_depth_m,
                    is_river: col.is_river,
                });
            }
        }

        DrillDownPatch {
            level: clamped_lvl,
            center_xy_m,
            hex_spacing_m: spacing,
            radius_hex: r_hex,
            columns,
        }
    }

    /// Create a Level 3 streamable 3D hex-prism volume centered at `center_xy_m`.
    pub fn create_leaf_volume(&self, center_xy_m: [f64; 2]) -> LocalTerrainVolume {
        LocalTerrainVolume::new(
            self.anchor.clone(),
            self.config,
            self.stratigraphy.clone(),
            self.rivers.clone(),
            center_xy_m,
        )
    }

    /// Scan the local tile neighborhood (< 5 ms) to discover notable physical features
    /// present on this tile (River, Highest Peak, Coastline, Karst, Ore Outcrop, Impact Crater).
    pub fn find_points_of_interest(&self) -> Vec<LocalPointOfInterest> {
        let mut pois = Vec::new();
        let sea_m = self.anchor.sea_level_m as f32;
        let max_r = self.anchor.parent_tile_radius_m * 1.15;

        // 1. River / Gorge: pick the river segment midpoint closest to tile center
        if !self.rivers.is_empty() {
            let mut best_river: Option<([f64; 2], f32, f64, f32)> = None;
            for seg in &self.rivers {
                let z_w = 0.5 * (seg.z_water_0_m + seg.z_water_1_m);
                if z_w <= sea_m + 1.0 {
                    continue;
                }
                let mx = 0.5 * (seg.p0_m[0] + seg.p1_m[0]);
                let my = 0.5 * (seg.p0_m[1] + seg.p1_m[1]);
                let d = (mx * mx + my * my).sqrt();
                if d <= max_r * 1.35 {
                    let replace = match best_river {
                        None => true,
                        Some((_, _, best_d, _)) => d < best_d,
                    };
                    if replace {
                        best_river = Some(([mx, my], z_w, d, seg.channel_width_m));
                    }
                }
            }
            if let Some((xy, z_w, _, w)) = best_river {
                pois.push(LocalPointOfInterest {
                    kind: "river",
                    label: "River".to_string(),
                    local_xy_m: xy,
                    elevation_m: z_w,
                    description: format!("River channel (~{:.0} m wide) at {:.0} m elevation", w, z_w),
                });
            }
        }

        // Sample Level 0 columns across the tile neighborhood for peaks, coast, karst, ore, craters
        let spacing = self.hex_spacing_for_level(0);
        let r_hex = 14i32;
        let mut best_peak: Option<([f64; 2], f32, MaterialId)> = None;
        let mut has_ocean = false;
        let mut has_land = false;
        let mut best_coast: Option<([f64; 2], f32, f32)> = None;
        let mut best_karst: Option<([f64; 2], f32, f32)> = None;
        let mut best_ore: Option<([f64; 2], f32, f32, MaterialId)> = None;
        let mut best_crater: Option<([f64; 2], f32, f32)> = None;

        for q in -r_hex..=r_hex {
            let r_min = (-r_hex).max(-q - r_hex);
            let r_max = r_hex.min(-q + r_hex);
            for r in r_min..=r_max {
                let xy = axial_to_xy_m(q, r, spacing);
                if (xy[0] * xy[0] + xy[1] * xy[1]).sqrt() > max_r {
                    continue;
                }
                let col = synthesize_column(&self.anchor, &self.stratigraphy, &self.rivers, xy[0], xy[1]);
                if col.is_ocean {
                    has_ocean = true;
                } else {
                    has_land = true;
                    if best_peak.map_or(true, |(_, e, _)| col.surface_elev_m > e) {
                        best_peak = Some((xy, col.surface_elev_m, col.surficial_material));
                    }
                    let coast_dist = (col.surface_elev_m - sea_m).abs();
                    if best_coast.map_or(true, |(_, _, d)| coast_dist < d) {
                        best_coast = Some((xy, col.surface_elev_m, coast_dist));
                    }
                }
                if self.anchor.has_atmosphere && !col.is_ocean {
                    if let Some(top) = col.strata.first() {
                        if top.calcity > 0.65 && col.ctx.precip_norm > 0.25 {
                            let score = top.calcity * col.ctx.precip_norm;
                            if best_karst.map_or(true, |(_, _, s)| score > s) {
                                best_karst = Some((xy, col.surface_elev_m, score));
                            }
                        }
                    }
                }
                // Check shallowest mineralized bed
                for s in col.strata.iter().take(3) {
                    if let Some(vein) = s.mineral_veins.first() {
                        let depth = (col.surface_elev_m - s.top_z_m).max(0.0);
                        if depth < 250.0 && best_ore.map_or(true, |(_, _, d, _)| depth < d) {
                            best_ore = Some((xy, col.surface_elev_m, depth, vein.material));
                        }
                        break;
                    }
                }
                if col.ctx.impact_scar > 0.25
                    && best_crater.map_or(true, |(_, _, sc)| col.ctx.impact_scar > sc)
                {
                    best_crater = Some((xy, col.surface_elev_m, col.ctx.impact_scar));
                }
            }
        }

        if let Some((xy, elev, mat)) = best_peak {
            pois.push(LocalPointOfInterest {
                kind: "peak",
                label: "High Point".to_string(),
                local_xy_m: xy,
                elevation_m: elev,
                description: format!("Highest local point at {:.0} m ({})", elev, mat.name()),
            });
        }
        if has_land && has_ocean {
            if let Some((xy, elev, dist)) = best_coast {
                if dist < 250.0 {
                    pois.push(LocalPointOfInterest {
                        kind: "coast",
                        label: "Coastline".to_string(),
                        local_xy_m: xy,
                        elevation_m: elev,
                        description: format!("Land/sea coastal transition near {:.0} m", elev),
                    });
                }
            }
        }
        if let Some((xy, elev, _)) = best_karst {
            pois.push(LocalPointOfInterest {
                kind: "karst",
                label: "Karst Caves".to_string(),
                local_xy_m: xy,
                elevation_m: elev,
                description: format!("Humid soluble limestone karst & caverns at {:.0} m", elev),
            });
        }
        if let Some((xy, elev, depth, mat)) = best_ore {
            pois.push(LocalPointOfInterest {
                kind: "ore",
                label: format!("Ore: {}", mat.name()),
                local_xy_m: xy,
                elevation_m: elev,
                description: format!("{} mineralized bed (~{:.0} m below surface)", mat.name(), depth),
            });
        }
        if let Some((xy, elev, _)) = best_crater {
            pois.push(LocalPointOfInterest {
                kind: "crater",
                label: "Impact Crater".to_string(),
                local_xy_m: xy,
                elevation_m: elev,
                description: format!("Impact crater structure & breccia at {:.0} m", elev),
            });
        }

        pois
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tec11_geology::rock::{Rock, RockKind, RockLayer};
    use tec11_geology::PrehistorySimulation;
    use tec11_proto::Message as _;

    fn earth_snapshot_bytes() -> Vec<u8> {
        let mut sim = PrehistorySimulation::new_earth(42);
        sim.step();
        sim.to_snapshot().encode_to_vec()
    }

    #[test]
    fn test_determinism_pentagon_safety_and_memory_budget() {
        let bytes = earth_snapshot_bytes();
        // Tile 0 on SphericalGrid is always one of the 12 pentagons
        let world_a = LocalTerrainWorld::from_snapshot_bytes(&bytes, 0, LocalVolumeConfig::default())
            .expect("Pentagon tile anchor should load");
        assert!(world_a.anchor().is_pentagon);
        let world_b = LocalTerrainWorld::from_snapshot_bytes(&bytes, 0, LocalVolumeConfig::default())
            .expect("Second load should succeed");

        let mut vol_a = world_a.create_leaf_volume([0.0, 0.0]);
        let mut vol_b = world_b.create_leaf_volume([0.0, 0.0]);
        let z0 = vol_a.surface_z_center - 48;

        // Generate 8x8x4 = 256 chunks (128x128x128 volume) and verify SC-004 determinism & SC-002 memory
        let cz0 = z0.div_euclid(CHUNK_Z);
        for cq in -4..4 {
            for cr in -4..4 {
                for cz in cz0..(cz0 + 4) {
                    let coord = ChunkCoord { cq, cr, cz };
                    let ca = vol_a.get_or_generate_chunk(coord).storage.clone();
                    let cb = vol_b.get_or_generate_chunk(coord).storage.clone();
                    assert_eq!(ca, cb, "Chunk {:?} must be bit-for-bit deterministic", coord);
                }
            }
        }
        let mem_bytes = vol_a.estimated_memory_bytes();
        assert!(
            mem_bytes < 16 * 1024 * 1024,
            "128x128x128 volume memory ({} bytes) must stay well under 16 MB",
            mem_bytes
        );

        // Verify progressive drill-down patches Levels 0..=2 produce finite elevations
        for lvl in 0..=2 {
            let patch = world_a.generate_drilldown_patch(lvl, [0.0, 0.0], 8);
            assert!(!patch.columns.is_empty());
            for col in &patch.columns {
                assert!(col.surface_elev_m.is_finite());
            }
            let mesh = LocalTerrainMesher::build_patch_mesh(&patch);
            assert!(mesh.vertex_count() > 0);
        }
    }

    #[test]
    fn test_lateral_stratigraphic_continuity_and_pinchouts() {
        // Construct a synthetic 2-neighbor anchor where tile 0 has limestone over granite
        // and neighbor 1 has only granite (no limestone) -> limestone must taper smoothly to 0
        let limestone = Rock {
            kind: RockKind::Sedimentary,
            name: "limestone",
            felsity: 0.5,
            toughness: 0.45,
            clasticity: Some(4.0),
            calcity: Some(0.92),
            bogginess: None,
            quartzity: None,
            sulfidity: None,
            salinity: None,
        };
        let granite = Rock::intrusive(0.74);

        let anchor = LocalZoomAnchor {
            world_seed: 99,
            stage: SnapshotStage::Prehistory,
            grid_size: 6,
            radius_km: 6371.0,
            parent_tile_id: 0,
            parent_tile_radius_m: 100_000.0,
            anchor_normal: [0.0, 1.0, 0.0],
            tangent_east: [1.0, 0.0, 0.0],
            tangent_north: [0.0, 0.0, 1.0],
            sea_level_m: 0.0,
            has_atmosphere: true,
            has_life: true,
            is_pentagon: false,
            neighbors: vec![
                NeighborTileSample {
                    tile_id: 0,
                    local_xy_m: [0.0, 0.0],
                    elevation_m: 400.0,
                    bottom_m: -9000.0,
                    mountainosity: 1.0,
                    subduction: 0.0,
                    impact_scar: 0.0,
                    soil_depth_m: 2.0,
                    soil_fertility: 1.0,
                    is_glaciated: false,
                    glacier_cover: 0.0,
                    annual_temp_c: 15.0,
                    precip_norm: 0.6,
                    tidal_range_m: 1.5,
                    wind_vec: [1.0, 0.0],
                    layers: vec![
                        RockLayer { rock: granite.clone(), thickness: 9.0 },
                        RockLayer { rock: limestone.clone(), thickness: 0.4 },
                    ],
                    occurrences: vec![],
                },
                NeighborTileSample {
                    tile_id: 1,
                    local_xy_m: [180_000.0, 0.0],
                    elevation_m: 600.0,
                    bottom_m: -9000.0,
                    mountainosity: 1.0,
                    subduction: 0.0,
                    impact_scar: 0.0,
                    soil_depth_m: 2.0,
                    soil_fertility: 1.0,
                    is_glaciated: false,
                    glacier_cover: 0.0,
                    annual_temp_c: 15.0,
                    precip_norm: 0.6,
                    tidal_range_m: 1.5,
                    wind_vec: [1.0, 0.0],
                    layers: vec![RockLayer { rock: granite.clone(), thickness: 9.6 }],
                    occurrences: vec![],
                },
            ],
            river_edges: vec![],
        };

        let strat = RegionalStratigraphyField::from_anchor(&anchor);
        let ctx_center = strat.sample_context(&anchor, 0.0, 0.0);
        let col_center = strat.evaluate_column(&ctx_center, 0.0, 0.0, 400.0);
        let ctx_mid = strat.sample_context(&anchor, 90_000.0, 0.0);
        let col_mid = strat.evaluate_column(&ctx_mid, 90_000.0, 0.0, 500.0);
        let ctx_nb = strat.sample_context(&anchor, 180_000.0, 0.0);
        let col_nb = strat.evaluate_column(&ctx_nb, 180_000.0, 0.0, 600.0);

        let lime_thick = |col: &[LocalStratumSlice]| -> f32 {
            col.iter()
                .find(|s| s.material == MaterialId::LIMESTONE)
                .map(|s| s.top_z_m - s.bottom_z_m)
                .unwrap_or(0.0)
        };
        let t0 = lime_thick(&col_center);
        let t1 = lime_thick(&col_mid);
        let t2 = lime_thick(&col_nb);
        assert!(
            t0 > t1 && t1 > t2,
            "Limestone bed should smoothly taper from center ({t0:.1} m) -> boundary ({t1:.1} m) -> neighbor ({t2:.1} m)"
        );
    }

    #[test]
    fn test_tunnel_excavation_and_delta_roundtrip() {
        let bytes = earth_snapshot_bytes();
        let world = LocalTerrainWorld::from_snapshot_bytes(&bytes, 10, LocalVolumeConfig::default())
            .expect("Load earth tile 10");
        let mut vol = world.create_leaf_volume([0.0, 0.0]);
        let z_tunnel = vol.surface_z_center - 15;

        // Excavate a tunnel spanning across chunk boundary q = 15 -> q = 16
        for q in 14..=17 {
            vol.set_cell(q, 0, z_tunnel, HexPrismCell::AIR);
        }
        let delta_bytes = vol.export_delta_bytes();
        assert!(!delta_bytes.is_empty());

        // Reload onto a fresh volume and verify the tunnel persists
        let mut vol2 = world.create_leaf_volume([0.0, 0.0]);
        vol2.import_delta_bytes(&delta_bytes)
            .expect("Delta import should succeed");
        for q in 14..=17 {
            let c = vol2.get_cell(q, 0, z_tunnel);
            assert!(c.is_air());
            assert_ne!(c.flags & cell_flags::EXCAVATED_BY_DELTA, 0);
        }

        // Verify mesher builds a clean Z-sliced volume mesh
        let mesh = LocalTerrainMesher::build_volume_mesh(&mut vol2, -8..8, -8..8, z_tunnel - 8, z_tunnel + 16);
        assert!(mesh.vertex_count() > 0);
    }
}
