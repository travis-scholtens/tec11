use std::f64::consts::PI;

use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{dot, Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use crate::prehistory::PrehistorySimulation;
use crate::rock::{
    deposit_sedimentary, DepositClimate, ErodedMaterialBatch, Rock, RockKind, RockLayer,
};
use crate::tectonics::TectonicsSimulation;
use crate::tile::TileGeology;
use crate::world::{koeppen_climate_name, koeppen_rgb};

/// Standalone Meteor Strike, Crater Excavation & Impact Winter Sandbox (`MeteorScreen`).
/// Loads any `.tec11` planet file (or canned Earth `ETOPO2`) and visualizes:
/// - Holsapple & Schmidt Pi-group crater scaling (`D_crater` from impactor diameter, velocity, and density),
/// - Multi-ring crater excavation, shock metamorphism (`hornfels`), and impact-melt sheets (`basalt`),
/// - Raised impact-breccia (`conglomerate`) ejecta rims and flooded circular bays / crater basins,
/// - Airblast, thermal radiation & tsunami shock zones,
/// - Global silicate dust / sulfate aerosol impact winter (`Delta T` cooling) and biosphere survival vs. extinction.
pub struct MeteorSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    radius_km: f64,
    base_tiles: Vec<TileGeology>,
    tiles: Vec<TileGeology>,
    climate_model: ClimateModel,
    impactor_diameter_km: f64,
    impactor_velocity_km_s: f64,
    impactor_density_kg_m3: f64,
    impact_count: u32,
    recovery_steps: u32,
    current_cooling_c: f32,
    last_impact_tile: Option<usize>,
    last_crater_diameter_km: f32,
    last_energy_mt: f64,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
    tile_koeppen: Vec<u8>,
    tile_base_life: Vec<f32>,
    tile_life: Vec<f32>,
    tile_temp_norm: Vec<f32>,
    tile_temp_c: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    tile_impact_scar: Vec<f32>,
    tile_shock_heat: Vec<f32>,
    tile_crater_depth_km: Vec<f32>,
    tile_ejecta_thickness_km: Vec<f32>,
    tile_blast_intensity: Vec<f32>,
    rng: Pcg64Mcg,
}

