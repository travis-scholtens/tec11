//! Immutable planetary anchor (`LocalZoomAnchor`) extracted from `.tec11` `PlanetSnapshot`
//! bytes across all three simulation stages (`Tectonics`, `Prehistory`, `History`).

use std::f64::consts::PI;

use tec11_climate::{nominal_cells_for_spin, ClimateModel};
use tec11_geology::rock::RockLayer;
use tec11_geology::{trace_coarse_rivers, PangeaWorld, TileGeology};
use tec11_math_grid::SphericalGrid;
use tec11_proto::{Message as _, PlanetSnapshot, PlanetStageOneof};

use crate::materials::MaterialId;
use crate::LocalTerrainError;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SnapshotStage {
    Tectonics,
    Prehistory,
    History,
}

impl SnapshotStage {
    pub fn name(self) -> &'static str {
        match self {
            Self::Tectonics => "Tectonics",
            Self::Prehistory => "Prehistory",
            Self::History => "History",
        }
    }
}

/// Mineralized bed/interface hint extracted from `TileGeology::derive_layer_occurrences`.
#[derive(Clone, Debug)]
pub struct LayerMineralHint {
    pub layer_index_from_top: usize,
    pub material: MaterialId,
    pub grade: f32,
    pub abundance: f32,
    pub is_interface: bool,
}

/// Parent river segment crossing or entering the anchor's 1-ring neighborhood.
#[derive(Clone, Debug)]
pub struct CoarseRiverEdge {
    pub edge_id: u64,
    pub from_xy_m: [f64; 2],
    pub to_xy_m: [f64; 2],
    pub from_elev_m: f32,
    pub to_elev_m: f32,
    pub discharge: f32,
}

/// Single tile sample (anchor center or 1-ring neighbor) projected into the local tangent plane.
#[derive(Clone, Debug)]
pub struct NeighborTileSample {
    pub tile_id: usize,
    pub local_xy_m: [f64; 2],
    pub elevation_m: f32,
    pub bottom_m: f32,
    pub mountainosity: f32,
    pub subduction: f32,
    pub impact_scar: f32,
    pub soil_depth_m: f32,
    pub soil_fertility: f32,
    pub is_glaciated: bool,
    pub glacier_cover: f32,
    pub annual_temp_c: f32,
    pub precip_norm: f32,
    pub tidal_range_m: f32,
    pub wind_vec: [f32; 2],
    pub layers: Vec<RockLayer>,
    pub occurrences: Vec<LayerMineralHint>,
}

/// Immutable planetary reference extracted from `.tec11` snapshot bytes (`data-model.md`).
#[derive(Clone, Debug)]
pub struct LocalZoomAnchor {
    pub world_seed: u64,
    pub stage: SnapshotStage,
    pub grid_size: u32,
    pub radius_km: f64,
    pub parent_tile_id: usize,
    pub parent_tile_radius_m: f64,
    pub anchor_normal: [f64; 3],
    pub tangent_east: [f64; 3],
    pub tangent_north: [f64; 3],
    pub sea_level_m: f64,
    pub has_atmosphere: bool,
    pub has_life: bool,
    pub is_pentagon: bool,
    pub neighbors: Vec<NeighborTileSample>,
    pub river_edges: Vec<CoarseRiverEdge>,
}

impl LocalZoomAnchor {
    /// Load an immutable anchor from `.tec11` `PlanetSnapshot` bytes (`Tectonics`, `Prehistory`, or `History`).
    pub fn from_snapshot_bytes(
        snapshot_bytes: &[u8],
        parent_tile_id: usize,
    ) -> Result<Self, LocalTerrainError> {
        let snap = PlanetSnapshot::decode(snapshot_bytes)
            .map_err(|e| LocalTerrainError::SnapshotDecode(e.to_string()))?;
        Self::from_snapshot(&snap, parent_tile_id)
    }

