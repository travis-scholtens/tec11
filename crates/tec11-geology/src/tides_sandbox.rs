use std::collections::BTreeSet;

use tec11_climate::EARTH_SIZE6_ELEVATIONS_M;
use tec11_math_grid::{FineParentage, FiniteVolumeSphereGeom, FullSphereFineGrid, Pcg64Mcg};
use tec11_proto::PlanetSnapshotExt;
use tec11_tides::LaplaceTidalSolver;

use crate::prehistory::PrehistorySimulation;
use crate::rivers::trace_coarse_rivers;
use crate::sky_vantage::{
    build_uniform_fine_tile_inspection_view, json_escape, TileInspectionView, TileLiveEnvironment,
    TileSkyMoon, UniformFineTileSkyParams,
};
use crate::sun_sky::{
    generate_default_constellations, generate_solar_system_with_rng, substitute_simulated_planet,
    AtmosphereType, Constellation, HomeStar, PlanetaryOptics, ReconstructedSkyEnvironment,
    SolarSystemPlanet,
};
use crate::terrain::{TERRAIN_MIN_RIVER_ELEV, TERRAIN_MIN_RIVER_PRECIP};
use crate::world::koeppen_climate_name;

/// Shared full-sphere `coarse + 2` planetary substrate used by both `TidesSandbox` and `WeatherSandbox`.
pub struct FinePlanetBase {
    pub prehistory: PrehistorySimulation,
    pub fine: FullSphereFineGrid,
    pub geom: FiniteVolumeSphereGeom,
    pub fine_is_land: Vec<bool>,
    pub fine_is_river: Vec<bool>,
    pub fine_primary_coarse: Vec<usize>,
    pub effective_coarse_koeppen: Vec<u8>,
    pub effective_coarse_life: Vec<f32>,
    pub river_routes: Vec<Vec<usize>>,
    pub home_star: HomeStar,
    pub atmosphere: AtmosphereType,
    pub optics: PlanetaryOptics,
    pub has_atmosphere: bool,
    pub has_life: bool,
    pub default_moon_count: u32,
    pub default_moon_mass_lunar: f32,
    pub default_moon_dist_radii: f32,
    pub solar_system: Vec<SolarSystemPlanet>,
    pub constellations: Vec<Constellation>,
}

