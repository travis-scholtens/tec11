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
pub mod volcanoes;

use std::fmt;
use std::sync::Arc;

pub use anchor::{
    CoarseRiverEdge, FineGroundSample, LayerMineralHint, LocalZoomAnchor, NeighborTileSample,
    PlanetaryTerrainContext, SnapshotStage, TileCanonicalFrame,
};
pub use chunk::{
    axial_to_xy_m, xy_m_to_axial, CellInspection, ChunkCoord, ChunkStorage, HexChunk,
    LocalTerrainVolume, CHUNK_Q, CHUNK_R, CHUNK_VOLUME, CHUNK_Z,
};
pub use delta::{DeltaOverlay, DELTA_FORMAT_VERSION};
pub use geomorphology::{sample_cell_at_z, synthesize_column, ColumnSynthesis};
pub use materials::{cell_flags, HexPrismCell, MaterialId};
pub use mesher::{HexMeshBuffers, LocalTerrainMesher};
pub use rivers::{
    build_river_splines, classify_lake_state, query_rivers, DrainageNetwork, DrainageReach,
    LakeBasin, LakeQueryResult, LakeState, RiverQueryResult, RiverSplineSegment,
};
pub use stratigraphy::{
    CorrelatedStratum, InterpolatedLocalContext, LocalStratumSlice, MineralVeinSpec,
    RegionalStratigraphyField,
};
pub use volcanoes::{
    ParasiticCone, VolcanicEdifice, VolcanicField, VolcanicQueryResult, VolcanoKind,
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
    pub is_lake: bool,
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
    drainage: Arc<DrainageNetwork>,
    volcanoes: Arc<VolcanicField>,
}