    /// Load an immutable anchor directly from a decoded `PlanetSnapshot`.
    pub fn from_snapshot(
        snap: &PlanetSnapshot,
        parent_tile_id: usize,
    ) -> Result<Self, LocalTerrainError> {
        let grid_size = snap.grid_size.clamp(4, 8);
        let grid = SphericalGrid::with_size(grid_size);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        if parent_tile_id >= n {
            return Err(LocalTerrainError::InvalidTileId {
                tile_id: parent_tile_id,
                tile_count: n,
            });
        }

        let radius_km = if snap.radius_km > 100.0 {
            snap.radius_km as f64
        } else {
            PangeaWorld::planet_radius_km(grid_size)
        };
        let spin = if snap.spin > 0.01 {
            snap.spin as f64
        } else {
            1.0
        };
        let tilt_deg = if snap.tilt_deg >= 0.0 {
            snap.tilt_deg as f64
        } else {
            23.0
        };

        let world_seed = hash_bytes_fnv1a(&snap.rng_state_16b);

        let (stage, geo_tiles, has_atmosphere, has_life, glaciation, sea_level_m) =
            match snap.stage.as_ref() {
                Some(PlanetStageOneof::Tectonics(tec)) => {
                    let tiles: Vec<TileGeology> = tec
                        .tiles
                        .iter()
                        .filter_map(TileGeology::from_proto_geological_tile)
                        .collect();
                    let env_atm = snap
                        .environment
                        .as_ref()
                        .map(|e| e.has_atmosphere)
                        .unwrap_or(tec.atmosphere_ticks_remaining == 0);
                    let env_life = snap
                        .environment
                        .as_ref()
                        .map(|e| e.has_life)
                        .unwrap_or(tec.life_ticks_remaining == 0);
                    let has_atm = env_atm && tec.atmosphere_ticks_remaining == 0;
                    let has_l = has_atm && env_life && tec.life_ticks_remaining == 0;
                    (SnapshotStage::Tectonics, tiles, has_atm, has_l, 0.5, 0.0)
                }
                Some(PlanetStageOneof::Prehistory(pre)) => {
                    let tiles: Vec<TileGeology> = pre
                        .tiles
                        .iter()
                        .filter_map(|pt| pt.geology.as_ref())
                        .filter_map(TileGeology::from_proto_geological_tile)
                        .collect();
                    let g = prehistory_glaciation_for_tick(pre.glaciation_tick);
                    let sea_m = eustatic_sea_level_m(g);
                    (SnapshotStage::Prehistory, tiles, true, true, g, sea_m)
                }
                Some(PlanetStageOneof::History(hist)) => {
                    let pre = hist
                        .prehistory_base
                        .as_ref()
                        .ok_or_else(|| LocalTerrainError::MissingStageData("HistoryStage missing prehistory_base"))?;
                    let tiles: Vec<TileGeology> = pre
                        .tiles
                        .iter()
                        .filter_map(|pt| pt.geology.as_ref())
                        .filter_map(TileGeology::from_proto_geological_tile)
                        .collect();
                    let g = prehistory_glaciation_for_tick(pre.glaciation_tick);
                    (SnapshotStage::History, tiles, true, true, g, 0.0)
                }
                None => {
                    return Err(LocalTerrainError::MissingStageData("PlanetSnapshot missing stage"));
                }
            };

        if geo_tiles.len() != n {
            return Err(LocalTerrainError::TileCountMismatch {
                expected: n,
                actual: geo_tiles.len(),
            });
        }

        // Compute orthonormal local tangent frame at `parent_tile_id`
        let anchor_normal = normalize3(grid.faces[parent_tile_id].0);
        let (tangent_east, tangent_north) = build_tangent_basis(anchor_normal);

        // Estimate physical tile radius from average angular distance to 1-ring neighbors
        let nb_ids = &adjacency[parent_tile_id];
        let is_pentagon = nb_ids.len() == 5;
        let radius_m = radius_km * 1000.0;
        let mut avg_dist_m = 0.0f64;
        for &nb in nb_ids {
            let p = normalize3(grid.faces[nb].0);
            let ang = dot3(anchor_normal, p).clamp(-1.0, 1.0).acos();
            avg_dist_m += ang * radius_m;
        }
        let parent_tile_radius_m = if !nb_ids.is_empty() {
            (avg_dist_m / (nb_ids.len() as f64)) * 0.55
        } else {
            120_000.0
        };

        // Derive physical climate & rivers if atmosphere is present
        let elevs_km: Vec<f32> = geo_tiles
            .iter()
            .map(|t| (t.elevation() - (sea_level_m / 1000.0) as f32).max(0.0))
            .collect();
        let signed_elevs_km: Vec<f32> = geo_tiles
            .iter()
            .map(|t| t.elevation() - (sea_level_m / 1000.0) as f32)
            .collect();

        let (climate_opt, rivers_raw) = if has_atmosphere {
            let cells = nominal_cells_for_spin(spin);
            let mut cm = ClimateModel::new_for_tectonics_with_adjacency(
                grid.clone(),
                adjacency.clone(),
                cells,
                spin,
                tilt_deg,
            );
            cm.set_glaciation_only(glaciation);
            cm.update_for_glaciation_step(&elevs_km, has_life);
            let precip_norm: Vec<f32> = cm
                .classifications()
                .iter()
                .map(|c| (c.mean_precipitation as f32).clamp(0.0, 1.0))
                .collect();
            let rivers = trace_coarse_rivers(
                &signed_elevs_km,
                &precip_norm,
                cm.tile_glacier_cover(),
                &adjacency,
                2.0,
                0.25,
            );
            (Some(cm), rivers)
        } else {
            (None, Vec::new())
        };

        // Moon tidal amplitude proxy
        let tidal_base_m = snap
            .environment
            .as_ref()
            .map(|e| {
                if e.moon_count == 0 {
                    0.45f32
                } else {
                    let dist = e.current_moon_dist_radii.max(12.0);
                    let lunar = (e.moon_mass_lunar.max(0.05) * (60.0 / dist).powi(3)) as f32;
                    (0.5 + 1.8 * lunar).clamp(0.4, 8.0)
                }
            })
            .unwrap_or(1.8);

        // Collect center tile + 1-ring neighbors (5 for pentagon, 6 for hexagon)
        let mut sample_ids = Vec::with_capacity(1 + nb_ids.len());
        sample_ids.push(parent_tile_id);
        sample_ids.extend_from_slice(nb_ids);

        let project_xy = |tid: usize| -> [f64; 2] {
            let p = normalize3(grid.faces[tid].0);
            let dot = dot3(anchor_normal, p).clamp(-1.0, 1.0);
            let ang = dot.acos();
            if ang < 1e-12 {
                return [0.0, 0.0];
            }
            let diff = [
                p[0] - dot * anchor_normal[0],
                p[1] - dot * anchor_normal[1],
                p[2] - dot * anchor_normal[2],
            ];
            let dlen = (diff[0] * diff[0] + diff[1] * diff[1] + diff[2] * diff[2]).sqrt().max(1e-12);
            let dir = [diff[0] / dlen, diff[1] / dlen, diff[2] / dlen];
            let arc_m = ang * radius_m;
            [dot3(dir, tangent_east) * arc_m, dot3(dir, tangent_north) * arc_m]
        };

        let mut neighbors = Vec::with_capacity(sample_ids.len());
        for &tid in &sample_ids {
            let tile = &geo_tiles[tid];
            let elev_m = tile.elevation() * 1000.0;
            let bottom_m = tile.bottom * 1000.0;

            let (is_glac, glac_cov, temp_c, precip, wind_xy) = if let Some(ref cm) = climate_opt {
                let cls = &cm.classifications()[tid];
                let w3 = cm.wind_vector_xyz(tid);
                let we = (w3[0] as f64) * tangent_east[0]
                    + (w3[1] as f64) * tangent_east[1]
                    + (w3[2] as f64) * tangent_east[2];
                let wn = (w3[0] as f64) * tangent_north[0]
                    + (w3[1] as f64) * tangent_north[1]
                    + (w3[2] as f64) * tangent_north[2];
                (
                    cm.is_tile_glaciated(tid),
                    cm.tile_glacier_cover().get(tid).copied().unwrap_or(0.0),
                    cls.mean_temperature as f32,
                    (cls.mean_precipitation as f32).clamp(0.0, 1.0),
                    [we as f32, wn as f32],
                )
            } else {
                (false, 0.0, 20.0, 0.0, [0.0, 0.0])
            };

            // Extract mineral vein hints from TileGeology::derive_layer_occurrences
            let mut occurrences = Vec::new();
            let mut depth_km = 0.0f32;
            for (top_down_idx, (idx, layer)) in tile.layers.iter().enumerate().rev().enumerate() {
                let occs = tile.derive_layer_occurrences(idx, depth_km);
                for occ in occs {
                    if let Some((mk, _)) = occ.minerals.first() {
                        occurrences.push(LayerMineralHint {
                            layer_index_from_top: top_down_idx,
                            material: MaterialId::from_mineral_code(mk.code(), &occ.label),
                            grade: occ.grade,
                            abundance: occ.abundance,
                            is_interface: occ.is_interface,
                        });
                    }
                }
                depth_km += layer.thickness;
            }

            neighbors.push(NeighborTileSample {
                tile_id: tid,
                local_xy_m: project_xy(tid),
                elevation_m: elev_m,
                bottom_m,
                mountainosity: tile.mountainosity,
                subduction: tile.subduction,
                impact_scar: tile.impact_scar,
                soil_depth_m: if has_atmosphere { tile.soil_depth_m } else { 0.0 },
                soil_fertility: tile.soil_fertility,
                is_glaciated: is_glac,
                glacier_cover: glac_cov,
                annual_temp_c: temp_c,
                precip_norm: precip,
                tidal_range_m: if has_atmosphere { tidal_base_m } else { 0.0 },
                wind_vec: wind_xy,
                layers: tile.layers.clone(),
                occurrences,
            });
        }

        // Extract any coarse river edges touching the anchor or its 1-ring neighbors
        let mut river_edges = Vec::new();
        for (r_idx, path) in rivers_raw.iter().enumerate() {
            for w in path.windows(2) {
                let a = w[0];
                let b = w[1];
                if sample_ids.contains(&a) || sample_ids.contains(&b) {
                    let from_xy_m = project_xy(a);
                    let to_xy_m = project_xy(b);
                    let from_elev_m = (geo_tiles[a].elevation() * 1000.0).max(sea_level_m as f32);
                    let to_elev_m = (geo_tiles[b].elevation() * 1000.0)
                        .max(sea_level_m as f32)
                        .min(from_elev_m);
                    let step_pos = path.iter().position(|&x| x == a).unwrap_or(0) as f32;
                    let discharge = (1.0 + step_pos * 0.6).min(12.0);
                    let edge_id = ((r_idx as u64) << 32) ^ ((a as u64) << 16) ^ (b as u64);
                    river_edges.push(CoarseRiverEdge {
                        edge_id,
                        from_xy_m,
                        to_xy_m,
                        from_elev_m,
                        to_elev_m,
                        discharge,
                    });
                }
            }
        }

        Ok(Self {
            world_seed,
            stage,
            grid_size,
            radius_km,
            parent_tile_id,
            parent_tile_radius_m,
            anchor_normal,
            tangent_east,
            tangent_north,
            sea_level_m,
            has_atmosphere,
            has_life,
            is_pentagon,
            neighbors,
            river_edges,
        })
    }