impl FinePlanetBase {
    pub fn new_earth() -> Self {
        Self::from_prehistory(PrehistorySimulation::new_earth(42))
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Option<Self> {
        let snap = tec11_proto::PlanetSnapshot::decode_bytes(bytes).ok()?;
        let pre = PrehistorySimulation::from_snapshot(&snap, 42)?;
        let sky = ReconstructedSkyEnvironment::from_snapshot(
            &snap,
            pre.radius_km() as f32,
            pre.land_percent(),
        );
        let mut base = Self::from_prehistory(pre);
        base.home_star = sky.home_star;
        base.atmosphere = sky.atmosphere;
        base.optics = sky.optics;
        base.has_atmosphere = sky.has_atmosphere;
        base.has_life = sky.has_life;
        base.default_moon_count = sky.moon_count;
        base.default_moon_mass_lunar = sky.moon_mass_lunar;
        base.default_moon_dist_radii = sky.current_moon_dist_radii;
        base.solar_system = sky.solar_system;
        base.constellations = sky.constellations;
        Some(base)
    }

    pub fn from_prehistory(prehistory: PrehistorySimulation) -> Self {
        let coarse_grid = prehistory.grid();
        let coarse_adj = prehistory.adjacency();
        let tiles = prehistory.tiles();

        // On canned size-6 Earth, use exact physical meter elevations/depths (`EARTH_SIZE6_ELEVATIONS_M * 0.001 km`)
        // so abyssal basins, continental shelves, and low-lying deltas/coasts (`+1..+50 m`) have true physical meters.
        let physical_coarse_elev_km: Vec<f32> =
            if !prehistory.is_custom_planet() && coarse_grid.size == 6 && tiles.len() == EARTH_SIZE6_ELEVATIONS_M.len() {
                EARTH_SIZE6_ELEVATIONS_M
                    .iter()
                    .map(|&m| (m as f32) * 0.001)
                    .collect()
            } else {
                tiles.iter().map(|t| t.elevation()).collect()
            };
        let coarse_mounts: Vec<f32> = tiles.iter().map(|t| t.mountainosity).collect();
        let coarse_thicks: Option<Vec<f32>> = prehistory
            .is_custom_planet()
            .then(|| tiles.iter().map(|t| t.thickness()).collect());

        let fine = FullSphereFineGrid::from_coarse_with_crust(
            coarse_grid,
            &physical_coarse_elev_km,
            &coarse_mounts,
            coarse_thicks.as_deref(),
        );

        // Route coarse rivers onto the fine grid using `PrehistorySimulation`'s climate elevations.
        let climate_elevs = prehistory.tile_elevations();
        let koeppen_ids = prehistory.tile_koeppen_ids();
        let glacier_cover = prehistory.climate_model().tile_glacier_cover();
        let precip_norm = prehistory.tile_precip_norm();
        let coarse_rivers = trace_coarse_rivers(
            &climate_elevs,
            precip_norm,
            glacier_cover,
            coarse_adj,
            TERRAIN_MIN_RIVER_ELEV,
            TERRAIN_MIN_RIVER_PRECIP,
        );
        let river_routes = fine.route_rivers(coarse_grid, &coarse_rivers);
        let river_set: BTreeSet<usize> = river_routes.iter().flatten().copied().collect();

        let nf = fine.grid.faces.len();
        let mut fine_is_land = Vec::with_capacity(nf);
        let mut fine_is_river = Vec::with_capacity(nf);
        for idx in 0..nf {
            fine_is_land.push(fine.fine_elevations_km[idx] > 0.0);
            fine_is_river.push(river_set.contains(&idx));
        }
        let fine_primary_coarse = fine.primary_coarse.clone();

        let radius_m = (prehistory.radius_km() * 1000.0).max(1_000_000.0);
        let day_hours = 24.0 / prehistory.spin().max(0.1);
        let geom = FiniteVolumeSphereGeom::new(&fine.grid, &fine.adjacency, radius_m, day_hours);

        let (
            home_star,
            atmosphere,
            optics,
            has_atmosphere,
            has_life,
            default_moon_count,
            default_moon_mass_lunar,
            default_moon_dist_radii,
            solar_system,
            constellations,
        ) = if let Some(env_proto) = prehistory.saved_environment().cloned() {
            let dummy_snap = tec11_proto::PlanetSnapshot {
                rng_state_16b: Vec::new(),
                grid_size: prehistory.grid().size,
                radius_km: prehistory.radius_km() as f32,
                spin: prehistory.spin() as f32,
                tilt_deg: prehistory.tilt_deg() as f32,
                stage: None,
                environment: Some(env_proto),
            };
            let sky = ReconstructedSkyEnvironment::from_snapshot(
                &dummy_snap,
                prehistory.radius_km() as f32,
                prehistory.land_percent(),
            );
            (
                sky.home_star,
                sky.atmosphere,
                sky.optics,
                sky.has_atmosphere,
                sky.has_life,
                sky.moon_count,
                sky.moon_mass_lunar,
                sky.current_moon_dist_radii,
                sky.solar_system,
                sky.constellations,
            )
        } else {
            let home_star = HomeStar::sol();
            let atmosphere = AtmosphereType::EarthLikeN2O2;
            let optics = PlanetaryOptics::compute(&home_star, atmosphere);
            let mut sys_rng = Pcg64Mcg::from_seed_u64(0x517C_C1B7_2722_0A95);
            let ecliptic_pole = [0.0, -0.398, 0.917];
            let mut solar_system =
                generate_solar_system_with_rng(&mut sys_rng, &home_star, ecliptic_pole, true);
            substitute_simulated_planet(
                &mut solar_system,
                &home_star,
                prehistory.radius_km() as f32,
                prehistory.land_percent(),
                1,
                ecliptic_pole,
            );
            let constellations = generate_default_constellations(42);
            (
                home_star,
                atmosphere,
                optics,
                true,
                true,
                1,
                1.0,
                60.27,
                solar_system,
                constellations,
            )
        };

        // Classify any submarine coarse tile that hosts an emergent sub-grid island (`CoarseFace(c)` > 0 km)
        // directly through `ClimateModel::classify_emergent_island` at its emergent summit elevation.
        let mut effective_coarse_koeppen = koeppen_ids.to_vec();
        let mut effective_coarse_life = prehistory.tile_life().to_vec();
        for c in 0..coarse_grid.faces.len() {
            if effective_coarse_koeppen.get(c).copied().unwrap_or(0) == 0 {
                if let Some(&summit_f) = fine.coarse_to_fine_children[c].iter().find(|&&f| {
                    matches!(fine.parentage[f], FineParentage::CoarseFace(_)) && fine_is_land[f]
                }) {
                    let isl_clim = prehistory.climate_model().classify_emergent_island(
                        c,
                        fine.fine_elevations_km[summit_f],
                        has_life,
                    );
                    if c < effective_coarse_koeppen.len() {
                        effective_coarse_koeppen[c] = isl_clim.koeppen as u8;
                    }
                    if c < effective_coarse_life.len() {
                        effective_coarse_life[c] = if isl_clim.life { 1.0 } else { 0.0 };
                    }
                }
            }
        }

        Self {
            prehistory,
            fine,
            geom,
            fine_is_land,
            fine_is_river,
            fine_primary_coarse,
            effective_coarse_koeppen,
            effective_coarse_life,
            river_routes,
            home_star,
            atmosphere,
            optics,
            has_atmosphere,
            has_life,
            default_moon_count,
            default_moon_mass_lunar,
            default_moon_dist_radii,
            solar_system,
            constellations,
        }
    }

    pub fn set_atmosphere(&mut self, atmosphere: AtmosphereType) {
        self.atmosphere = atmosphere;
        self.optics = PlanetaryOptics::compute(&self.home_star, atmosphere);
    }

    pub fn day_hours(&self) -> f64 {
        24.0 / self.prehistory.spin().max(0.1)
    }

    pub fn days_per_year(&self) -> f64 {
        let spin = self.prehistory.spin().max(0.1) as f32;
        let earth_days = self
            .solar_system
            .iter()
            .find(|p| p.is_home_world)
            .map(|p| p.orbital_period_days)
            .unwrap_or(self.home_star.orbital_period_days)
            .max(5.0);
        ((earth_days * spin) as f64).clamp(8.0, 5000.0)
    }

    pub fn home_init_angle_rad(&self) -> f64 {
        self.solar_system
            .iter()
            .find(|p| p.is_home_world)
            .map(|p| p.orbit_angle_rad as f64)
            .unwrap_or(0.0)
    }

    pub fn planet_name(&self) -> String {
        if self.prehistory.is_custom_planet() {
            format!(
                "Custom Planet ({} geoticks, {}% land)",
                self.prehistory.tectonics_ticks(),
                self.prehistory.land_percent()
            )
        } else {
            "Earth (ETOPO2 Bathymetry & Relief)".to_string()
        }
    }

    pub fn flora_and_fauna_for_coarse(&mut self, c_idx: usize) -> (Vec<String>, Vec<String>) {
        if !self.has_atmosphere || !self.has_life {
            return (Vec::new(), Vec::new());
        }
        let (mut flora, mut fauna) = self
            .prehistory
            .salient_flora_and_fauna_for_tile(c_idx, 8, 8);
        // If `c_idx` is a submarine coarse tile with an emergent sub-grid volcanic/atoll island,
        // disperse terrestrial island flora & fauna from the nearest compatible mainland tile.
        let is_island_parent = self
            .prehistory
            .tiles()
            .get(c_idx)
            .is_some_and(|t| t.elevation() <= 0.0)
            && self.effective_coarse_koeppen.get(c_idx).copied().unwrap_or(0) != 0;
        if is_island_parent {
            let target_k = self.effective_coarse_koeppen[c_idx];
            if target_k != 12 {
                let faces = &self.prehistory.grid().faces;
                let p = faces.get(c_idx).map(|f| f.0).unwrap_or([1.0, 0.0, 0.0]);
                let raw_k = self.prehistory.tile_koeppen_ids();
                let mut best_land: Option<(f64, usize)> = None;
                for (cand, &(q, _)) in faces.iter().enumerate() {
                    let ck = raw_k.get(cand).copied().unwrap_or(0);
                    if ck == 0 || ck == 12 {
                        continue;
                    }
                    let dot = p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
                    let klim_bonus = if ck == target_k { 0.18 } else { 0.0 };
                    let score = dot + klim_bonus;
                    if best_land.is_none_or(|(best_s, _)| score > best_s) {
                        best_land = Some((score, cand));
                    }
                }
                if let Some((_, donor)) = best_land {
                    let (d_fl, d_fa) = self
                        .prehistory
                        .salient_flora_and_fauna_for_tile(donor, 5, 4);
                    for s in d_fl {
                        if flora.len() < 8 && !flora.contains(&s) {
                            flora.push(s);
                        }
                    }
                    for s in d_fa {
                        if fauna.len() < 8 && !fauna.contains(&s) {
                            fauna.push(s);
                        }
                    }
                }
            }
        }
        (flora, fauna)
    }

    /// True-color surface RGB (`[0..1]`) for fine cell `fine_idx` based on Köppen biome, rock felsity,
    /// planetary sky & foliage optics (`self.optics`), elevation relief, rivers, and `snow_depth_m`.
    pub fn true_color_land_or_sea_rgb(&self, fine_idx: usize, snow_depth_m: f32) -> [f32; 3] {
        let elev_km = self.fine.fine_elevations_km[fine_idx];
        if elev_km <= 0.0 {
            if !self.has_atmosphere {
                let s = ((7.5 + elev_km) / 7.5).clamp(0.12, 1.0);
                return [0.24 * s, 0.27 * s, 0.34 * s];
            }
            let depth_km = (-elev_km).clamp(0.0, 6.5);
            let shelf = (1.0 - (depth_km / 0.25).min(1.0)).powi(2);
            let abyss = (depth_km / 5.2).clamp(0.0, 1.0);
            let sky = self.optics.sky_zenith_rgb;
            let open_r = 0.02 + 0.34 * sky[0];
            let open_g = 0.06 + 0.48 * sky[1];
            let open_b = 0.14 + 0.56 * sky[2];
            let depth_scale_r = 1.0 + 0.35 * shelf - 0.52 * abyss;
            let depth_scale_g = 1.0 + 0.42 * shelf - 0.50 * abyss;
            let depth_scale_b = 1.0 + 0.28 * shelf - 0.38 * abyss;
            let r = (open_r * depth_scale_r).clamp(0.01, 1.0);
            let g = (open_g * depth_scale_g).clamp(0.02, 1.0);
            let b = (open_b * depth_scale_b).clamp(0.04, 1.0);
            return [r, g, b];
        }

        let c_idx = self.fine_primary_coarse[fine_idx];
        let koeppen = self
            .effective_coarse_koeppen
            .get(c_idx)
            .copied()
            .unwrap_or(0);
        let felsity = self
            .prehistory
            .tiles()
            .get(c_idx)
            .map(|t| t.top_felsity())
            .unwrap_or(0.55)
            .clamp(0.0, 1.0);

        let rock_rgb = [
            0.34 + 0.28 * felsity,
            0.31 + 0.24 * felsity,
            0.28 + 0.20 * felsity,
        ];
        if !self.has_atmosphere {
            return rock_rgb;
        }
        let fol = self.optics.plant_foliage_rgb;
        let hi = self.optics.plant_highlight_rgb;

        let glac = self
            .prehistory
            .climate_model()
            .tile_glacier_cover()
            .get(c_idx)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let (raw_veg_w, mut base_rgb) = if glac >= 0.50 {
            (0.0, [0.88_f32, 0.92, 0.96]) // Active glacier / ice sheet
        } else {
            match koeppen {
                1 => (0.90, [fol[0] * 0.84, fol[1] * 0.84, fol[2] * 0.84]), // Af rainforest
                2 => (
                    0.82,
                    [
                        0.72 * fol[0] + 0.18 * hi[0],
                        0.72 * fol[1] + 0.18 * hi[1],
                        0.72 * fol[2] + 0.18 * hi[2],
                    ],
                ), // Am monsoon
                3 => (
                    0.62,
                    [
                        0.50 * fol[0] + 0.18 * hi[0] + 0.24,
                        0.50 * fol[1] + 0.18 * hi[1] + 0.20,
                        0.50 * fol[2] + 0.18 * hi[2] + 0.12,
                    ],
                ), // Aw savanna
                4 => (
                    0.28,
                    [
                        0.38 * fol[0] + 0.40,
                        0.38 * fol[1] + 0.35,
                        0.38 * fol[2] + 0.23,
                    ],
                ), // BS steppe
                5 => (0.05, [0.78, 0.67, 0.46]), // BW desert
                6 | 7 | 8 => (
                    0.74,
                    [
                        0.58 * fol[0] + 0.30 * hi[0],
                        0.58 * fol[1] + 0.30 * hi[1],
                        0.58 * fol[2] + 0.30 * hi[2],
                    ],
                ),
                9 | 10 | 11 => (0.68, [fol[0] * 0.68, fol[1] * 0.68, fol[2] * 0.68]), // Boreal taiga
                _ => (
                    0.45,
                    [
                        0.55 * fol[0] + 0.20,
                        0.55 * fol[1] + 0.18,
                        0.55 * fol[2] + 0.15,
                    ],
                ),
            }
        };
        if glac > 0.05 && glac < 0.50 {
            let snow_w = (glac * 1.4).clamp(0.0, 0.70);
            for k in 0..3 {
                base_rgb[k] = snow_w * [0.86, 0.90, 0.94][k] + (1.0 - snow_w) * base_rgb[k];
            }
        }
        let veg_w = if self.has_life { raw_veg_w } else { 0.0 };

        let alpine = ((elev_km - 2.6) / 2.2).clamp(0.0, 1.0);
        let eff_veg = veg_w * (1.0 - 0.85 * alpine);
        for k in 0..3 {
            base_rgb[k] = eff_veg * base_rgb[k] + (1.0 - eff_veg) * rock_rgb[k];
        }

        // Subtle hypsometric shading for high mountain ridges
        let relief_boost = (elev_km / 6.0).clamp(0.0, 0.25);
        for c in &mut base_rgb {
            *c = (*c * (0.94 + relief_boost)).clamp(0.0, 1.0);
        }

        if self.fine_is_river[fine_idx] {
            let sky = self.optics.sky_zenith_rgb;
            let riv = [
                (0.04 + 0.42 * sky[0]).clamp(0.02, 0.90),
                (0.09 + 0.54 * sky[1]).clamp(0.04, 0.92),
                (0.18 + 0.64 * sky[2]).clamp(0.08, 0.95),
            ];
            base_rgb = [
                0.35 * base_rgb[0] + 0.65 * riv[0],
                0.35 * base_rgb[1] + 0.65 * riv[1],
                0.35 * base_rgb[2] + 0.65 * riv[2],
            ];
        }

        let snow_cover = (snow_depth_m / 0.08).clamp(0.0, 1.0);
        if snow_cover > 0.001 {
            let snow_rgb = [0.93, 0.96, 0.99];
            for k in 0..3 {
                base_rgb[k] = (1.0 - snow_cover) * base_rgb[k] + snow_cover * snow_rgb[k];
            }
        }
        base_rgb
    }

    pub fn stratigraphy_json_for_fine(&self, fine_idx: usize) -> String {
        let Some(&c_idx) = self.fine_primary_coarse.get(fine_idx) else {
            return "{\"hasColumn\":false}".to_string();
        };
        let fine_elev = self
            .fine
            .fine_elevations_km
            .get(fine_idx)
            .copied()
            .unwrap_or(0.0);
        self.prehistory
            .tiles()
            .get(c_idx)
            .map(|t| t.to_stratigraphy_json(c_idx, Some(fine_elev)))
            .unwrap_or_else(|| "{\"hasColumn\":false}".to_string())
    }
}

/// Standalone Dynamic Astronomical Tides simulation (`TidesScreen`) on a uniform `coarse + 2` grid.
pub struct TidesSandbox {
    pub base: FinePlanetBase,
    pub solver: LaplaceTidalSolver,
}

impl TidesSandbox {
    pub fn new_earth() -> Self {
        Self::from_base(FinePlanetBase::new_earth(), 1, 1.0, 60.27)
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Option<Self> {
        let base = FinePlanetBase::from_snapshot_bytes(bytes)?;
        let mc = base.default_moon_count;
        let mm = base.default_moon_mass_lunar;
        let md = base.default_moon_dist_radii;
        Some(Self::from_base(base, mc, mm, md))
    }

    pub fn from_base(
        base: FinePlanetBase,
        moon_count: u32,
        primary_mass_lunar: f32,
        primary_dist_radii: f32,
    ) -> Self {
        let day_hours = base.day_hours();
        let tilt_deg = base.prehistory.tilt_deg();
        let days_per_year = base.days_per_year();
        let home_init = base.home_init_angle_rad();
        let solar_factor = base.home_star.solar_tide_factor as f64;
        let bed_m = base.fine.fine_bed_m.clone();
        let solver = LaplaceTidalSolver::new(
            &base.fine.grid,
            &base.geom,
            bed_m,
            day_hours,
            tilt_deg,
            days_per_year,
            home_init,
            solar_factor,
            moon_count,
            primary_mass_lunar,
            primary_dist_radii,
        );
        Self { base, solver }
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Some(base) = FinePlanetBase::from_snapshot_bytes(bytes) else {
            return false;
        };
        let mc = base.default_moon_count;
        let mm = base.default_moon_mass_lunar;
        let md = base.default_moon_dist_radii;
        *self = Self::from_base(base, mc, mm, md);
        true
    }

    pub fn reset_tides(&mut self) {
        self.solver.reset_state(&self.base.geom);
    }

    pub fn step_hours(&mut self, hours: u32) {
        let steps = hours.clamp(1, 240);
        for _ in 0..steps {
            self.solver.step_hour(&self.base.geom, None);
        }
    }

    pub fn set_day_of_year(&mut self, day: u32) {
        self.solver.set_day_of_year(day);
    }

    pub fn set_moon_params(&mut self, count: u32, primary_mass: f32, primary_dist_radii: f32) {
        self.solver
            .set_moon_params(count, primary_mass, primary_dist_radii);
    }

    pub fn sim_clock_label(&self) -> String {
        let (year, day, utc_tau) = self.solver.current_sim_calendar();
        let day_h = self.base.day_hours();
        let total_h = utc_tau * day_h;
        let hh = total_h.floor() as u32;
        let mm = ((total_h.fract() * 60.0).floor() as u32).min(59);
        if year > 0 {
            format!(
                "Yr {}, Day {} · {:02}:{:02} UTC (+{}h)",
                year,
                day,
                hh,
                mm,
                self.solver.elapsed_hours()
            )
        } else {
            format!(
                "Day {} · {:02}:{:02} UTC (+{}h)",
                day,
                hh,
                mm,
                self.solver.elapsed_hours()
            )
        }
    }

    pub fn max_tide_abs_m(&self) -> f32 {
        self.solver
            .eta_m
            .iter()
            .fold(0.0_f32, |acc, &v| acc.max(v.abs()))
    }

    pub fn max_tidal_range_m(&self) -> f32 {
        self.solver
            .tidal_range_m
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
    }

    pub fn max_current_mps(&self) -> f32 {
        self.solver
            .speed_mps
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
    }

    pub fn tile_colors_rgb(&self, mode: &str, selected_tile: i32) -> Vec<f32> {
        let n = self.base.fine.grid.faces.len();
        let sel = (selected_tile >= 0).then_some(selected_tile as usize);
        let sun_dir = self.solver.subsolar_unit_xyz(0.0);
        let mut out = Vec::with_capacity(n * 3);

        for i in 0..n {
            if sel == Some(i) {
                out.extend_from_slice(&[1.0, 0.15, 0.15]);
                continue;
            }
            let p = self.base.fine.grid.faces[i].0;
            let plen = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-12);
            let cos_sun = ((p[0] * sun_dir[0] + p[1] * sun_dir[1] + p[2] * sun_dir[2]) / plen) as f32;
            let day_shade = 0.62 + 0.38 * (cos_sun * 2.5).clamp(-0.35, 1.0).max(0.0);

            let is_wet = self.solver.h_total_m[i] > 0.05;
            let is_inundated = self.solver.is_inundated_land(i);

            let rgb = match mode {
                "tidal_range" => {
                    if !is_wet && self.base.fine_is_land[i] {
                        let base_c = self.base.true_color_land_or_sea_rgb(i, 0.0);
                        [base_c[0] * 0.55, base_c[1] * 0.55, base_c[2] * 0.55]
                    } else {
                        let rng_m = self.solver.tidal_range_m[i];
                        viridis_plasma_range_rgb(rng_m)
                    }
                }
                "current_speed" => {
                    if !is_wet {
                        let base_c = self.base.true_color_land_or_sea_rgb(i, 0.0);
                        [base_c[0] * 0.50, base_c[1] * 0.50, base_c[2] * 0.50]
                    } else {
                        let spd = self.solver.speed_mps[i];
                        current_speed_rgb(spd)
                    }
                }
                "bathymetry" => {
                    let elev_km = self.base.fine.fine_elevations_km[i];
                    if elev_km > 0.0 {
                        let base_c = self.base.true_color_land_or_sea_rgb(i, 0.0);
                        [base_c[0] * day_shade, base_c[1] * day_shade, base_c[2] * day_shade]
                    } else {
                        bathymetry_hypsometric_rgb(elev_km)
                    }
                }
                _ => {
                    // Default `"live_eta"`: diverging sea-surface anomaly + cyan inundation highlight + sunlit land
                    if is_inundated {
                        [0.08, 0.92, 0.96]
                    } else if !is_wet {
                        let base_c = self.base.true_color_land_or_sea_rgb(i, 0.0);
                        [base_c[0] * day_shade, base_c[1] * day_shade, base_c[2] * day_shade]
                    } else {
                        let eta = self.solver.eta_m[i];
                        live_tide_diverging_rgb(eta, day_shade)
                    }
                }
            };
            out.extend_from_slice(&rgb);
        }
        out
    }