impl LocalTerrainWorld {
    pub fn new(anchor: LocalZoomAnchor, mut config: LocalVolumeConfig) -> Self {
        config.hex_spacing_m = config.hex_spacing_m.clamp(0.5, 2.0);
        config.z_step_m = config.z_step_m.clamp(0.5, 2.0);
        let stratigraphy = RegionalStratigraphyField::from_anchor(&anchor);
        let drainage = Arc::new(DrainageNetwork::build(&anchor, &stratigraphy));
        let volcanoes = Arc::new(VolcanicField::build(&anchor, &stratigraphy, &drainage));
        Self {
            anchor,
            config,
            stratigraphy,
            drainage,
            volcanoes,
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

    pub fn rivers(&self) -> &[DrainageReach] {
        &self.drainage.reaches
    }

    pub fn drainage(&self) -> &DrainageNetwork {
        &self.drainage
    }

    pub fn volcanoes(&self) -> &[VolcanicEdifice] {
        &self.volcanoes.edifices
    }

    pub fn volcanic_field(&self) -> &VolcanicField {
        &self.volcanoes
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
        let include_tributaries = clamped_lvl >= 1;
        let mut columns = Vec::with_capacity(((2 * r_hex + 1) * (2 * r_hex + 1)) as usize);

        for q in -r_hex..=r_hex {
            let r_min = (-r_hex).max(-q - r_hex);
            let r_max = r_hex.min(-q + r_hex);
            for r in r_min..=r_max {
                let rel = axial_to_xy_m(q, r, spacing);
                let wx = center_xy_m[0] + rel[0];
                let wy = center_xy_m[1] + rel[1];
                let col = geomorphology::synthesize_column_with_options(
                    &self.anchor,
                    &self.stratigraphy,
                    &self.drainage,
                    &self.volcanoes,
                    wx,
                    wy,
                    spacing * 0.52,
                    include_tributaries,
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
                    is_lake: col.is_lake,
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
            Arc::clone(&self.drainage),
            Arc::clone(&self.volcanoes),
            center_xy_m,
        )
    }

    /// Scan the local tile neighborhood (< 5 ms) to discover notable physical features
    /// present on this tile (River, Lake, Highest Peak, Coastline, Karst, Ore Outcrop, Impact Crater).
    pub fn find_points_of_interest(&self) -> Vec<LocalPointOfInterest> {
        let mut pois = Vec::new();
        let sea_m = self.anchor.sea_level_m as f32;
        let max_r = self.anchor.parent_tile_radius_m * 1.15;

        // 1. River / Great River: anchor POI right on the riverbank so Level 3 opens showing both bank & water
        if !self.drainage.reaches.is_empty() {
            let mut best_river: Option<([f64; 2], f32, f64, f32)> = None;
            for seg in &self.drainage.reaches {
                if seg.order > 0 {
                    continue;
                }
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
                        let vx = seg.p1_m[0] - seg.p0_m[0];
                        let vy = seg.p1_m[1] - seg.p0_m[1];
                        let vlen = (vx * vx + vy * vy).sqrt().max(1.0);
                        let bank_off = (seg.channel_width_m as f64) * 0.45;
                        let bank_xy = [mx + (-vy / vlen) * bank_off, my + (vx / vlen) * bank_off];
                        best_river = Some((bank_xy, z_w, d, seg.channel_width_m));
                    }
                }
            }
            if let Some((xy, z_w, _, w)) = best_river {
                let label = if w >= 600.0 {
                    format!("Great River (~{:.1} km)", w / 1000.0)
                } else {
                    "River".to_string()
                };
                pois.push(LocalPointOfInterest {
                    kind: "river",
                    label,
                    local_xy_m: xy,
                    elevation_m: z_w,
                    description: format!("Riverbank (~{:.0} m wide channel) at {:.0} m elevation", w, z_w),
                });
            }
        }

        // 2. Structural Lake / Salt Flat / Frozen Lake
        if let Some(lake) = self.drainage.lakes.iter().find(|l| {
            let d = (l.center_xy_m[0].powi(2) + l.center_xy_m[1].powi(2)).sqrt();
            d <= max_r * 1.25 && (l.state == LakeState::Freshwater || l.state == LakeState::Frozen || l.state == LakeState::SaltFlat)
        }) {
            let (label, desc) = match lake.state {
                LakeState::Freshwater => (
                    "Lake".to_string(),
                    format!("Freshwater lake basin at {:.0} m elevation", lake.z_water_m),
                ),
                LakeState::Frozen => (
                    "Frozen Lake".to_string(),
                    format!("Ice-covered alpine lake at {:.0} m elevation", lake.z_water_m),
                ),
                LakeState::SaltFlat => (
                    "Salt Flat".to_string(),
                    format!("Evaporite salt playa basin at {:.0} m elevation", lake.z_water_m),
                ),
                LakeState::DryPlaya => ("Basin".to_string(), "Dry basin".to_string()),
            };
            pois.push(LocalPointOfInterest {
                kind: "lake",
                label,
                local_xy_m: lake.center_xy_m,
                elevation_m: lake.z_water_m,
                description: desc,
            });
        }

        // 2b. Volcanic Edifice / Caldera / Crater Lake / Volcanic Neck (`Spec 009`, `FR-007`)
        if let Some(ed) = self
            .volcanoes
            .edifices
            .iter()
            .filter(|e| {
                let d = (e.center_xy_m[0].powi(2) + e.center_xy_m[1].powi(2)).sqrt();
                d <= max_r * 1.25
            })
            .min_by(|a, b| {
                let da = a.center_xy_m[0].powi(2) + a.center_xy_m[1].powi(2);
                let db = b.center_xy_m[0].powi(2) + b.center_xy_m[1].powi(2);
                da.total_cmp(&db)
            })
        {
            let rim_xy = ed.poi_rim_xy_m();
            let rim_col = synthesize_column(
                &self.anchor,
                &self.stratigraphy,
                &self.drainage,
                &self.volcanoes,
                rim_xy[0],
                rim_xy[1],
            );
            let has_crater_lake = matches!(
                ed.crater_lake_state,
                Some(LakeState::Freshwater | LakeState::Frozen | LakeState::SaltFlat)
            );
            let is_caldera = ed.caldera_radius_m >= 1200.0;
            let label = ed.kind.display_name(has_crater_lake, is_caldera).to_string();
            let desc = if has_crater_lake {
                format!(
                    "{} rim at {:.0} m (crater lake at {:.0} m, {})",
                    label,
                    rim_col.surface_elev_m,
                    ed.crater_lake_z_m.unwrap_or(rim_col.surface_elev_m),
                    ed.magma_material.name()
                )
            } else {
                format!(
                    "{} summit/rim at {:.0} m (+{:.0} m relief, {})",
                    label,
                    rim_col.surface_elev_m,
                    ed.peak_height_m,
                    ed.magma_material.name()
                )
            };
            pois.push(LocalPointOfInterest {
                kind: "volcano",
                label,
                local_xy_m: rim_xy,
                elevation_m: rim_col.surface_elev_m,
                description: desc,
            });
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
                let col = synthesize_column(
                    &self.anchor,
                    &self.stratigraphy,
                    &self.drainage,
                    &self.volcanoes,
                    xy[0],
                    xy[1],
                );
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
                    has_intrusion: false,
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
                    has_intrusion: false,
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
            fine_samples: vec![],
            fine_spacing_m: 0.0,
            tile_frames: vec![],
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

    #[test]
    fn river_endpoints_and_ring2_neighbors_are_present_in_anchor() {
        let mut sim = PrehistorySimulation::new_earth(777);
        for _ in 0..4 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();

        let n_tiles = sim.tiles().len();
        let mut checked = 0usize;
        for tid in 0..n_tiles {
            if sim.tiles()[tid].elevation() <= 0.0 {
                continue;
            }
            let anchor = LocalZoomAnchor::from_snapshot_bytes(&bytes, tid)
                .expect("Prehistory anchor should build");
            if anchor.river_edges.is_empty() {
                continue;
            }
            assert!(
                anchor.neighbors.len() > 7,
                "Expected 2-ring neighbors (>7) on tile {tid}, got {}",
                anchor.neighbors.len()
            );
            for edge in &anchor.river_edges {
                let has_from = anchor.neighbors.iter().any(|nb| {
                    (nb.local_xy_m[0] - edge.from_xy_m[0]).abs() < 1.0
                        && (nb.local_xy_m[1] - edge.from_xy_m[1]).abs() < 1.0
                });
                let has_to = anchor.neighbors.iter().any(|nb| {
                    (nb.local_xy_m[0] - edge.to_xy_m[0]).abs() < 1.0
                        && (nb.local_xy_m[1] - edge.to_xy_m[1]).abs() < 1.0
                });
                assert!(has_from, "River edge start missing from anchor.neighbors on tile {tid}");
                assert!(has_to, "River edge end missing from anchor.neighbors on tile {tid}");
            }
            checked += 1;
            if checked >= 8 {
                break;
            }
        }
        assert!(checked > 0, "Expected at least one tile with river edges");
    }

    #[test]
    fn planetary_context_reanchors_and_roundtrips_lat_lon() {
        let mut sim = PrehistorySimulation::new_earth(123);
        for _ in 0..3 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();
        let ctx = PlanetaryTerrainContext::from_snapshot_bytes(&bytes)
            .expect("PlanetaryTerrainContext should decode");
        let anchor_a = ctx.build_anchor(0).expect("Anchor A should build");
        let nb = &anchor_a.neighbors[1];
        let (lat, lon) = anchor_a.xy_m_to_lat_lon(nb.local_xy_m);
        let back_xy = anchor_a.project_lat_lon_to_xy_m(lat, lon);
        let err_m = ((back_xy[0] - nb.local_xy_m[0]).powi(2)
            + (back_xy[1] - nb.local_xy_m[1]).powi(2))
        .sqrt();
        assert!(
            err_m < 25.0,
            "xy_m_to_lat_lon round-trip error should be tiny, got {err_m:.2} m"
        );
        assert_eq!(ctx.nearest_tile_id(lat, lon), nb.tile_id);

        let anchor_b = ctx.build_anchor(nb.tile_id).expect("Anchor B should build");
        let b_xy = anchor_b.project_lat_lon_to_xy_m(lat, lon);
        assert!(
            b_xy[0].abs() < 25.0 && b_xy[1].abs() < 25.0,
            "Projected neighbor center in its own anchor should be near (0,0), got {:?}",
            b_xy
        );
    }

    #[test]
    fn non_negative_relief_envelope_prevents_aqueducts_and_uphill_waterfalls() {
        let mut sim = PrehistorySimulation::new_earth(314);
        for _ in 0..4 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();
        // Find a land tile with an active river
        let mut verified_reaches = 0usize;
        for tid in 0..sim.tiles().len() {
            if sim.tiles()[tid].elevation() <= 0.2 {
                continue;
            }
            let world =
                LocalTerrainWorld::from_snapshot_bytes(&bytes, tid, LocalVolumeConfig::default())
                    .expect("World should load");
            if world.rivers().is_empty() {
                continue;
            }
            for seg in world.rivers() {
                assert!(
                    seg.z_water_1_m <= seg.z_water_0_m + 1.0e-4,
                    "River reach on tile {tid} must be strictly non-increasing downstream: {} -> {}",
                    seg.z_water_0_m,
                    seg.z_water_1_m
                );
                // Sample perpendicular banks on both sides of the reach midpoint:
                // banks must NEVER dip below `z_water` (zero floating aqueducts!).
                let mx = 0.5 * (seg.p0_m[0] + seg.p1_m[0]);
                let my = 0.5 * (seg.p0_m[1] + seg.p1_m[1]);
                let vx = seg.p1_m[0] - seg.p0_m[0];
                let vy = seg.p1_m[1] - seg.p0_m[1];
                let vlen = (vx * vx + vy * vy).sqrt().max(1.0);
                let bank_d = (seg.channel_width_m as f64) * 0.75 + 25.0;
                for &side in &[-1.0f64, 1.0] {
                    let bx = mx + (-vy / vlen) * side * bank_d;
                    let by = my + (vx / vlen) * side * bank_d;
                    let col = synthesize_column(
                        world.anchor(),
                        world.stratigraphy(),
                        world.drainage(),
                        world.volcanic_field(),
                        bx,
                        by,
                    );
                    if !col.is_ocean && !col.is_river && !col.is_lake {
                        if let Some(rh_bank) = world.drainage().query_rivers(bx, by, 0.0, true) {
                            assert!(
                                col.surface_elev_m >= rh_bank.z_water_m,
                                "Bank dipped below nearest river z_water on tile {tid}: bank={} < z_w={}",
                                col.surface_elev_m,
                                rh_bank.z_water_m
                            );
                        }
                    }
                }
                verified_reaches += 1;
            }
            if verified_reaches >= 40 {
                break;
            }
        }
        assert!(verified_reaches > 0, "Expected to verify at least one river reach");
    }

    #[test]
    fn volcano_level3_conduit_dike_hornfels_sulfur_and_lava_tubes() {
        // 1. Subduction andesite stratovolcano -> conduit, HORNFELS halo, and SULFUR_DEPOSIT crust
        let andesite_anchor = LocalZoomAnchor {
            world_seed: 2025,
            stage: SnapshotStage::Prehistory,
            grid_size: 6,
            radius_km: 6371.0,
            parent_tile_id: 7,
            parent_tile_radius_m: 80_000.0,
            anchor_normal: [0.0, 1.0, 0.0],
            tangent_east: [1.0, 0.0, 0.0],
            tangent_north: [0.0, 0.0, 1.0],
            sea_level_m: 0.0,
            has_atmosphere: true,
            has_life: true,
            is_pentagon: false,
            neighbors: vec![NeighborTileSample {
                tile_id: 7,
                local_xy_m: [0.0, 0.0],
                elevation_m: 600.0,
                bottom_m: -9000.0,
                mountainosity: 0.85,
                subduction: 0.55,
                has_intrusion: true,
                impact_scar: 0.0,
                soil_depth_m: 0.8,
                soil_fertility: 1.0,
                is_glaciated: false,
                glacier_cover: 0.0,
                annual_temp_c: 10.0,
                precip_norm: 0.65,
                tidal_range_m: 1.0,
                wind_vec: [1.0, 0.0],
                layers: vec![RockLayer {
                    rock: Rock::extrusive(0.60),
                    thickness: 9.6,
                }],
                occurrences: vec![],
            }],
            river_edges: vec![],
            fine_samples: vec![],
            fine_spacing_m: 0.0,
            tile_frames: vec![TileCanonicalFrame {
                tile_id: 7,
                east_2d: [1.0, 0.0],
                north_2d: [0.0, 1.0],
            }],
        };

        let world = LocalTerrainWorld::new(andesite_anchor, LocalVolumeConfig::default());
        assert_eq!(world.volcanoes().len(), 1);
        let ed = &world.volcanoes()[0];
        assert_eq!(ed.kind, VolcanoKind::Stratovolcano);

        // Center column must sit in the magmatic feeder conduit:
        // shallow (< 55 m) is solidified ANDESITE lava, deeper (>= 55 m) transitions to DIORITE pluton
        let center_col = synthesize_column(
            world.anchor(),
            world.stratigraphy(),
            world.drainage(),
            world.volcanic_field(),
            ed.center_xy_m[0],
            ed.center_xy_m[1],
        );
        let shallow_conduit = sample_cell_at_z(
            world.anchor(),
            world.stratigraphy(),
            &center_col,
            ed.center_xy_m[0],
            ed.center_xy_m[1],
            center_col.surface_elev_m - 25.0,
        );
        assert_eq!(
            shallow_conduit.material,
            MaterialId::ANDESITE,
            "Expected shallow volcanic conduit to be solidified ANDESITE"
        );
        let deep_conduit = sample_cell_at_z(
            world.anchor(),
            world.stratigraphy(),
            &center_col,
            ed.center_xy_m[0],
            ed.center_xy_m[1],
            center_col.surface_elev_m - 75.0,
        );
        assert_eq!(
            deep_conduit.material,
            MaterialId::DIORITE,
            "Expected deep volcanic conduit to transition to DIORITE pluton"
        );

        // Just outside conduit radius must hit the HORNFELS contact-metamorphic halo
        let halo_x = ed.center_xy_m[0] + (ed.conduit_radius_m as f64) + 12.0;
        let halo_y = ed.center_xy_m[1];
        let halo_col = synthesize_column(
            world.anchor(),
            world.stratigraphy(),
            world.drainage(),
            world.volcanic_field(),
            halo_x,
            halo_y,
        );
        let halo_cell = sample_cell_at_z(
            world.anchor(),
            world.stratigraphy(),
            &halo_col,
            halo_x,
            halo_y,
            halo_col.surface_elev_m - 25.0,
        );
        assert_eq!(
            halo_cell.material,
            MaterialId::HORNFELS,
            "Expected HORNFELS contact-metamorphic halo around conduit"
        );

        // Scan around the summit rim / caldera edge for fumarolic SULFUR_DEPOSIT crust
        let mut sulfur_hits = 0usize;
        for i in 0..48 {
            let ang = (i as f64) * std::f64::consts::TAU / 48.0;
            for &r_mult in &[0.75, 0.92, 1.0, 1.08] {
                let r = (ed.caldera_radius_m as f64).max(ed.conduit_radius_m as f64) * r_mult;
                let px = ed.center_xy_m[0] + ang.cos() * r;
                let py = ed.center_xy_m[1] + ang.sin() * r;
                let col = synthesize_column(
                    world.anchor(),
                    world.stratigraphy(),
                    world.drainage(),
                    world.volcanic_field(),
                    px,
                    py,
                );
                if col.surficial_material == MaterialId::SULFUR_DEPOSIT {
                    sulfur_hits += 1;
                }
            }
        }
        assert!(
            sulfur_hits > 0,
            "Expected fumarolic SULFUR_DEPOSIT patches near active stratovolcano rim"
        );

        // 2. Basalt shield volcano flank -> hollow 3D lava tubes with NATURAL_CAVE flag
        let shield_anchor = LocalZoomAnchor {
            world_seed: 4040,
            stage: SnapshotStage::Prehistory,
            grid_size: 6,
            radius_km: 6371.0,
            parent_tile_id: 9,
            parent_tile_radius_m: 80_000.0,
            anchor_normal: [0.0, 1.0, 0.0],
            tangent_east: [1.0, 0.0, 0.0],
            tangent_north: [0.0, 0.0, 1.0],
            sea_level_m: 0.0,
            has_atmosphere: true,
            has_life: true,
            is_pentagon: false,
            neighbors: vec![NeighborTileSample {
                tile_id: 9,
                local_xy_m: [0.0, 0.0],
                elevation_m: 400.0,
                bottom_m: -9000.0,
                mountainosity: 0.65,
                subduction: 0.0,
                has_intrusion: true,
                impact_scar: 0.0,
                soil_depth_m: 0.5,
                soil_fertility: 1.0,
                is_glaciated: false,
                glacier_cover: 0.0,
                annual_temp_c: 18.0,
                precip_norm: 0.55,
                tidal_range_m: 1.0,
                wind_vec: [1.0, 0.0],
                layers: vec![RockLayer {
                    rock: Rock::extrusive(0.45),
                    thickness: 9.4,
                }],
                occurrences: vec![],
            }],
            river_edges: vec![],
            fine_samples: vec![],
            fine_spacing_m: 0.0,
            tile_frames: vec![TileCanonicalFrame {
                tile_id: 9,
                east_2d: [1.0, 0.0],
                north_2d: [0.0, 1.0],
            }],
        };

        let shield_world = LocalTerrainWorld::new(shield_anchor, LocalVolumeConfig::default());
        assert_eq!(shield_world.volcanoes().len(), 1);
        let sed = &shield_world.volcanoes()[0];
        assert_eq!(sed.kind, VolcanoKind::ShieldVolcano);

        let flank_base_x = sed.center_xy_m[0] + (sed.base_radius_m as f64) * 0.40;
        let flank_base_y = sed.center_xy_m[1];
        let mut lava_tube_cells = 0usize;
        for ix in 0..24 {
            for iy in 0..24 {
                let x = flank_base_x + (ix as f64) * 3.0;
                let y = flank_base_y + (iy as f64) * 3.0;
                let col = synthesize_column(
                    shield_world.anchor(),
                    shield_world.stratigraphy(),
                    shield_world.drainage(),
                    shield_world.volcanic_field(),
                    x,
                    y,
                );
                for dz in 5..26 {
                    let c = sample_cell_at_z(
                        shield_world.anchor(),
                        shield_world.stratigraphy(),
                        &col,
                        x,
                        y,
                        col.surface_elev_m - (dz as f32),
                    );
                    if c.is_air() && (c.flags & cell_flags::NATURAL_CAVE) != 0 {
                        lava_tube_cells += 1;
                    }
                }
            }
        }
        assert!(
            lava_tube_cells > 20,
            "Expected hollow 3D basalt lava tubes on shield volcano flank (found {lava_tube_cells})"
        );
    }

    #[test]
    fn volcano_reanchor_invariance_and_pois() {
        let mut sim = PrehistorySimulation::new_earth(42);
        for _ in 0..4 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();
        let ctx = PlanetaryTerrainContext::from_snapshot_bytes(&bytes)
            .expect("PlanetaryTerrainContext should decode");

        // Find a tile that hosts a volcanic edifice, verify its POI and re-anchor invariance
        let mut checked_volcano = false;
        for tid in 0..sim.tiles().len() {
            let anchor_a = ctx.build_anchor(tid).expect("Anchor A should build");
            let world_a = LocalTerrainWorld::new(anchor_a, LocalVolumeConfig::default());
            let Some(ed_a) = world_a.volcanoes().iter().find(|e| e.tile_id == tid) else {
                continue;
            };

            // Verify FR-007 volcanic POI is emitted
            let pois = world_a.find_points_of_interest();
            assert!(
                pois.iter().any(|p| p.kind == "volcano"),
                "Tile {tid} with edifice {:?} should emit a 'volcano' POI",
                ed_a.kind
            );

            // Re-anchor onto a neighboring tile that still contains `tid` in its 2-ring
            let nb_tid = world_a.anchor().neighbors[1].tile_id;
            let anchor_b = ctx.build_anchor(nb_tid).expect("Anchor B should build");
            let world_b = LocalTerrainWorld::new(anchor_b, LocalVolumeConfig::default());
            let Some(ed_b) = world_b.volcanoes().iter().find(|e| e.tile_id == tid) else {
                continue;
            };

            // Both anchors must derive identical volcanic morphology and dimensions
            assert_eq!(ed_a.kind, ed_b.kind);
            assert!((ed_a.base_radius_m - ed_b.base_radius_m).abs() < 1e-3);
            assert!((ed_a.peak_height_m - ed_b.peak_height_m).abs() < 1e-3);

            // Evaluate the volcanic rim point in each anchor's own edifice coordinates (`SC-004`):
            // canonical frame ensures < 0.01 m elevation invariance across coarse-tile re-anchoring.
            let rim_a = ed_a.poi_rim_xy_m();
            let rim_b = ed_b.poi_rim_xy_m();
            let q_a = world_a.volcanic_field().query(
                rim_a[0],
                rim_a[1],
                world_a.anchor().has_atmosphere,
                world_a.anchor().world_seed,
            );
            let q_b = world_b.volcanic_field().query(
                rim_b[0],
                rim_b[1],
                world_b.anchor().has_atmosphere,
                world_b.anchor().world_seed,
            );
            let da = q_a.map(|q| q.delta_elev_m).unwrap_or(0.0);
            let db = q_b.map(|q| q.delta_elev_m).unwrap_or(0.0);
            assert!(
                (da - db).abs() < 0.01,
                "Volcanic rim delta across adjacent anchors drifted: A={da:.4} m vs B={db:.4} m"
            );
            checked_volcano = true;
            break;
        }
        assert!(
            checked_volcano,
            "Expected at least one active volcanic tile in Earth Prehistory simulation"
        );
    }

    #[test]
    fn crater_lakes_sit_strictly_below_caldera_rim() {
        let mut sim = PrehistorySimulation::new_earth(42);
        for _ in 0..4 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();
        let ctx = PlanetaryTerrainContext::from_snapshot_bytes(&bytes)
            .expect("PlanetaryTerrainContext should decode");

        let mut checked_lakes = 0usize;
        for tid in 0..sim.tiles().len() {
            let anchor = ctx.build_anchor(tid).expect("Anchor should build");
            let world = LocalTerrainWorld::new(anchor, LocalVolumeConfig::default());
            for ed in world.volcanoes() {
                if ed.tile_id != tid {
                    continue;
                }
                let Some(z_lake) = ed.crater_lake_z_m else {
                    continue;
                };
                if ed.crater_lake_state != Some(LakeState::Freshwater) {
                    continue;
                }
                // Check 32 azimuths around the full 360° caldera rim: every rim point must sit
                // strictly ABOVE the crater lake water surface (never a raised water plateau!).
                let mut min_rim = f32::INFINITY;
                for i in 0..32 {
                    let ang = (i as f64) * std::f64::consts::TAU / 32.0;
                    let rx = ed.center_xy_m[0] + ang.cos() * (ed.caldera_radius_m as f64);
                    let ry = ed.center_xy_m[1] + ang.sin() * (ed.caldera_radius_m as f64);
                    let col = synthesize_column(
                        world.anchor(),
                        world.stratigraphy(),
                        world.drainage(),
                        world.volcanic_field(),
                        rx,
                        ry,
                    );
                    if !col.is_river && !col.is_ocean {
                        assert!(
                            col.surface_elev_m > z_lake + 2.0,
                            "Crater lake on tile {tid} ({:?}) had water ({z_lake:.1} m) above rim ({:.1} m) at angle {i}",
                            ed.kind,
                            col.surface_elev_m
                        );
                        min_rim = min_rim.min(col.surface_elev_m);
                    }
                }
                // Check that the caldera floor inside the bowl is strictly below the rim
                let fx = ed.center_xy_m[0] + (ed.caldera_radius_m as f64) * 0.55;
                let fy = ed.center_xy_m[1];
                let floor_col = synthesize_column(
                    world.anchor(),
                    world.stratigraphy(),
                    world.drainage(),
                    world.volcanic_field(),
                    fx,
                    fy,
                );
                assert!(
                    floor_col.surface_elev_m < min_rim - 40.0,
                    "Caldera floor ({:.1} m) should sit well below rim ({min_rim:.1} m) on tile {tid} ({:?})",
                    floor_col.surface_elev_m,
                    ed.kind
                );
                checked_lakes += 1;
            }
            if checked_lakes >= 8 {
                break;
            }
        }
        assert!(
            checked_lakes > 0,
            "Expected to verify at least one crater lake in Earth Prehistory simulation"
        );
    }
}
