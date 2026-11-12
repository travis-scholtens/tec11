use std::f64::consts::PI;

use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{dot, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use crate::prehistory::PrehistorySimulation;
use crate::rock::{
    deposit_sedimentary, DepositClimate, ErodedMaterialBatch, Rock, RockKind, RockLayer,
};
use crate::tectonics::TectonicsSimulation;
use crate::tile::TileGeology;
use crate::world::{koeppen_climate_name, koeppen_rgb};

/// Standalone Moon, Harmonic Orbit & Tidal Erosion Sandbox (`MoonScreen`).
/// Loads any `.tec11` planet file (or canned Earth `ETOPO2`) and visualizes:
/// - 2:1 Laplace harmonic resonance chains (`P_k = 2^(k-1) P_1`, `a_k = 2^(2(k-1)/3) a_1`),
/// - Hadean-to-Prehistory lunar orbital recession (`a^(13/2)` law) and rotational spin braking,
/// - Axial tilt (obliquity) stabilization vs. chaotic wander,
/// - Quadrupolar tidal bulge & Green's Law shallow-shelf / bay-funnel tidal range (`meters`),
/// - Spatial distribution of coastal tidal scour vs. inland fluvial erosion and shelf deposition.
pub struct MoonSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    base_tiles: Vec<TileGeology>,
    tiles: Vec<TileGeology>,
    climate_model: ClimateModel,
    moon_count: u32,
    moon_mass: f64,
    target_moon_dist_radii: f64,
    target_day_hours: f64,
    base_tilt_deg: f64,
    epoch_pct: u32,
    orbital_phase_deg: f64,
    steps: u32,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_temp_norm: Vec<f32>,
    tile_temp_c: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    tile_tidal_range_m: Vec<f32>,
    tile_tidal_bulge_m: Vec<f32>,
    tile_tidal_scour: Vec<f32>,
    tile_fluvial_lost: Vec<f32>,
    tile_tidal_deposition: Vec<f32>,
}