    /// Returns 3D velocity vectors `(vx, vy, vz)` on the `CoarseFace` subset of fine cells so
    /// 3D globe arrows are evenly spaced without polar crowding.
    pub fn sampled_tidal_arrows(&self) -> Vec<(usize, [f32; 3], f32)> {
        let mut arrows = Vec::with_capacity(self.base.prehistory.tile_count());
        let max_spd = self.max_current_mps().max(0.15);
        for (i, parent) in self.base.fine.parentage.iter().enumerate() {
            if !matches!(parent, FineParentage::CoarseFace(_)) {
                continue;
            }
            if self.solver.h_total_m[i] <= 0.05 {
                continue;
            }
            let spd = self.solver.speed_mps[i];
            if spd < 0.008 {
                continue;
            }
            let v3 = self.base.geom.local_uv_to_xyz(
                i,
                self.solver.u_east[i],
                self.solver.v_north[i],
            );
            let norm_spd = (spd / max_spd).clamp(0.05, 1.0);
            arrows.push((i, v3, norm_spd));
        }
        arrows
    }

    pub fn inspect_tile_structured(&mut self, fine_idx: usize) -> Option<TileInspectionView> {
        if fine_idx >= self.base.fine.grid.faces.len() {
            return None;
        }
        let c_idx = self.base.fine_primary_coarse[fine_idx];
        let (flora, fauna) = self.base.flora_and_fauna_for_coarse(c_idx);

        let elev_m = self.base.fine.fine_bed_m[fine_idx];
        let h_m = self.solver.h_total_m[fine_idx];
        let eta_m = self.solver.eta_m[fine_idx];
        let range_m = self.solver.tidal_range_m[fine_idx];
        let spd = self.solver.speed_mps[fine_idx];
        let koeppen = self
            .base
            .effective_coarse_koeppen
            .get(c_idx)
            .copied()
            .unwrap_or(0);

        let (year, day, utc_tau) = self.solver.current_sim_calendar();
        let lon_rad = self.base.geom.lons_rad[fine_idx] as f64;
        let local_tau = (utc_tau + lon_rad / (2.0 * std::f64::consts::PI)).rem_euclid(1.0) as f32;
        let local_h = (local_tau as f64) * self.base.day_hours();
        let l_hh = local_h.floor() as u32;
        let l_mm = ((local_h.fract() * 60.0).floor() as u32).min(59);

        let loc_summary = if elev_m <= 0.0 {
            format!(
                "Ocean Depth: {:.0} m | Tide η: {:+.2} m | 28h Range: {:.2} m | Current: {:.2} m/s",
                -elev_m, eta_m, range_m, spd
            )
        } else if self.solver.is_inundated_land(fine_idx) {
            format!(
                "INUNDATED COAST (+{:.1} m bed, {:.2} m water) | Tide η: {:+.2} m | {}",
                elev_m,
                h_m,
                eta_m,
                koeppen_climate_name(koeppen)
            )
        } else {
            format!(
                "{} | Elevation: +{:.0} m | Coarse Parent #{}",
                koeppen_climate_name(koeppen),
                elev_m,
                c_idx
            )
        };

        let tide_summary = if h_m > 0.05 {
            format!(
                "Water col: {:.1} m · Diss: {:.3} W/m²",
                h_m, self.solver.dissipation_wm2[fine_idx]
            )
        } else {
            "Dry Land (Above High Tide)".to_string()
        };

        let strat_json = self.base.stratigraphy_json_for_fine(fine_idx);
        let live_env = TileLiveEnvironment {
            sim_locked: true,
            sim_year: year,
            sim_day: day,
            sim_hour_frac: local_tau,
            sim_clock_label: format!("Day {day} · {l_hh:02}:{l_mm:02} Local"),
            tide_offset_m: eta_m,
            tidal_range_m: range_m,
            cloud_low: 0.0,
            cloud_high: 0.0,
            rain_mm_hr: 0.0,
            snow_mm_hr: 0.0,
            snow_depth_m: 0.0,
            wind_speed_mps: 0.0,
            wind_az_deg: 0.0,
            storm_darkening: 0.0,
        };

        let mut view = build_uniform_fine_tile_inspection_view(
            fine_idx,
            UniformFineTileSkyParams {
                fine_grid: &self.base.fine.grid,
                fine_adjacency: &self.base.fine.adjacency,
                fine_elev_km: &self.base.fine.fine_elevations_km,
                fine_mountainosity_norm: &self.base.fine.fine_mountainosity_norm,
                fine_is_land: &self.base.fine_is_land,
                fine_is_river: &self.base.fine_is_river,
                fine_primary_coarse: &self.base.fine_primary_coarse,
                coarse_tiles: self.base.prehistory.tiles(),
                coarse_koeppen_ids: &self.base.effective_coarse_koeppen,
                coarse_glacier_cover: self.base.prehistory.climate_model().tile_glacier_cover(),
                coarse_tile_life: &self.base.effective_coarse_life,
                radius_km: self.base.prehistory.radius_km(),
                day_hours: self.base.day_hours(),
                tilt_deg: self.base.prehistory.tilt_deg(),
                has_atmosphere: self.base.has_atmosphere,
                has_life: self.base.has_life,
                current_year: year,
                moon_count: self.solver.moon_count(),
                home_star: &self.base.home_star,
                optics: &self.base.optics,
                solar_system: &self.base.solar_system,
                constellations: &self.base.constellations,
                title: format!("Fine Tile #{fine_idx} (Size {})", self.base.fine.grid.size),
                location_summary: loc_summary,
                population_summary: tide_summary,
                technology: format!(
                    "Moons: {} (M={:.2}x, d={:.1} Rp)",
                    self.solver.moon_count(),
                    self.solver.primary_mass_lunar(),
                    self.solver.primary_dist_radii()
                ),
                stratigraphy_json: strat_json,
                live_env: Some(live_env),
            },
        )?;

        // Sync SkyVantage moon periods with exact Keplerian distance-scaled periods in LaplaceTidalSolver:
        view.sky.moons = self
            .solver
            .moon_bodies()
            .iter()
            .map(|m| TileSkyMoon {
                index: m.index,
                name: format!("Moon {}", m.index + 1),
                period_days: m.period_local_days,
                inclination_deg: m.inclination_deg,
                init_angle_rad: m.init_angle_rad,
                node_asc_rad: m.node_asc_rad,
            })
            .collect();
        view.flora = flora;
        view.fauna = fauna;
        Some(view)
    }