impl MeteorSandbox {
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
                            quartzity: None,
                            sulfidity: None,
                            salinity: None,
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
            radius_km: 6371.0,
            base_tiles: tiles.clone(),
            tiles,
            climate_model,
            impactor_diameter_km: 10.0,
            impactor_velocity_km_s: 20.0,
            impactor_density_kg_m3: 3000.0,
            impact_count: 0,
            recovery_steps: 0,
            current_cooling_c: 0.0,
            last_impact_tile: None,
            last_crater_diameter_km: 0.0,
            last_energy_mt: 0.0,
            custom_geoticks: None,
            custom_land_percent: None,
            tile_koeppen: vec![0; n],
            tile_base_life: vec![0.0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            tile_impact_scar: vec![0.0; n],
            tile_shock_heat: vec![0.0; n],
            tile_crater_depth_km: vec![0.0; n],
            tile_ejecta_thickness_km: vec![0.0; n],
            tile_blast_intensity: vec![0.0; n],
            rng: Pcg64Mcg::from_seed_u64(0x5EED_CAFE_1234_5678),
        };
        sim.recompute_climate_and_biosphere();
        sim
    }

    pub fn load_earth(&mut self) {
        let d = self.impactor_diameter_km;
        let v = self.impactor_velocity_km_s;
        let rho = self.impactor_density_kg_m3;
        *self = Self::new_earth();
        self.impactor_diameter_km = d;
        self.impactor_velocity_km_s = v;
        self.impactor_density_kg_m3 = rho;
    }

    /// Load any `.tec11` snapshot (`Tectonics`, `Prehistory`, or `History`) into the Meteor Sandbox.
    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let radius_km = if snap.radius_km > 0.0 {
            snap.radius_km as f64
        } else {
            6371.0
        };
        let (grid, tiles, geoticks, land_pct, spin, tilt_deg) =
            if let Some(tec_sim) = TectonicsSimulation::from_snapshot(&snap, 42) {
                let sp = if snap.spin > 0.0 {
                    (snap.spin as f64).clamp(0.25, 6.0)
                } else {
                    1.0
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
                    sp,
                    tilt,
                )
            } else if let Some(pre_sim) = PrehistorySimulation::from_snapshot(&snap, 42) {
                (
                    pre_sim.grid().clone(),
                    pre_sim.tiles().to_vec(),
                    pre_sim.tectonics_ticks(),
                    pre_sim.land_percent(),
                    1.0,
                    23.0,
                )
            } else {
                return false;
            };

        let n = grid.faces.len();
        self.adjacency = grid.adjacency();
        self.grid = grid.clone();
        self.radius_km = radius_km;
        self.base_tiles = tiles.clone();
        self.tiles = tiles;
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        self.climate_model = ClimateModel::new_for_tectonics(grid, cells, spin, tilt_deg);
        self.impact_count = 0;
        self.recovery_steps = 0;
        self.current_cooling_c = 0.0;
        self.last_impact_tile = None;
        self.last_crater_diameter_km = 0.0;
        self.last_energy_mt = 0.0;
        self.custom_geoticks = Some(geoticks);
        self.custom_land_percent = Some(land_pct);
        self.tile_koeppen = vec![0; n];
        self.tile_base_life = vec![0.0; n];
        self.tile_life = vec![0.0; n];
        self.tile_temp_norm = vec![0.5; n];
        self.tile_temp_c = vec![15.0; n];
        self.tile_precip_norm = vec![0.5; n];
        self.tile_impact_scar = vec![0.0; n];
        self.tile_shock_heat = vec![0.0; n];
        self.tile_crater_depth_km = vec![0.0; n];
        self.tile_ejecta_thickness_km = vec![0.0; n];
        self.tile_blast_intensity = vec![0.0; n];

        // Populate pre-existing impact scars from loaded rock strata:
        for v in 0..n {
            let sig = self.tiles[v].impact_signature();
            if sig > 0.0 {
                self.tile_impact_scar[v] = sig;
            }
        }
        self.recompute_climate_and_biosphere();
        true
    }

    /// Reset planet to its pre-impact state.
    pub fn reset_planet(&mut self) {
        self.tiles = self.base_tiles.clone();
        self.impact_count = 0;
        self.recovery_steps = 0;
        self.current_cooling_c = 0.0;
        self.last_impact_tile = None;
        self.last_crater_diameter_km = 0.0;
        self.last_energy_mt = 0.0;
        self.tile_impact_scar.fill(0.0);
        self.tile_shock_heat.fill(0.0);
        self.tile_crater_depth_km.fill(0.0);
        self.tile_ejecta_thickness_km.fill(0.0);
        self.tile_blast_intensity.fill(0.0);
        for v in 0..self.tiles.len() {
            let sig = self.tiles[v].impact_signature();
            if sig > 0.0 {
                self.tile_impact_scar[v] = sig;
            }
        }
        self.recompute_climate_and_biosphere();
    }

    pub fn set_impactor_params(
        &mut self,
        diameter_km: f64,
        velocity_km_s: f64,
        density_kg_m3: f64,
    ) {
        self.impactor_diameter_km = diameter_km.clamp(0.5, 120.0);
        self.impactor_velocity_km_s = velocity_km_s.clamp(11.0, 75.0);
        self.impactor_density_kg_m3 = density_kg_m3.clamp(500.0, 9000.0);
    }

    /// Impactor kinetic energy in Megatons TNT (`1 MT = 4.184e15 J`).
    pub fn configured_kinetic_energy_mt(&self) -> f64 {
        let r_m = 500.0 * self.impactor_diameter_km;
        let vol_m3 = (4.0 / 3.0) * PI * r_m * r_m * r_m;
        let mass_kg = self.impactor_density_kg_m3 * vol_m3;
        let v_m_s = 1000.0 * self.impactor_velocity_km_s;
        let ke_joules = 0.5 * mass_kg * v_m_s * v_m_s;
        ke_joules / 4.184e15
    }

    /// Expected final complex crater diameter (`km`) from Holsapple & Schmidt Pi-group scaling:
    /// `D_crater ~ 30.5 * (rho_i / rho_target)^(1/3) * D_i^0.78 * (v_i / 20)^0.44`.
    /// For a 10 km stony asteroid (3000 kg/m^3) at 20 km/s, yields ~187 km (Chicxulub scale).
    pub fn configured_crater_diameter_km(&self) -> f64 {
        let rho_ratio = (self.impactor_density_kg_m3 / 2850.0).powf(1.0 / 3.0);
        let d_term = self.impactor_diameter_km.powf(0.78);
        let v_term = (self.impactor_velocity_km_s / 20.0).powf(0.44);
        (30.5 * rho_ratio * d_term * v_term).clamp(8.0, 1600.0)
    }

    /// Expected global surface temperature drop (`°C`) from sub-micron silicate dust
    /// and stratospheric sulfate aerosols injected by the configured impactor.
    pub fn configured_cooling_drop_c(&self) -> f32 {
        let mt = self.configured_kinetic_energy_mt();
        // Reference: Chicxulub (~7.5e7 MT) produces ~11.5 C global cooling:
        let ratio = (mt / 7.5e7).max(0.0);
        (11.5 * ratio.powf(0.36)).clamp(0.2, 28.0) as f32
    }

    /// Strike `target_tile` (or a random tile if `None`) with the currently configured impactor.
    /// Returns the struck tile index.
    pub fn strike_tile(&mut self, target_tile: Option<usize>) -> usize {
        let n = self.tiles.len();
        if n == 0 {
            return 0;
        }
        let center = match target_tile {
            Some(idx) if idx < n => idx,
            _ => (self.rng.next_u64() as usize) % n,
        };

        let d_crater_km = self.configured_crater_diameter_km();
        let r_crater_km = 0.5 * d_crater_km;
        let ke_mt = self.configured_kinetic_energy_mt();
        let cooling_pulse = self.configured_cooling_drop_c();

        self.impact_count = self.impact_count.saturating_add(1);
        self.last_impact_tile = Some(center);
        self.last_crater_diameter_km = d_crater_km as f32;
        self.last_energy_mt = ke_mt;
        self.current_cooling_c = (self.current_cooling_c + cooling_pulse).clamp(0.0, 30.0);

        let p_center = self.grid.faces[center].0;
        let excavate_max_km = (0.42 * (d_crater_km / 180.0)).clamp(0.15, 1.80) as f32;
        let rim_max_km = (0.20 * (d_crater_km / 180.0)).clamp(0.06, 0.55) as f32;

        // Check if `center` is coastal (adjacent to ocean) so coastal strikes can breach a circular bay:
        let is_coastal = self.adjacency[center]
            .iter()
            .any(|&nb| self.tiles[nb].elevation() <= 0.0);

        let ring1 = self.adjacency[center].clone();

        for u in 0..n {
            let p_u = self.grid.faces[u].0;
            let cos_ang = dot(p_center, p_u).clamp(-1.0, 1.0);
            let dist_km = cos_ang.acos() * self.radius_km;
            let is_center = u == center;
            let is_ring1 = ring1.contains(&u);

            if is_center || dist_km <= r_crater_km {
                // Inside transient/final crater basin:
                let profile = if r_crater_km > 1.0 {
                    (1.0 - (dist_km / r_crater_km).powi(2)).clamp(0.35, 1.0) as f32
                } else {
                    1.0
                };
                let exc = excavate_max_km * profile;
                let pre_elev = self.tiles[u].elevation();

                if pre_elev > 0.0 {
                    let cap = (self.tiles[u].thickness() - 1.2).max(0.0).min(exc);
                    if cap > 0.03 {
                        let _ = self.tiles[u].erode_column(cap, 1.0, cap);
                    }
                    // Shock-metamorphosed hornfels crater floor:
                    self.tiles[u].deposit(RockLayer {
                        rock: Rock {
                            kind: RockKind::Metamorphic,
                            name: "hornfels",
                            felsity: 0.55,
                            toughness: 0.88,
                            clasticity: None,
                            calcity: None,
                            bogginess: None,
                            quartzity: None,
                            sulfidity: if d_crater_km >= 180.0 {
                                Some(((d_crater_km - 150.0) / 300.0).clamp(0.18, 0.45) as f32)
                            } else {
                                None
                            },
                            salinity: None,
                        },
                        thickness: 0.20,
                    });
                    if d_crater_km >= 220.0 {
                        // Impact-melt basalt sheet in large complex basins:
                        let mut melt_sheet = Rock::extrusive(0.48);
                        melt_sheet.sulfidity = Some(0.38);
                        self.tiles[u].deposit(RockLayer {
                            rock: melt_sheet,
                            thickness: 0.24,
                        });
                    }
                    // If coastal and large enough (or low elevation), breach into a circular bay / crater lake:
                    let new_elev = if (is_coastal || pre_elev < 0.45) && d_crater_km >= 140.0 {
                        (pre_elev - 0.85 * exc).clamp(-0.65, 0.05)
                    } else {
                        (pre_elev - 0.65 * exc).clamp(0.06, pre_elev.max(0.06))
                    };
                    self.tiles[u].bottom = new_elev - self.tiles[u].thickness();
                    self.tiles[u].mountainosity *= 0.30;
                    self.tiles[u].limit();
                } else {
                    // Submarine crater excavation & shocked basement:
                    let sea_elev = (pre_elev - 0.35 * exc).clamp(-6.8, -0.15);
                    self.tiles[u].deposit(RockLayer {
                        rock: Rock {
                            kind: RockKind::Metamorphic,
                            name: "hornfels",
                            felsity: 0.50,
                            toughness: 0.85,
                            clasticity: None,
                            calcity: None,
                            bogginess: None,
                            quartzity: None,
                            sulfidity: Some(0.15),
                            salinity: None,
                        },
                        thickness: 0.16,
                    });
                    self.tiles[u].bottom = sea_elev - self.tiles[u].thickness();
                    self.tiles[u].limit();
                }

                self.tile_crater_depth_km[u] += exc;
                self.tile_impact_scar[u] = 1.0;
                self.tile_shock_heat[u] = 1.0;
                self.tile_blast_intensity[u] = 1.0;
            } else if (is_ring1 && d_crater_km >= 80.0) || dist_km <= 2.2 * r_crater_km {
                // Continuous ejecta blanket & raised structural rim:
                let r_norm = (dist_km / r_crater_km.max(40.0)).clamp(1.0, 2.5) as f32;
                let falloff = (1.0 / (r_norm * r_norm)).clamp(0.20, 1.0);
                let rim_t = rim_max_km * falloff;
                let pre_elev = self.tiles[u].elevation();
                if pre_elev > -0.35 {
                    self.tiles[u].deposit(RockLayer {
                        rock: Rock {
                            kind: RockKind::Sedimentary,
                            name: "conglomerate",
                            felsity: 0.58,
                            toughness: 0.55,
                            clasticity: Some(0.96),
                            calcity: None,
                            bogginess: None,
                            quartzity: None,
                            sulfidity: None,
                            salinity: None,
                        },
                        thickness: rim_t,
                    });
                    self.tiles[u].isostasize(-0.35 * rim_t);
                    if self.tiles[u].elevation() > 5.8 {
                        self.tiles[u].bottom = 5.8 - self.tiles[u].thickness();
                    }
                    self.tiles[u].mountainosity = (self.tiles[u].mountainosity + 1.2 * falloff).min(20.0);
                    self.tiles[u].limit();
                }
                self.tile_ejecta_thickness_km[u] += rim_t;
                self.tile_impact_scar[u] = self.tile_impact_scar[u].max(0.55);
                self.tile_shock_heat[u] = self.tile_shock_heat[u].max(0.45 * falloff);
                self.tile_blast_intensity[u] = self.tile_blast_intensity[u].max(0.85 * falloff);
            } else if dist_km <= 5.5 * r_crater_km.max(80.0) {
                // Airblast, thermal pulse & distal ejecta fallout zone:
                let blast_r = 5.5 * r_crater_km.max(80.0);
                let w = (1.0 - dist_km / blast_r).clamp(0.0, 1.0) as f32;
                self.tile_blast_intensity[u] = self.tile_blast_intensity[u].max(0.65 * w);
                self.tile_impact_scar[u] = self.tile_impact_scar[u].max(0.28 * w);
            }
        }

        self.recompute_climate_and_biosphere();
        center
    }

    /// Simulate a Late Heavy Bombardment barrage of `count` random asteroid/basin impacts.
    pub fn strike_barrage(&mut self, count: u32) -> usize {
        let saved_d = self.impactor_diameter_km;
        let saved_v = self.impactor_velocity_km_s;
        let mut last_tile = 0;
        for _ in 0..count.clamp(1, 25) {
            let u = self.rng.next_f64();
            self.impactor_diameter_km = (6.0 + 38.0 * u * u).clamp(4.0, 60.0);
            self.impactor_velocity_km_s = 15.0 + 25.0 * self.rng.next_f64();
            last_tile = self.strike_tile(None);
        }
        self.impactor_diameter_km = saved_d;
        self.impactor_velocity_km_s = saved_v;
        last_tile
    }

    /// Advance 1 post-impact recovery & fluvial erosion step:
    /// - Stratospheric dust and sulfate aerosols settle (`current_cooling_c *= 0.55`),
    /// - Thermal shock glow cools (`tile_shock_heat *= 0.50`),
    /// - Downhill fluvial erosion weathers raised ejecta rims and deposits sediment into crater basins.
    pub fn step_recover_and_erode(&mut self) {
        self.recovery_steps = self.recovery_steps.saturating_add(1);
        self.current_cooling_c *= 0.55;
        if self.current_cooling_c < 0.15 {
            self.current_cooling_c = 0.0;
        }
        for h in &mut self.tile_shock_heat {
            *h *= 0.50;
            if *h < 0.03 {
                *h = 0.0;
            }
        }
        for b in &mut self.tile_blast_intensity {
            *b *= 0.65;
            if *b < 0.03 {
                *b = 0.0;
            }
        }

        let n = self.tiles.len();
        let mut outgoing_dests: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
        let mut max_erosion_cap: Vec<f32> = vec![0.0; n];

        for v in 0..n {
            let elev_v = self.tiles[v].elevation();
            if elev_v > 0.02 {
                let climate_factor =
                    (self.tile_precip_norm[v] * (1.0 - self.tile_temp_norm[v]).max(0.0)).max(0.05);
                let mut min_nb = elev_v;
                for &u in &self.adjacency[v] {
                    let nb_elev = self.tiles[u].elevation().max(0.0);
                    if nb_elev < elev_v {
                        min_nb = min_nb.min(nb_elev);
                        let drop = elev_v - nb_elev;
                        let d = (0.34 * (elev_v + drop) / 10.0) * climate_factor;
                        if d > 0.0 {
                            outgoing_dests[v].push((u, d));
                        }
                    }
                }
                if min_nb < elev_v {
                    max_erosion_cap[v] = (0.40 * (elev_v - min_nb.max(0.02))).max(0.0);
                }
            }
        }

        let mut incoming_batches: Vec<Vec<ErodedMaterialBatch>> = vec![Vec::new(); n];
        for v in 0..n {
            if outgoing_dests[v].is_empty() || max_erosion_cap[v] <= 1e-6 {
                continue;
            }
            let total_deg: f32 = outgoing_dests[v].iter().map(|&(_, d)| d).sum();
            if total_deg <= 1e-7 {
                continue;
            }
            let avg_deg = total_deg / (outgoing_dests[v].len() as f32);
            let (eroded, stripped) =
                self.tiles[v].erode_column(avg_deg, 1.0, max_erosion_cap[v]);
            if eroded > 1e-6 && !stripped.is_empty() {
                for &(dest_u, deg_u) in &outgoing_dests[v] {
                    let share = eroded * (deg_u / total_deg);
                    incoming_batches[dest_u].push(ErodedMaterialBatch {
                        amount: share,
                        total: eroded,
                        substance: stripped.clone(),
                    });
                }
            }
        }

        let has_active_biosphere = self.current_cooling_c < 8.0;
        for t in 0..n {
            if !incoming_batches[t].is_empty() {
                let dep_clim = DepositClimate {
                    temperature_c: self.tile_temp_c[t],
                    precipitation_norm: self.tile_precip_norm[t],
                    life: self.tile_life[t],
                };
                let is_sea = self.tiles[t].elevation() <= 0.0;
                let layer = deposit_sedimentary(
                    &incoming_batches[t],
                    has_active_biosphere,
                    is_sea,
                    Some(dep_clim),
                );
                self.tiles[t].deposit(layer);
                self.tiles[t].limit();
            }
        }

        self.recompute_climate_and_biosphere();
    }

    fn recompute_climate_and_biosphere(&mut self) {
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);

        let cooling_norm = self.current_cooling_c / 75.0;
        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            let base_tn = c.mean_temperature as f32;
            let base_temp_c = base_tn * 75.0 - 20.0;
            let cooled_tn = (base_tn - cooling_norm).clamp(0.0, 1.0);
            let temp_c = cooled_tn * 75.0 - 20.0;
            self.tile_temp_norm[v] = cooled_tn;
            self.tile_temp_c[v] = temp_c;
            self.tile_precip_norm[v] =
                ((c.mean_precipitation as f32) * (1.0 - 0.018 * self.current_cooling_c).clamp(0.25, 1.0)).clamp(0.0, 1.0);

            // Reclassify effective Köppen under impact winter cooling:
            let base_k = c.koeppen as u8;
            self.tile_koeppen[v] = if self.tiles[v].elevation() <= 0.0 {
                0
            } else if self.current_cooling_c > 0.5 && temp_c < -8.0 {
                12 // EF (Ice Cap / Impact Winter Freeze)
            } else if self.current_cooling_c > 0.5 && temp_c < 0.0 && base_temp_c >= 0.0 {
                13 // ET (Tundra)
            } else {
                base_k
            };

            let base_life = if c.life && self.tiles[v].elevation() > 0.0 {
                1.0
            } else {
                0.0
            };
            self.tile_base_life[v] = base_life;

            // Biosphere survival: wiped out in high-intensity blast zones or sub-freezing impact winter:
            let blast_survival = (1.0 - self.tile_blast_intensity[v] * 1.15).clamp(0.0, 1.0);
            let winter_survival = if self.current_cooling_c <= 0.05 {
                1.0
            } else if temp_c < -2.0 {
                0.0
            } else if self.current_cooling_c > 16.0 {
                0.10
            } else {
                (1.0 - self.current_cooling_c / 22.0).clamp(0.15, 1.0)
            };
            self.tile_life[v] = base_life * blast_survival * winter_survival;
        }
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn impact_count(&self) -> u32 {
        self.impact_count
    }

    pub fn recovery_steps(&self) -> u32 {
        self.recovery_steps
    }

    pub fn current_cooling_c(&self) -> f32 {
        self.current_cooling_c
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

    /// Percentage (`0.0..=100.0%`) of originally habitable biosphere tiles still supporting life.
    pub fn biosphere_survival_pct(&self) -> f32 {
        let mut base_sum = 0.0f32;
        let mut cur_sum = 0.0f32;
        for v in 0..self.tiles.len() {
            base_sum += self.tile_base_life[v];
            cur_sum += self.tile_life[v];
        }
        if base_sum <= 1e-5 {
            100.0
        } else {
            (100.0 * cur_sum / base_sum).clamp(0.0, 100.0)
        }
    }

    /// Format a summary of the configured impactor and cumulative planetary impact state.
    pub fn impact_summary(&self) -> String {
        let d_crater = self.configured_crater_diameter_km();
        let mt = self.configured_kinetic_energy_mt();
        let mt_str = if mt >= 1.0e6 {
            format!("{:.1}M MT", mt / 1.0e6)
        } else if mt >= 1.0e3 {
            format!("{:.1}K MT", mt / 1.0e3)
        } else {
            format!("{:.0} MT", mt)
        };
        let class_str = if d_crater < 40.0 {
            "Regional Bolide"
        } else if d_crater < 130.0 {
            "Regional Basin"
        } else if d_crater < 260.0 {
            "Chicxulub Extinction Class"
        } else if d_crater < 550.0 {
            "Vredefort / Sudbury Ring Basin"
        } else {
            "Hadean / LHB Mega-Basin"
        };
        format!(
            "Impactor: {:.0} km @ {:.0} km/s → Crater {:.0} km ({}, {}) | Cooling: -{:.1} °C | Biosphere: {:.0}%",
            self.impactor_diameter_km,
            self.impactor_velocity_km_s,
            d_crater,
            mt_str,
            class_str,
            self.current_cooling_c,
            self.biosphere_survival_pct()
        )
    }

    pub fn inspect_tile(&self, idx: usize) -> String {
        if idx >= self.tiles.len() {
            return String::new();
        }
        let t = &self.tiles[idx];
        let elev = t.elevation();
        let kind = if elev > 0.0 { "Land" } else { "Ocean / Flooded Crater" };
        let scar = self.tile_impact_scar[idx];
        let zone = if self.tile_crater_depth_km[idx] > 0.01 || scar >= 0.75 {
            format!(
                "Crater Basin Floor (excavated {:.2} km, shocked hornfels/melt)",
                self.tile_crater_depth_km[idx]
            )
        } else if self.tile_ejecta_thickness_km[idx] > 0.01 || scar >= 0.45 {
            format!(
                "Raised Ejecta Rim (+{:.2} km impact breccia conglomerate)",
                self.tile_ejecta_thickness_km[idx]
            )
        } else if self.tile_blast_intensity[idx] > 0.05 {
            format!(
                "Airblast & Thermal Fallout Zone (intensity {:.0}%)",
                self.tile_blast_intensity[idx] * 100.0
            )
        } else {
            "Unshocked Crust".to_string()
        };

        let mut lines = Vec::new();
        lines.push(format!(
            "Tile #{idx} ({kind}) | Elev: {elev:+.2} km | Crust: {:.1} km",
            t.thickness()
        ));
        lines.push(format!("Impact Zone: {zone}"));
        lines.push(format!(
            "Climate: {} | Temp: {:+.1} °C (Impact Winter {:+.1} °C) | Life: {:.0}%",
            koeppen_climate_name(self.tile_koeppen[idx]),
            self.tile_temp_c[idx],
            -self.current_cooling_c,
            self.tile_life[idx] * 100.0
        ));
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

    /// Compute RGB vertex/tile colors for the Meteor Sandbox:
    /// - `"crater_ejecta"`: Natural terrain + glowing orange/crimson crater floors + gold ejecta rims + violet blast wave
    /// - `"topography"`: Natural terrain & bathymetry showing flooded circular crater bays and inland basins
    /// - `"impact_winter"`: Biosphere survival (green refugia) vs. impact winter freeze (icy blue) & blast sterilization (ash/crimson)
    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_ejecta: bool,
        show_blast: bool,
        selected_tile: Option<usize>,
    ) -> Vec<f32> {
        let n = self.tiles.len();
        let mut out = Vec::with_capacity(n * 3);

        for v in 0..n {
            let h = self.tiles[v].elevation();
            let base = if h <= 0.0 {
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

            let rgb = match aspect {
                "topography" => base,
                "impact_winter" => {
                    if h <= 0.0 {
                        if self.tile_temp_c[v] < -2.0 {
                            // Sea ice expansion during impact winter:
                            [0.72, 0.85, 0.95]
                        } else {
                            base
                        }
                    } else if self.tile_blast_intensity[v] > 0.35 {
                        // Sterilized blast & thermal incineration zone:
                        let w = self.tile_blast_intensity[v].clamp(0.35, 1.0);
                        [0.35 + 0.55 * w, 0.18 * (1.0 - 0.5 * w), 0.16]
                    } else if self.tile_temp_c[v] < 0.0 {
                        // Impact winter sub-freezing zone:
                        [0.70, 0.82, 0.96]
                    } else {
                        koeppen_rgb(self.tile_koeppen[v])
                    }
                }
                _ => {
                    // Default "crater_ejecta" view:
                    let mut c = base;
                    let scar = self.tile_impact_scar[v];
                    let heat = self.tile_shock_heat[v];

                    if show_blast && self.tile_blast_intensity[v] > 0.05 && scar < 0.45 {
                        let w = (self.tile_blast_intensity[v] * 0.70).clamp(0.10, 0.65);
                        c[0] = c[0] * (1.0 - w) + 0.78 * w;
                        c[1] = c[1] * (1.0 - w) + 0.22 * w;
                        c[2] = c[2] * (1.0 - w) + 0.68 * w;
                    }
                    if show_ejecta && (self.tile_ejecta_thickness_km[v] > 0.01 || (scar >= 0.45 && scar < 0.75)) {
                        // Raised breccia conglomerate ejecta rim in bright amber-gold:
                        let w = (self.tile_ejecta_thickness_km[v] * 4.5 + 0.55).clamp(0.50, 0.95);
                        c[0] = c[0] * (1.0 - w) + 0.98 * w;
                        c[1] = c[1] * (1.0 - w) + 0.78 * w;
                        c[2] = c[2] * (1.0 - w) + 0.10 * w;
                    }
                    if self.tile_crater_depth_km[v] > 0.01 || scar >= 0.75 {
                        // Excavated crater basin floor: fiery orange-red when fresh, cooling to shocked crimson-purple:
                        let r_col = 0.72 + 0.28 * heat;
                        let g_col = 0.12 + 0.36 * heat;
                        let b_col = 0.55 * (1.0 - heat) + 0.06 * heat;
                        let w = 0.88;
                        c[0] = c[0] * (1.0 - w) + r_col * w;
                        c[1] = c[1] * (1.0 - w) + g_col * w;
                        c[2] = c[2] * (1.0 - w) + b_col * w;
                    }
                    c
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
    fn test_meteor_sandbox_scaling_strike_and_recovery() {
        let mut sandbox = MeteorSandbox::new_earth();
        assert_eq!(sandbox.tile_count(), 7292);
        assert_eq!(sandbox.impact_count(), 0);
        assert!((sandbox.biosphere_survival_pct() - 100.0).abs() < 1e-4);

        // 10 km stony asteroid at 20 km/s -> ~180-190 km Chicxulub-scale crater:
        sandbox.set_impactor_params(10.0, 20.0, 3000.0);
        let d_chicxulub = sandbox.configured_crater_diameter_km();
        assert!(
            (150.0..=220.0).contains(&d_chicxulub),
            "Expected Chicxulub-scale crater diameter in 150..220 km, got {d_chicxulub:.1} km"
        );

        // Strike a known land tile and verify crater floor hornfels + neighbor ejecta conglomerate + impact winter cooling:
        let land_idx = (0..sandbox.tile_count())
            .find(|&i| sandbox.tiles[i].elevation() > 0.8)
            .unwrap();
        sandbox.strike_tile(Some(land_idx));
        assert_eq!(sandbox.impact_count(), 1);
        assert!(sandbox.current_cooling_c() > 8.0);
        assert!(sandbox.biosphere_survival_pct() < 90.0);

        let report = sandbox.inspect_tile(land_idx);
        assert!(report.contains("Crater Basin Floor"));
        assert!(report.contains("hornfels"));

        // Step recovery and verify dust settles and biosphere rebounds:
        let cooled = sandbox.current_cooling_c();
        sandbox.step_recover_and_erode();
        assert_eq!(sandbox.recovery_steps(), 1);
        assert!(sandbox.current_cooling_c() < cooled);

        let colors = sandbox.tile_colors_rgb("crater_ejecta", true, true, Some(land_idx));
        assert_eq!(colors.len(), 7292 * 3);
    }
}