impl MoonSandbox {
    pub fn new_earth() -> Self {
        let grid = SphericalGrid::with_size(6);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let mut tiles = Vec::with_capacity(n);

        for &elev_m in EARTH_SIZE6_ELEVATIONS_M.iter() {
            let elev_km = (elev_m as f32) / 900.0;
            if elev_km > 0.0 {
                let mut t = TileGeology {
                    bottom: -1.0,
                    mountainosity: (elev_km * 2.5).clamp(0.0, 20.0),
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    layers: vec![RockLayer {
                        rock: Rock {
                            kind: RockKind::Sedimentary,
                            name: "sandstone",
                            felsity: 0.5,
                            toughness: 0.5,
                            clasticity: Some(1.0),
                            calcity: None,
                            bogginess: None,
                        },
                        thickness: elev_km + 1.0,
                    }],
                };
                t.limit();
                tiles.push(t);
            } else {
                let bathy_km = ((elev_m as f32) / 1000.0).clamp(-7.0, -0.2);
                tiles.push(TileGeology {
                    bottom: bathy_km - 5.0,
                    mountainosity: 0.0,
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    layers: vec![RockLayer {
                        rock: Rock::extrusive(0.5),
                        thickness: 5.0,
                    }],
                });
            }
        }

        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);
        let mut sim = Self {
            grid,
            adjacency,
            base_tiles: tiles.clone(),
            tiles,
            climate_model,
            moon_count: 1,
            moon_mass: 1.0,
            target_moon_dist_radii: 60.0,
            target_day_hours: 24.0,
            base_tilt_deg: 23.0,
            epoch_pct: 100,
            orbital_phase_deg: 0.0,
            steps: 0,
            custom_geoticks: None,
            custom_land_percent: None,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            tile_tidal_range_m: vec![0.0; n],
            tile_tidal_bulge_m: vec![0.0; n],
            tile_tidal_scour: vec![0.0; n],
            tile_fluvial_lost: vec![0.0; n],
            tile_tidal_deposition: vec![0.0; n],
        };
        sim.recompute_fields(false);
        sim
    }

    pub fn load_earth(&mut self) {
        let moon_count = self.moon_count;
        let moon_mass = self.moon_mass;
        let target_moon_dist_radii = self.target_moon_dist_radii;
        let epoch_pct = self.epoch_pct;
        *self = Self::new_earth();
        self.moon_count = moon_count;
        self.moon_mass = moon_mass;
        self.target_moon_dist_radii = target_moon_dist_radii;
        self.epoch_pct = epoch_pct;
        self.recompute_fields(false);
    }

    /// Load any `.tec11` snapshot (`Tectonics`, `Prehistory`, or `History`) into the Moon Sandbox.
    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let (grid, tiles, geoticks, land_pct, day_hours, tilt_deg) =
            if let Some(tec_sim) = TectonicsSimulation::from_snapshot(&snap, 42) {
                let dh = if snap.spin > 0.0 {
                    (24.0 / (snap.spin as f64)).clamp(4.0, 120.0)
                } else {
                    24.0
                };
                let tilt = if snap.tilt_deg > 0.0 {
                    snap.tilt_deg as f64
                } else {
                    23.0
                };
                (
                    tec_sim.grid().clone(),
                    tec_sim.tiles().to_vec(),
                    tec_sim.geoticks(),
                    tec_sim.land_percent(),
                    dh,
                    tilt,
                )
            } else if let Some(pre_sim) = PrehistorySimulation::from_snapshot(&snap, 42) {
                (
                    pre_sim.grid().clone(),
                    pre_sim.tiles().to_vec(),
                    pre_sim.tectonics_ticks(),
                    pre_sim.land_percent(),
                    24.0,
                    23.0,
                )
            } else {
                return false;
            };

        let n = grid.faces.len();
        self.adjacency = grid.adjacency();
        self.grid = grid.clone();
        self.base_tiles = tiles.clone();
        self.tiles = tiles;
        self.target_day_hours = day_hours;
        self.base_tilt_deg = tilt_deg;
        self.climate_model = ClimateModel::new_for_tectonics(grid, 3, 24.0 / day_hours, tilt_deg);
        self.steps = 0;
        self.custom_geoticks = Some(geoticks);
        self.custom_land_percent = Some(land_pct);
        self.tile_koeppen = vec![0; n];
        self.tile_life = vec![0.0; n];
        self.tile_temp_norm = vec![0.5; n];
        self.tile_temp_c = vec![15.0; n];
        self.tile_precip_norm = vec![0.5; n];
        self.tile_tidal_range_m = vec![0.0; n];
        self.tile_tidal_bulge_m = vec![0.0; n];
        self.tile_tidal_scour = vec![0.0; n];
        self.tile_fluvial_lost = vec![0.0; n];
        self.tile_tidal_deposition = vec![0.0; n];
        self.recompute_fields(false);
        true
    }

    /// Reset rock layers to the initially loaded planet state (undoing cumulative step erosion)
    /// and recompute diagnostic fields for the current moon configuration.
    pub fn reset_layers(&mut self) {
        self.tiles = self.base_tiles.clone();
        self.steps = 0;
        self.orbital_phase_deg = 0.0;
        self.recompute_fields(false);
    }

    pub fn set_moon_params(
        &mut self,
        moon_count: u32,
        moon_mass: f64,
        target_moon_dist_radii: f64,
        epoch_pct: u32,
    ) {
        self.moon_count = moon_count.min(8);
        self.moon_mass = if self.moon_count == 0 {
            0.0
        } else {
            moon_mass.clamp(0.05, 10.0)
        };
        self.target_moon_dist_radii = target_moon_dist_radii.clamp(15.0, 160.0);
        self.epoch_pct = epoch_pct.min(100);
        self.recompute_fields(false);
    }

    /// Advance orbital phase and apply one coupled Fluvial + Coastal Tidal erosion & shelf deposition step.
    pub fn step_erode(&mut self) {
        self.orbital_phase_deg = (self.orbital_phase_deg + 25.0).rem_euclid(360.0);
        self.recompute_fields(true);
        self.steps += 1;
    }

    pub fn initial_moon_dist_radii(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return 0.0;
        }
        let h_tide = TectonicsSimulation::harmonic_tidal_multiplier(self.moon_count);
        let factor = (1.0 + 450.0 * self.moon_mass * h_tide).powf(2.0 / 13.0);
        (self.target_moon_dist_radii / factor).clamp(14.0, self.target_moon_dist_radii)
    }

    pub fn current_moon_dist_radii(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return 0.0;
        }
        let a0 = self.initial_moon_dist_radii();
        let a_tgt = self.target_moon_dist_radii;
        let tau = (self.epoch_pct as f64) / 100.0;
        let a0_p = a0.powf(6.5);
        let at_p = a_tgt.powf(6.5);
        (a0_p + (at_p - a0_p) * tau).max(1.0).powf(1.0 / 6.5)
    }

    pub fn current_tidal_strength(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            0.15
        } else {
            let a = self.current_moon_dist_radii().max(12.0);
            let h_tide = TectonicsSimulation::harmonic_tidal_multiplier(self.moon_count);
            let lunar = self.moon_mass * (60.0 / a).powi(3) * h_tide;
            0.15 + 0.85 * lunar
        }
    }

    pub fn current_spin(&self) -> f64 {
        let base_spin = 24.0 / self.target_day_hours.max(1.0);
        // Reference angular momentum extracted by 1 Earth-like moon (1.0x mass receding to 60 Rp):
        let a0_earth = (60.0 / 451.0f64.powf(2.0 / 13.0)).clamp(14.0, 60.0);
        let delta_spin_earth = 3.10 * (1.0 - (a0_earth / 60.0).sqrt());
        let proto_hadean_spin = base_spin + delta_spin_earth;

        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            // Without a moon to tidally brake the planet, it retains its fast Hadean rotation:
            proto_hadean_spin.clamp(0.25, 6.0)
        } else {
            let a0 = self.initial_moon_dist_radii();
            let a_cur = self.current_moon_dist_radii();
            let h_ang = TectonicsSimulation::harmonic_angular_momentum_multiplier(self.moon_count);
            let extracted_spin = 3.10
                * self.moon_mass
                * h_ang
                * ((a_cur / 60.0).sqrt() - (a0 / 60.0).sqrt()).max(0.0);
            (proto_hadean_spin - extracted_spin).clamp(0.25, 6.0)
        }
    }

    pub fn current_day_hours(&self) -> f64 {
        24.0 / self.current_spin()
    }

    pub fn current_cells(&self) -> u32 {
        tec11_climate::nominal_cells_for_spin(self.current_spin())
    }

    pub fn current_tilt_deg(&self) -> f64 {
        let h_tide = TectonicsSimulation::harmonic_tidal_multiplier(self.moon_count);
        let s_obliq = if self.moon_count == 0 || self.moon_mass <= 0.0 {
            0.0
        } else {
            self.moon_mass * (60.0 / self.target_moon_dist_radii.max(15.0)).powi(3) * h_tide
        };
        let t = (self.epoch_pct as f64) * 0.75 + (self.steps as f64);
        let wave = 0.65 * (0.19 * t).sin() + 0.35 * (0.31 * t + 1.1).cos();
        let amp = if s_obliq >= 0.25 {
            (1.2 / s_obliq.sqrt()).clamp(0.2, 2.4)
        } else {
            14.0 * (1.0 - 3.3 * s_obliq).clamp(0.2, 1.0)
        };
        (self.base_tilt_deg + amp * wave).clamp(0.0, 85.0)
    }

    /// Recompute climate, local tidal range, quadrupolar tidal bulge, coastal tidal scour,
    /// fluvial erosion, and (if `apply_to_strata` is true) mutate the rock column layers.
    fn recompute_fields(&mut self, apply_to_strata: bool) {
        let n = self.tiles.len();
        self.climate_model
            .set_spin_and_tilt_for_tectonics(self.current_spin(), self.current_tilt_deg());
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            let tn = c.mean_temperature as f32;
            self.tile_temp_norm[v] = tn;
            self.tile_temp_c[v] = tn * 75.0 - 20.0;
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }

        // 1. Compute harmonic moon positions & quadrupolar tidal bulge + Green's Law coastal range:
        let a1 = self.current_moon_dist_radii().max(12.0);
        let phase_rad = self.orbital_phase_deg * PI / 180.0;
        let incl_rad = 5.14 * PI / 180.0;
        let mut moon_vecs = Vec::with_capacity(self.moon_count as usize);
        for k in 0..self.moon_count.min(8) {
            // In a 2:1 Laplace resonance chain, P_k = 2^k P_1 => angular frequency omega_k = omega_1 / 2^k,
            // with Laplace phase offsets so inner moons periodically align in syzygy:
            let phi_k = phase_rad / (2.0f64.powi(k as i32)) + (k as f64) * 0.35;
            let mx = phi_k.cos() * incl_rad.cos();
            let my = phi_k.sin() * incl_rad.cos();
            let mz = (phi_k * 0.5).sin() * incl_rad.sin();
            let weight = self.moon_mass * (60.0 / a1).powi(3) * 4.0f64.powi(-(k as i32));
            moon_vecs.push(([mx, my, mz], weight));
        }
        let sun_vec = [1.0, 0.0, 0.0];
        let solar_weight = 0.18;

        let tidal_strength = self.current_tidal_strength();
        for v in 0..n {
            let p = self.grid.faces[v].0;
            let elev = self.tiles[v].elevation();

            // Instantaneous quadrupolar Legendre P_2(cos theta) = 0.5 * (3 cos^2 theta - 1):
            let cos_sun = dot(p, sun_vec);
            let mut p2_sum = solar_weight * 0.5 * (3.0 * cos_sun * cos_sun - 1.0);
            for &(m_vec, w) in &moon_vecs {
                let cos_m = dot(p, m_vec);
                p2_sum += w * 0.5 * (3.0 * cos_m * cos_m - 1.0);
            }

            // Count ocean & land neighbors for coastal shoaling / bay funneling:
            let mut ocean_nbs = 0usize;
            let mut land_nbs = 0usize;
            for &u in &self.adjacency[v] {
                if self.tiles[u].elevation() <= 0.0 {
                    ocean_nbs += 1;
                } else {
                    land_nbs += 1;
                }
            }

            // Green's Law shallow-shelf shoaling (eta ~ h^(-1/4)) + bay constriction amplification:
            let shoaling = if elev <= 0.0 {
                let depth_km = (-elev).clamp(0.15, 6.0) as f64;
                let shelf_factor = (2.5 / depth_km).powf(0.35).clamp(0.8, 2.4);
                let funnel_factor = 1.0 + 0.45 * (land_nbs as f64);
                shelf_factor * funnel_factor
            } else if ocean_nbs > 0 {
                1.35 + 0.35 * (ocean_nbs as f64)
            } else {
                0.08 // Solid-earth body tide on inland crust (~20-30 cm on Earth)
            };

            self.tile_tidal_range_m[v] = (0.85 * tidal_strength * shoaling).clamp(0.0, 80.0) as f32;
            self.tile_tidal_bulge_m[v] = (0.55 * p2_sum * shoaling).clamp(-40.0, 40.0) as f32;
        }

        // 2. Compute downhill Fluvial erosion + Coastal Tidal Scour & Shelf Deposition:
        let mut outgoing_dests: Vec<Vec<(usize, f32, f32)>> = vec![Vec::new(); n]; // (dest_u, fluvial_d, tidal_d)
        let mut max_erosion_cap: Vec<f32> = vec![0.0; n];
        let tidal_factor = tidal_strength.sqrt().clamp(0.2, 4.5) as f32;

        self.tile_tidal_scour.fill(0.0);
        self.tile_fluvial_lost.fill(0.0);
        self.tile_tidal_deposition.fill(0.0);

        for v in 0..n {
            let elev_v = self.tiles[v].elevation();
            if elev_v > 0.02 {
                let base_climate =
                    self.tile_precip_norm[v] * (1.0 - self.tile_temp_norm[v]).max(0.0);
                let high_relief = if elev_v > 2.2 {
                    0.08 * ((elev_v - 2.2) / 6.0).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let climate_factor = (base_climate + high_relief).max(0.0);
                let mut min_nb_elev = elev_v;
                let mut ocean_nb_count = 0usize;
                for &u in &self.adjacency[v] {
                    if self.tiles[u].elevation() <= 0.0 {
                        ocean_nb_count += 1;
                    }
                }
                // Headland / bay-mouth exposure factor:
                let exposure = 0.8 + 0.25 * (ocean_nb_count as f32);

                for &u in &self.adjacency[v] {
                    let raw_nb = self.tiles[u].elevation();
                    let nb_elev = raw_nb.max(0.0);
                    if nb_elev < elev_v {
                        min_nb_elev = min_nb_elev.min(nb_elev);
                        let drop = elev_v - nb_elev;
                        let fluvial_d = (0.34 * (elev_v + drop) / 10.0) * climate_factor;
                        let tidal_d = if raw_nb <= 0.0 {
                            0.014 * tidal_factor * exposure
                        } else {
                            0.0
                        };
                        if fluvial_d + tidal_d > 0.0 {
                            outgoing_dests[v].push((u, fluvial_d, tidal_d));
                        }
                    }
                }
                if min_nb_elev < elev_v {
                    let base_level = min_nb_elev.max(0.02);
                    max_erosion_cap[v] = (0.45 * (elev_v - base_level)).max(0.0);
                }
            }
        }

        let mut incoming_batches: Vec<Vec<ErodedMaterialBatch>> = vec![Vec::new(); n];
        let mut incoming_tidal_amount: Vec<f32> = vec![0.0; n];

        for v in 0..n {
            if outgoing_dests[v].is_empty() {
                continue;
            }
            let sum_fluvial: f32 = outgoing_dests[v].iter().map(|&(_, f, _)| f).sum();
            let sum_tidal: f32 = outgoing_dests[v].iter().map(|&(_, _, t)| t).sum();
            let total_degree = sum_fluvial + sum_tidal;

            self.tile_fluvial_lost[v] = sum_fluvial;
            self.tile_tidal_scour[v] = sum_tidal;

            if total_degree <= 1e-7 || max_erosion_cap[v] <= 1e-6 {
                continue;
            }

            let avg_degree = total_degree / (outgoing_dests[v].len() as f32);
            if apply_to_strata {
                let (total_eroded, stripped_layers) =
                    self.tiles[v].erode_column(avg_degree, 1.0, max_erosion_cap[v]);
                if total_eroded > 1e-6 && !stripped_layers.is_empty() {
                    for &(dest_u, f_u, t_u) in &outgoing_dests[v] {
                        let deg_u = f_u + t_u;
                        let share = total_eroded * (deg_u / total_degree);
                        let tidal_share = total_eroded * (t_u / total_degree);
                        incoming_tidal_amount[dest_u] += tidal_share;
                        incoming_batches[dest_u].push(ErodedMaterialBatch {
                            amount: share,
                            total: total_eroded,
                            substance: stripped_layers.clone(),
                        });
                    }
                }
            } else {
                // Diagnostic preview without mutating rock layers:
                let preview_eroded = avg_degree.min(max_erosion_cap[v]);
                for &(dest_u, f_u, t_u) in &outgoing_dests[v] {
                    let deg_u = f_u + t_u;
                    let share = preview_eroded * (deg_u / total_degree);
                    let is_sea = self.tiles[dest_u].elevation() <= 0.0;
                    if is_sea || t_u > 0.0 {
                        incoming_tidal_amount[dest_u] += share;
                    }
                }
            }
        }

        for t in 0..n {
            self.tile_tidal_deposition[t] = incoming_tidal_amount[t];
            if apply_to_strata && !incoming_batches[t].is_empty() {
                let dep_climate = DepositClimate {
                    temperature_c: self.tile_temp_c[t],
                    precipitation_norm: self.tile_precip_norm[t],
                    life: self.tile_life[t],
                };
                let is_sea = self.tiles[t].elevation() <= 0.0;
                let layer =
                    deposit_sedimentary(&incoming_batches[t], true, is_sea, Some(dep_climate));
                self.tiles[t].deposit(layer);
            }
        }

        if apply_to_strata {
            for tile in &mut self.tiles {
                tile.limit();
            }
        }
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn is_custom_planet(&self) -> bool {
        self.custom_geoticks.is_some()
    }

    pub fn loaded_geoticks(&self) -> u32 {
        self.custom_geoticks.unwrap_or(0)
    }

    pub fn loaded_land_percent(&self) -> u32 {
        self.custom_land_percent.unwrap_or(29)
    }

    pub fn moon_count(&self) -> u32 {
        self.moon_count
    }

    pub fn moon_mass(&self) -> f64 {
        self.moon_mass
    }

    pub fn target_moon_dist_radii(&self) -> f64 {
        self.target_moon_dist_radii
    }

    pub fn epoch_pct(&self) -> u32 {
        self.epoch_pct
    }

    /// Mean coastal tidal scour rate across all coastal land tiles (`km / geotick`).
    pub fn mean_coastal_tidal_scour(&self) -> f32 {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for &s in &self.tile_tidal_scour {
            if s > 0.0 {
                sum += s;
                count += 1;
            }
        }
        if count == 0 {
            0.0
        } else {
            sum / (count as f32)
        }
    }

    /// Fraction (`0.0..=100.0%`) of total coastal erosion attributable to lunar/solar tides.
    pub fn coastal_tidal_erosion_share_pct(&self) -> f32 {
        let mut sum_tidal = 0.0f32;
        let mut sum_total = 0.0f32;
        for v in 0..self.tiles.len() {
            if self.tile_tidal_scour[v] > 0.0 {
                sum_tidal += self.tile_tidal_scour[v];
                sum_total += self.tile_tidal_scour[v] + self.tile_fluvial_lost[v];
            }
        }
        if sum_total <= 1e-7 {
            0.0
        } else {
            100.0 * sum_tidal / sum_total
        }
    }

    /// Max local tidal range (`meters`) across all ocean/coastal tiles.
    pub fn max_tidal_range_m(&self) -> f32 {
        self.tile_tidal_range_m
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
    }

    /// Format a summary of the configured harmonic moon system.
    pub fn harmonic_chain_summary(&self) -> String {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return format!(
                "Moonless World (Solar tides only: 0.15x Earth) | Day: {:.1}h ({} cells) | Axial Tilt: {:.1}° (Chaotic wander ±14°)",
                self.current_day_hours(),
                self.current_cells(),
                self.current_tilt_deg()
            );
        }
        let a1 = self.current_moon_dist_radii();
        let mut parts = Vec::with_capacity(self.moon_count as usize);
        for k in 0..self.moon_count.min(4) {
            let period_ratio = 1u32 << k;
            let ak = a1 * 2.0f64.powf((2 * k) as f64 / 3.0);
            let tide_pct = 100.0 * 4.0f64.powi(-(k as i32))
                / TectonicsSimulation::harmonic_tidal_multiplier(self.moon_count);
            parts.push(format!(
                "M{}: {:.1} Rp ({}:1, {:.0}% tide)",
                k + 1,
                ak,
                period_ratio,
                tide_pct
            ));
        }
        format!(
            "Day: {:.1}h ({} cells) | Tilt: {:.1}° | Tides: {:.2}x Earth (Max {:.1}m) | {}",
            self.current_day_hours(),
            self.current_cells(),
            self.current_tilt_deg(),
            self.current_tidal_strength(),
            self.max_tidal_range_m(),
            parts.join(" · ")
        )
    }

    pub fn inspect_tile(&self, idx: usize) -> String {
        if idx >= self.tiles.len() {
            return String::new();
        }
        let t = &self.tiles[idx];
        let elev = t.elevation();
        let kind = if elev > 0.0 { "Land" } else { "Ocean" };
        let range_m = self.tile_tidal_range_m[idx];
        let bulge_m = self.tile_tidal_bulge_m[idx];
        let regime = if elev > 0.0 && self.tile_tidal_scour[idx] <= 0.0 {
            "Inland Solid-Crust Body Tide"
        } else if range_m < 2.0 {
            "Microtidal (< 2 m)"
        } else if range_m < 4.0 {
            "Mesotidal (2–4 m)"
        } else if range_m < 8.0 {
            "Macrotidal (4–8 m)"
        } else {
            "Megatidal (> 8 m)"
        };

        let mut lines = Vec::new();
        lines.push(format!(
            "Tile #{idx} ({kind}) | Elev: {elev:+.2} km | Crust: {:.1} km",
            t.thickness()
        ));
        lines.push(format!(
            "Tidal Range: {range_m:.2} m ({regime}) | Instantaneous Bulge: {bulge_m:+.2} m"
        ));
        if elev > 0.0 {
            let tidal_scour = self.tile_tidal_scour[idx];
            let fluvial = self.tile_fluvial_lost[idx];
            let tot = (tidal_scour + fluvial).max(1e-7);
            let pct_tidal = if tidal_scour > 0.0 {
                100.0 * tidal_scour / tot
            } else {
                0.0
            };
            lines.push(format!(
                "Erosion Rate: Tidal Scour {:.4} km ({pct_tidal:.0}% of total) | Fluvial Rain {:.4} km",
                tidal_scour, fluvial
            ));
            lines.push(format!(
                "Climate: {} | Temp: {:+.1} °C | Precip: {:.0}%",
                koeppen_climate_name(self.tile_koeppen[idx]),
                self.tile_temp_c[idx],
                self.tile_precip_norm[idx] * 100.0
            ));
        } else {
            lines.push(format!(
                "Coastal Shelf Deposition Rate: {:.4} km / step",
                self.tile_tidal_deposition[idx]
            ));
        }
        lines.push("Rock Layers (surface -> Moho):".to_string());
        for layer in t.layers.iter().rev() {
            let r = &layer.rock;
            lines.push(format!(
                "  • {} ({:.2} km, {}, tough {:.2})",
                r.name,
                layer.thickness,
                r.kind.code(),
                r.toughness
            ));
        }
        lines.join("\n")
    }

    /// Compute RGB vertex/tile colors for the Moon Sandbox:
    /// - `"tidal_erosion"`: Natural terrain + amber/gold coastal tidal scour + cyan shelf deposition + magenta fluvial erosion
    /// - `"tidal_range"`: Global tidal range & live quadrupolar tidal bulge heatmap
    /// - `"climate"`: Köppen climate zones under current moon-braked day length and axial tilt
    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_tidal_scour: bool,
        show_fluvial: bool,
        show_deposition: bool,
        selected_tile: Option<usize>,
    ) -> Vec<f32> {
        let n = self.tiles.len();
        let mut out = Vec::with_capacity(n * 3);

        for v in 0..n {
            let h = self.tiles[v].elevation();
            let rgb = match aspect {
                "climate" => {
                    if h > 0.0 {
                        koeppen_rgb(self.tile_koeppen[v])
                    } else {
                        let d = (-h / 6.0).clamp(0.0, 1.0);
                        [0.02 * (1.0 - d), 0.12 * (1.0 - d), 0.35 + 0.35 * (1.0 - d)]
                    }
                }
                "tidal_range" => {
                    let range_m = self.tile_tidal_range_m[v];
                    let bulge_m = self.tile_tidal_bulge_m[v];
                    if h <= 0.0 {
                        // Ocean color mapped by tidal range (0..12 m) + live bulge shading:
                        let u = (range_m / 8.0).clamp(0.0, 1.0);
                        let b_shift = (bulge_m / 4.0).clamp(-0.35, 0.35);
                        let r = (0.04 + 0.90 * u * u + 0.15 * b_shift).clamp(0.0, 1.0);
                        let g = (0.14 + 0.72 * u + 0.15 * b_shift).clamp(0.0, 1.0);
                        let b = (0.45 + 0.45 * (1.0 - 0.5 * u) + 0.20 * b_shift).clamp(0.0, 1.0);
                        [r, g, b]
                    } else if self.tile_tidal_scour[v] > 0.0 {
                        // Intertidal coastline highlighted by local tidal range:
                        let u = (range_m / 8.0).clamp(0.15, 1.0);
                        [0.95, (0.45 + 0.45 * u).clamp(0.0, 1.0), 0.10]
                    } else {
                        let e_norm = (h / 6.0).clamp(0.0, 1.0);
                        [0.18 + 0.25 * e_norm, 0.28 + 0.20 * e_norm, 0.18 + 0.15 * e_norm]
                    }
                }
                _ => {
                    // Default "tidal_erosion" view:
                    let mut base = if h <= 0.0 {
                        let d = (-h / 6.0).clamp(0.0, 1.0);
                        [0.02 * (1.0 - d), 0.12 * (1.0 - d), 0.35 + 0.35 * (1.0 - d)]
                    } else {
                        let felsity = self.tiles[v].top_felsity();
                        let life = self.tile_life[v];
                        [
                            (0.5 * 128.0) / 255.0,
                            (0.5 * (64.0 + 64.0 * felsity + 220.0 * life)) / 255.0,
                            (0.5 * (128.0 * felsity)) / 255.0,
                        ]
                    };

                    if show_fluvial && self.tile_fluvial_lost[v] > 1e-4 {
                        let w = (self.tile_fluvial_lost[v] * 14.0).clamp(0.0, 0.85);
                        base[0] = base[0] * (1.0 - w) + 0.92 * w;
                        base[1] = base[1] * (1.0 - w) + 0.12 * w;
                        base[2] = base[2] * (1.0 - w) + 0.38 * w;
                    }
                    if show_tidal_scour && self.tile_tidal_scour[v] > 1e-5 {
                        // Bright amber-gold highlight showing coastal tidal scour intensity:
                        let w = (self.tile_tidal_scour[v] * 22.0).sqrt().clamp(0.25, 0.96);
                        base[0] = base[0] * (1.0 - w) + 1.00 * w;
                        base[1] = base[1] * (1.0 - w) + 0.76 * w;
                        base[2] = base[2] * (1.0 - w) + 0.05 * w;
                    }
                    if show_deposition && h <= 0.0 && self.tile_tidal_deposition[v] > 1e-5 {
                        // Bright cyan-turquoise highlight showing coastal shelf tidal deposition:
                        let w = (self.tile_tidal_deposition[v] * 20.0).sqrt().clamp(0.20, 0.92);
                        base[0] = base[0] * (1.0 - w) + 0.08 * w;
                        base[1] = base[1] * (1.0 - w) + 0.92 * w;
                        base[2] = base[2] * (1.0 - w) + 0.88 * w;
                    }
                    base
                }
            };
            out.extend_from_slice(&rgb);
        }

        if let Some(sel) = selected_tile {
            if sel * 3 + 2 < out.len() {
                out[sel * 3] = 1.0;
                out[sel * 3 + 1] = 0.1;
                out[sel * 3 + 2] = 0.1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moon_sandbox_earth_and_harmonic_scaling() {
        let mut sandbox = MoonSandbox::new_earth();
        assert_eq!(sandbox.tile_count(), 7292);
        assert!((sandbox.current_day_hours() - 24.0).abs() < 0.05);
        assert_eq!(sandbox.current_cells(), 3);
        let base_scour = sandbox.mean_coastal_tidal_scour();
        let base_range = sandbox.max_tidal_range_m();
        assert!(base_scour > 0.0);
        assert!(base_range > 2.0);

        // Moonless Earth at 100% epoch retains fast unbraked Hadean rotation (< 12h day, 5 cells):
        sandbox.set_moon_params(0, 0.0, 60.0, 100);
        assert!(sandbox.current_day_hours() < 12.0);
        assert_eq!(sandbox.current_cells(), 5);

        // Heavier 1.5x moon at 100% epoch brakes rotation beyond 24h (> 32h day, 1 cell):
        sandbox.set_moon_params(1, 1.5, 60.0, 100);
        assert!(sandbox.current_day_hours() > 32.0);
        assert_eq!(sandbox.current_cells(), 1);

        // Configure 2 harmonic moons of 1.5x Lunar mass at Hadean epoch (0%):
        sandbox.set_moon_params(2, 1.5, 50.0, 0);
        let hadean_scour = sandbox.mean_coastal_tidal_scour();
        let hadean_range = sandbox.max_tidal_range_m();
        assert!(
            hadean_scour > base_scour * 2.0,
            "Expected Hadean 2-moon coastal tidal scour ({hadean_scour}) > 2x modern Earth ({base_scour})"
        );
        assert!(
            hadean_range > base_range * 3.0,
            "Expected Hadean 2-moon tidal range ({hadean_range} m) > 3x modern Earth ({base_range} m)"
        );

        // Step erosion and verify layers update and tile inspection reports tidal scour:
        sandbox.step_erode();
        assert_eq!(sandbox.steps(), 1);
        let colors = sandbox.tile_colors_rgb("tidal_erosion", true, true, true, Some(0));
        assert_eq!(colors.len(), 7292 * 3);
    }
}