    /// Formats the 48-hour Tide Gauge sparkline payload consumed by `paintSparklineCanvas`.
    pub fn tile_tide_series_json(&self, fine_idx: usize) -> String {
        if fine_idx >= self.base.fine.grid.faces.len() {
            return "{\"series\":[]}".to_string();
        }
        let vals = self.solver.cell_eta_series_m(fine_idx);
        let cur = self.solver.eta_m[fine_idx];
        let rng = self.solver.tidal_range_m[fine_idx];
        let min_v = vals.iter().copied().fold(cur, f32::min);
        let max_v = vals.iter().copied().fold(cur, f32::max);
        let pad = ((max_v - min_v).abs() * 0.2).max(0.25);
        let y_min = (min_v - pad).min(-0.25);
        let y_max = (max_v + pad).max(0.25);

        let vals_str: Vec<String> = vals.iter().map(|v| format!("{v:.3}")).collect();
        let subtitle = format!(
            "28h Range: {rng:.2} m | Min {min_v:+.2} m .. Max {max_v:+.2} m ({}h recorded)",
            vals.len()
        );
        format!(
            "{{\"currentLabel\":{},\"subtitle\":{},\"minLabel\":{},\"maxLabel\":{},\"series\":[{{\"label\":\"Tide η (m)\",\"color\":\"#38bdf8\",\"fillColor\":\"rgba(56,189,248,0.22)\",\"zeroLine\":true,\"min\":{y_min:.3},\"max\":{y_max:.3},\"values\":[{}]}}]}}",
            json_escape(&format!("η = {cur:+.2} m")),
            json_escape(&subtitle),
            json_escape(&format!("Low: {min_v:+.2} m")),
            json_escape(&format!("High: {max_v:+.2} m")),
            vals_str.join(",")
        )
    }
}

#[inline]
pub(crate) fn live_tide_diverging_rgb(eta_m: f32, day_shade: f32) -> [f32; 3] {
    // Soft tanh normalization around +-1.2 m so open-ocean +-0.4 m waves AND +-3 m shelf tides both pop!
    let x = (eta_m / 0.85).tanh();
    let shade = 0.82 + 0.18 * day_shade;
    if x >= 0.0 {
        // Mean sea slate-blue -> warm gold -> bright coral-magenta for extreme high tide / surge
        let t = x;
        if t < 0.55 {
            let u = t / 0.55;
            [
                (0.08 + 0.82 * u) * shade,
                (0.24 + 0.54 * u) * shade,
                (0.46 - 0.24 * u) * shade,
            ]
        } else {
            let u = (t - 0.55) / 0.45;
            [
                (0.90 + 0.10 * u) * shade,
                (0.78 - 0.46 * u) * shade,
                (0.22 + 0.56 * u) * shade,
            ]
        }
    } else {
        // Mean sea slate-blue -> deep cyan-teal -> indigo trough for low tide
        let t = -x;
        if t < 0.55 {
            let u = t / 0.55;
            [
                (0.08 - 0.04 * u) * shade,
                (0.24 + 0.42 * u) * shade,
                (0.46 + 0.44 * u) * shade,
            ]
        } else {
            let u = (t - 0.55) / 0.45;
            [
                (0.04 + 0.14 * u) * shade,
                (0.66 - 0.48 * u) * shade,
                (0.90 + 0.08 * u) * shade,
            ]
        }
    }
}

#[inline]
fn viridis_plasma_range_rgb(range_m: f32) -> [f32; 3] {
    // Amphidromic nodes (`range_m ~ 0`) glow dark navy-black; macrotidal bays (`> 2.5 m`) glow bright yellow-white.
    let t = (range_m / 2.6).sqrt().clamp(0.0, 1.0);
    if t < 0.33 {
        let u = t / 0.33;
        [0.04 + 0.18 * u, 0.05 + 0.26 * u, 0.18 + 0.44 * u]
    } else if t < 0.66 {
        let u = (t - 0.33) / 0.33;
        [0.22 + 0.56 * u, 0.31 + 0.30 * u, 0.62 - 0.28 * u]
    } else {
        let u = (t - 0.66) / 0.34;
        [0.78 + 0.22 * u, 0.61 + 0.36 * u, 0.34 - 0.14 * u]
    }
}

#[inline]
fn current_speed_rgb(spd_mps: f32) -> [f32; 3] {
    let t = ((spd_mps / 0.65).sqrt()).clamp(0.0, 1.0);
    [
        0.04 + 0.94 * t * t,
        0.10 + 0.78 * t * (1.0 - 0.25 * t),
        0.26 + 0.64 * (1.0 - (t - 0.45).abs() * 1.6).clamp(0.0, 1.0),
    ]
}

#[inline]
fn bathymetry_hypsometric_rgb(elev_km: f32) -> [f32; 3] {
    let depth_km = (-elev_km).clamp(0.0, 6.5);
    if depth_km < 0.25 {
        // Continental shelf: bright turquoise
        let u = depth_km / 0.25;
        [0.22 - 0.12 * u, 0.72 - 0.22 * u, 0.86 - 0.12 * u]
    } else {
        let u = ((depth_km - 0.25) / 5.75).clamp(0.0, 1.0);
        [0.10 - 0.08 * u, 0.50 - 0.42 * u, 0.74 - 0.50 * u]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tides_sandbox_earth_step_and_inspection() {
        let mut sb = TidesSandbox::new_earth();
        assert_eq!(sb.base.fine.grid.size, 8);
        assert_eq!(sb.base.fine.grid.faces.len(), 65_612);
        sb.step_hours(2);
        assert_eq!(sb.solver.elapsed_hours(), 2);
        assert!(sb.max_tide_abs_m() > 0.01);

        let colors = sb.tile_colors_rgb("live_eta", 10);
        assert_eq!(colors.len(), 65_612 * 3);

        let view = sb.inspect_tile_structured(10).expect("valid fine tile view");
        assert!(view.live_env.as_ref().unwrap().sim_locked);
        let series_json = sb.tile_tide_series_json(10);
        assert!(series_json.contains("\"series\""));
    }
}