    /// Deterministic hash identifying `(world_seed, stage, parent_tile_id)`.
    pub fn anchor_hash(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for b in self.world_seed.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= self.stage as u64;
        h = h.wrapping_mul(0x100000001b3);
        for b in (self.parent_tile_id as u64).to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    #[inline]
    pub fn center(&self) -> &NeighborTileSample {
        &self.neighbors[0]
    }

    /// Apply a fine-tile surface elevation override (e.g., an emergent oceanic volcanic/atoll
    /// island in History whose coarse parent tile is submarine, or a specific fine tile elevation).
    pub fn apply_surface_elevation_override(&mut self, surface_elev_km: f32) {
        if !surface_elev_km.is_finite() || self.neighbors.is_empty() {
            return;
        }
        let override_m = surface_elev_km * 1000.0;
        let was_submarine = self.neighbors[0].elevation_m <= (self.sea_level_m as f32);
        self.neighbors[0].elevation_m = override_m;
        if was_submarine && override_m > (self.sea_level_m as f32) {
            // Emergent sub-grid oceanic island: tighten neighbor radius so the volcanic/atoll
            // island rises out of the surrounding ocean at realistic fine-tile (~30-45 km) scale.
            let scale = 0.36f64;
            self.parent_tile_radius_m *= scale;
            for nb in self.neighbors.iter_mut().skip(1) {
                nb.local_xy_m[0] *= scale;
                nb.local_xy_m[1] *= scale;
            }
            if self.has_atmosphere && self.neighbors[0].soil_depth_m < 0.5 {
                self.neighbors[0].soil_depth_m = 1.5;
            }
        }
    }
}

fn prehistory_glaciation_for_tick(glaciation_tick: u32) -> f64 {
    const STEP: f64 = 16.0;
    0.5 - ((glaciation_tick as f64) * PI / STEP).cos() / 2.0
}

fn eustatic_sea_level_m(glaciation: f64) -> f64 {
    // Matches PrehistorySimulation::compute_eustatic_sea_level: -120 m at LGM (g=0), 0 m at interglacial (g>=0.5)
    let drop_frac = ((0.5 - glaciation) / 0.5).clamp(0.0, 1.0);
    -120.0 * drop_frac
}

fn hash_bytes_fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn build_tangent_basis(normal: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    // Prefer polar north [0, 1, 0] unless too close to pole, then fall back to [1, 0, 0]
    let ref_up = if normal[1].abs() < 0.92 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let east = normalize3(cross3(ref_up, normal));
    let north = normalize3(cross3(normal, east));
    (east, north)
}
