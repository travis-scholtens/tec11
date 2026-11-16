use tec11_climate::{MoistBaroclinicSolver, WeatherInitConfig, EARTH_GLACIATION};
use tec11_math_grid::FineParentage;

use crate::sky_vantage::{
    build_uniform_fine_tile_inspection_view, json_escape, TileInspectionView, TileLiveEnvironment,
    TileSkyMoon, UniformFineTileSkyParams,
};
use crate::sun_sky::AtmosphereType;
use crate::tides_sandbox::{live_tide_diverging_rgb, FinePlanetBase};
use crate::world::koeppen_climate_name;

/// Standalone Global Coupled Weather & Storm Surge simulation (`WeatherScreen`) on a uniform
/// `coarse + 2` spherical finite-volume grid.
pub struct WeatherSandbox {
    pub base: FinePlanetBase,
    pub solver: MoistBaroclinicSolver,
}

impl WeatherSandbox {
    pub fn new_earth() -> Self {
        Self::from_base(
            FinePlanetBase::new_earth(),
            0,
            EARTH_GLACIATION as f32,
            1,
            1.0,
            60.27,
        )
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Option<Self> {
        let base = FinePlanetBase::from_snapshot_bytes(bytes)?;
        let glac = (base.prehistory.glaciation() as f32).clamp(0.0, 1.0);
        Some(Self::from_base(base, 0, glac, 1, 1.0, 60.27))
    }

    pub fn from_base(
        mut base: FinePlanetBase,
        atmosphere_id: u32,
        glaciation: f32,
        moon_count: u32,
        primary_mass_lunar: f32,
        primary_dist_radii: f32,
    ) -> Self {
        base.set_atmosphere(AtmosphereType::from_id(atmosphere_id));
        let solver = Self::build_solver_for_base(
            &base,
            atmosphere_id,
            glaciation,
            moon_count,
            primary_mass_lunar,
            primary_dist_radii,
        );
        Self { base, solver }
    }

    fn build_solver_for_base(
        base: &FinePlanetBase,
        atmosphere_id: u32,
        glaciation: f32,
        moon_count: u32,
        primary_mass_lunar: f32,
        primary_dist_radii: f32,
    ) -> MoistBaroclinicSolver {
        let nc = base.prehistory.tile_count();
        let nf = base.fine.grid.faces.len();

        // 1. Annual mean Stommel-Ekman SST anomaly `[-1..+1]` from `ClimateModel` -> physical °C SST:
        let cm = base.prehistory.climate_model();
        let mut coarse_sst_anom = vec![0.0_f32; nc];
        let mut season_cnt = 0usize;
        for s in 0..8 {
            if let Some(snap) = cm.season_snapshot(s) {
                if snap.ocean.sst_anomaly.len() == nc {
                    for c in 0..nc {
                        coarse_sst_anom[c] += snap.ocean.sst_anomaly[c];
                    }
                    season_cnt += 1;
                }
            }
        }
        if season_cnt > 0 {
            let inv_s = 1.0 / (season_cnt as f32);
            for v in &mut coarse_sst_anom {
                *v *= inv_s;
            }
        }

        // 2. Initial Manabe soil moisture & perennial icecap/tundra snowpack from coarse Köppen & precip:
        let koeppen_ids = base.prehistory.tile_koeppen_ids();
        let precip_norm = base.prehistory.tile_precip_norm();
        let mut coarse_soil = vec![0.45_f32; nc];
        let mut coarse_snow_m = vec![0.0_f32; nc];
        for c in 0..nc {
            let k = koeppen_ids.get(c).copied().unwrap_or(0);
            let p = precip_norm.get(c).copied().unwrap_or(0.3).clamp(0.0, 1.5);
            coarse_soil[c] = match k {
                0 => 1.0,                                 // Ocean
                1 => 0.88,                                // Af rainforest
                2 => 0.80,                                // Am monsoon
                3 => 0.50,                                // Aw savanna
                4 => 0.24,                                // BS steppe
                5 => 0.06,                                // BW desert
                6 | 8 | 9 | 11 => (0.58 + 0.18 * p).min(0.85), // Humid temperate / boreal
                7 | 10 => (0.40 + 0.16 * p).min(0.70),    // Dry-summer Mediterranean / boreal
                12 => 0.72,                               // EF icecap
                13 => 0.56,                               // ET tundra
                _ => (0.35 + 0.25 * p).clamp(0.10, 0.85),
            };
            coarse_snow_m[c] = match k {
                12 => 1.40, // Perennial continental ice sheet / firn
                13 => 0.05, // Light tundra snow cover
                _ => 0.0,
            };
        }

        let tilt_deg = base.prehistory.tilt_deg();
        let glac_offset_c = tec11_climate::glaciation_temp_offset_c(glaciation);
        let mut base_sst_c = Vec::with_capacity(nf);
        let mut init_soil_moisture = Vec::with_capacity(nf);
        let mut init_snow_depth_m = Vec::with_capacity(nf);

        for i in 0..nf {
            let sin_phi = base.geom.sin_lat[i];
            let cos_phi = base.geom.cos_lat[i];
            let annual_mu = tec11_climate::orbital_annual_mean_cos_z(sin_phi, cos_phi, tilt_deg);
            let fine_sst_anom = base.fine.interpolate_coarse(i, &coarse_sst_anom);
            // Western boundary currents (+sst_anom) warm midlatitude east-coast waters up to +5.2 C,
            // while eastern boundary upwelling (-sst_anom) cools subtropical west coasts, driven
            // by tilt-general orbital insolation `annual_mu` rather than hardcoded `sin^2(lat)`:
            let sst = tec11_climate::equilibrium_open_sst_c(annual_mu, 1.0, fine_sst_anom);
            base_sst_c.push(sst);

            let bed_m = base.fine.fine_bed_m[i];
            if bed_m <= 0.0 {
                init_soil_moisture.push(1.0);
                init_snow_depth_m.push(0.0);
            } else {
                let fine_soil_interp = base.fine.interpolate_coarse(i, &coarse_soil);
                let fine_snow_interp = base.fine.interpolate_coarse(i, &coarse_snow_m);
                let river_boost = if base.fine_is_river[i] { 0.20 } else { 0.0 };
                init_soil_moisture.push((fine_soil_interp + river_boost).clamp(0.04, 0.95));

                let elev_km = base.fine.fine_elevations_km[i].max(0.0);
                let t_annual_est_c =
                    -20.0 + 160.0 * annual_mu - 6.5 * elev_km + glac_offset_c;
                let alpine_snow_m = if t_annual_est_c < -2.0 {
                    ((-2.0 - t_annual_est_c) * 0.015).clamp(0.0, 0.06)
                } else {
                    0.0
                };
                init_snow_depth_m.push(fine_snow_interp.max(alpine_snow_m).clamp(0.0, 2.0));
            }
        }

        let solar_tide_factor = (base.home_star.mass_solar
            / base.home_star.habitable_au.max(0.05).powi(3))
        .clamp(0.02, 20.0) as f64;

        MoistBaroclinicSolver::new(
            &base.fine.grid,
            &base.fine.adjacency,
            &base.geom,
            WeatherInitConfig {
                bed_m: &base.fine.fine_bed_m,
                mountainosity_norm: &base.fine.fine_mountainosity_norm,
                is_river: &base.fine_is_river,
                base_sst_c: &base_sst_c,
                init_soil_moisture: &init_soil_moisture,
                init_snow_depth_m: &init_snow_depth_m,
                day_hours: base.day_hours(),
                tilt_deg: base.prehistory.tilt_deg(),
                days_per_year: base.days_per_year(),
                home_init_angle_rad: base.home_init_angle_rad(),
                solar_tide_factor,
                solar_luminosity: 1.0,
                glaciation,
                atmosphere_id,
                moon_count,
                primary_mass_lunar,
                primary_dist_radii,
            },
        )
    }

    pub fn load_earth(&mut self) {
        let atm = self.solver.atmosphere_id();
        let mc = self.solver.tides.moon_count();
        let mm = self.solver.tides.primary_mass_lunar();
        let md = self.solver.tides.primary_dist_radii();
        *self = Self::from_base(
            FinePlanetBase::new_earth(),
            atm,
            EARTH_GLACIATION as f32,
            mc,
            mm,
            md,
        );
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Some(base) = FinePlanetBase::from_snapshot_bytes(bytes) else {
            return false;
        };
        let atm = self.solver.atmosphere_id();
        let glac = (base.prehistory.glaciation() as f32).clamp(0.0, 1.0);
        let mc = self.solver.tides.moon_count();
        let mm = self.solver.tides.primary_mass_lunar();
        let md = self.solver.tides.primary_dist_radii();
        *self = Self::from_base(base, atm, glac, mc, mm, md);
        true
    }

    pub fn reset_weather(&mut self) {
        self.solver.reset_state(&self.base.geom);
    }

    pub fn step_hours(&mut self, hours: u32) {
        let n = hours.clamp(1, 240);
        for _ in 0..n {
            self.solver.step_hour(&self.base.geom);
        }
    }

    pub fn set_day_of_year(&mut self, day: u32) {
        self.solver.set_day_of_year(day);
    }

    pub fn set_environment_params(
        &mut self,
        atmosphere_id: u32,
        glaciation: f32,
        moon_count: u32,
        primary_mass: f32,
        primary_dist_radii: f32,
    ) {
        let atm = AtmosphereType::from_id(atmosphere_id);
        self.base.set_atmosphere(atm);
        self.solver.set_environment_params(
            atm.id(),
            glaciation,
            moon_count,
            primary_mass,
            primary_dist_radii,
        );
    }

    pub fn sim_clock_label(&self) -> String {
        let (year, day, tau) = self.solver.tides.current_sim_calendar();
        let local_hour = (tau * self.base.day_hours()).rem_euclid(self.base.day_hours());
        let hh = local_hour.floor() as u32;
        let mm = ((local_hour.fract() * 60.0).floor() as u32).min(59);
        format!(
            "Year {} · Day {} · {:02}:{:02} UTC (+{}h)",
            year,
            day,
            hh,
            mm,
            self.solver.tides.elapsed_hours()
        )
    }

    pub fn global_mean_temp_c(&self) -> f32 {
        let mut sum_t = 0.0_f64;
        let mut sum_a = 0.0_f64;
        for (i, &t) in self.solver.tsurf_c.iter().enumerate() {
            let a = self.base.geom.areas_m2[i] as f64;
            sum_t += (t as f64) * a;
            sum_a += a;
        }
        (sum_t / sum_a.max(1.0)) as f32
    }

    pub fn min_slp_hpa(&self) -> f32 {
        self.solver
            .slp_hpa
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min)
    }

    pub fn max_slp_hpa(&self) -> f32 {
        self.solver
            .slp_hpa
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
    }

    pub fn max_wind_mps(&self) -> f32 {
        self.solver
            .wind1_speed_mps
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
    }

    pub fn max_jet_mps(&self) -> f32 {
        self.solver
            .wind2_speed_mps
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
    }

    pub fn max_precip_mm_hr(&self) -> f32 {
        self.solver
            .rain_mm_hr
            .iter()
            .zip(self.solver.snow_mm_hr.iter())
            .map(|(&r, &s)| r + s)
            .fold(0.0_f32, f32::max)
    }

    pub fn max_storm_surge_m(&self) -> f32 {
        self.solver
            .storm_surge_m
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max)
    }

    /// Per-fine-tile RGB (`3 * num_fine_cells`) for the 6 `WeatherScreen` map modes:
    /// - `"satellite"` (Default): True-color biome/ocean + snowpack + cyan storm inundation + semi-transparent cloud decks + precip radar + moving day/night terminator
    /// - `"pressure_fronts"`: Sea-level pressure anomaly + baroclinic frontal sharpness (`|\nabla \theta_1|`)
    /// - `"temperature"`: Surface skin temperature (`-40 .. +42 °C`)
    /// - `"precipitation"`: Cloud water + rainfall/snowfall radar + swollen river discharge
    /// - `"winds_jet"`: `300 hPa` upper-tropospheric jet stream isotachs (`0 .. 70 m/s`)
    /// - `"storm_surge"`: Coupled astronomical tide + meteorological storm surge & coastal inundation
    pub fn tile_colors_rgb(&self, mode: &str, selected_tile: i32) -> Vec<f32> {
        let nf = self.base.fine.grid.faces.len();
        let mut out = Vec::with_capacity(nf * 3);
        let sun_dir = self.solver.tides.subsolar_unit_xyz(0.0);

        // Reference sea-level pressure in hPa for `"pressure_fronts"` anomaly coloring:
        let p_ref_hpa = if !self.solver.slp_hpa.is_empty() {
            let mut sum_p = 0.0_f64;
            let mut sum_a = 0.0_f64;
            for (i, &p) in self.solver.slp_hpa.iter().enumerate() {
                let a = self.base.geom.areas_m2[i] as f64;
                sum_p += (p as f64) * a;
                sum_a += a;
            }
            (sum_p / sum_a.max(1.0)) as f32
        } else {
            1013.25
        };
        let sunset_rgb = self.base.optics.sunset_rgb;

        for i in 0..nf {
            let p_raw = self.base.fine.grid.faces[i].0;
            let plen = (p_raw[0] * p_raw[0] + p_raw[1] * p_raw[1] + p_raw[2] * p_raw[2])
                .sqrt()
                .max(1e-12);
            let cos_sun = ((p_raw[0] * sun_dir[0] + p_raw[1] * sun_dir[1] + p_raw[2] * sun_dir[2])
                / plen) as f32;
            let day_shade = ((cos_sun + 0.14) / 1.14).clamp(0.0, 1.0);
            let inundated = self.solver.tides.is_inundated_land(i);
            let is_land = self.base.fine_is_land[i];

            let mut rgb = match mode {
                "pressure_fronts" => {
                    let dp_hpa = self.solver.slp_hpa[i] - p_ref_hpa;
                    let front = self.solver.front_strength_k100km[i];
                    pressure_and_fronts_rgb(dp_hpa, front, is_land)
                }
                "temperature" => {
                    let tc = self.solver.tsurf_c[i];
                    temperature_thermal_rgb(tc, is_land)
                }
                "precipitation" => {
                    let base_surf = self
                        .base
                        .true_color_land_or_sea_rgb(i, self.solver.snow_depth_m[i]);
                    precipitation_radar_rgb(
                        base_surf,
                        is_land,
                        self.solver.cloud_low[i],
                        self.solver.cloud_high[i],
                        self.solver.rain_mm_hr[i],
                        self.solver.snow_mm_hr[i],
                        self.solver.river_flow_m3s[i],
                    )
                }
                "winds_jet" => {
                    let base_surf = self
                        .base
                        .true_color_land_or_sea_rgb(i, self.solver.snow_depth_m[i]);
                    jet_stream_isotach_rgb(base_surf, is_land, self.solver.wind2_speed_mps[i])
                }
                "storm_surge" => {
                    if inundated {
                        [0.08, 0.94, 0.98]
                    } else if is_land {
                        let base_land = self
                            .base
                            .true_color_land_or_sea_rgb(i, self.solver.snow_depth_m[i]);
                        let lit = 0.62 + 0.30 * day_shade;
                        [base_land[0] * lit, base_land[1] * lit, base_land[2] * lit]
                    } else {
                        // Combine live total sea-level `eta_m` with amplified storm-surge highlight
                        let eff_eta =
                            self.solver.tides.eta_m[i] + 1.8 * self.solver.storm_surge_m[i];
                        live_tide_diverging_rgb(eff_eta, day_shade)
                    }
                }
                _ => {
                    // Default `"satellite"` true-color view:
                    let mut surf = if inundated {
                        [0.08, 0.90, 0.96]
                    } else {
                        self.base
                            .true_color_land_or_sea_rgb(i, self.solver.snow_depth_m[i])
                    };

                    // Highlight swollen storm-fed rivers (`river_flow_m3s > 300 m^3/s`):
                    if is_land && !inundated && self.solver.river_flow_m3s[i] > 300.0 {
                        let rf = ((self.solver.river_flow_m3s[i] - 300.0) / 3500.0).clamp(0.0, 0.78);
                        surf[0] = (1.0 - rf) * surf[0] + rf * 0.14;
                        surf[1] = (1.0 - rf) * surf[1] + rf * 0.68;
                        surf[2] = (1.0 - rf) * surf[2] + rf * 0.92;
                    }

                    // Moving solar terminator with warm atmospheric twilight belt & cool nightside ambient:
                    let sun_lit = ((cos_sun + 0.04) / 0.24).clamp(0.0, 1.0);
                    let noon_boost = 0.84 + 0.24 * cos_sun.max(0.0);
                    let twi = (1.0 - (cos_sun.abs() / 0.16)).clamp(0.0, 1.0);
                    let night_rgb = [0.20_f32, 0.24, 0.34];
                    let mut lit_surf = [0.0_f32; 3];
                    for k in 0..3 {
                        let day_c = surf[k] * noon_boost;
                        let night_c = surf[k] * night_rgb[k];
                        let base_c = night_c + sun_lit * (day_c - night_c);
                        lit_surf[k] = (base_c + 0.16 * twi * sunset_rgb[k] * surf[k]).clamp(0.0, 1.0);
                    }

                    // Semi-transparent low & high cloud decks shaded by terminator:
                    let c_low = self.solver.cloud_low[i];
                    let c_high = self.solver.cloud_high[i];
                    let cloud_alpha = (0.72 * c_low + 0.48 * c_high * (1.0 - 0.55 * c_low))
                        .clamp(0.0, 0.88);
                    let cloud_lit = 0.28 + 0.76 * ((cos_sun + 0.10) / 1.10).clamp(0.0, 1.0);
                    let cloud_rgb = [
                        (0.95 * cloud_lit + 0.14 * twi * sunset_rgb[0]).clamp(0.0, 1.0),
                        (0.97 * cloud_lit + 0.12 * twi * sunset_rgb[1]).clamp(0.0, 1.0),
                        (1.00 * cloud_lit + 0.10 * twi * sunset_rgb[2]).clamp(0.0, 1.0),
                    ];
                    for k in 0..3 {
                        lit_surf[k] =
                            (1.0 - cloud_alpha) * lit_surf[k] + cloud_alpha * cloud_rgb[k];
                    }

                    // Subtle active precipitation glint inside storm cores:
                    let rain = self.solver.rain_mm_hr[i];
                    let snow = self.solver.snow_mm_hr[i];
                    if rain > 0.35 {
                        let rw = ((rain - 0.35) / 5.5).clamp(0.0, 0.45);
                        lit_surf[0] = (1.0 - rw) * lit_surf[0] + rw * 0.22;
                        lit_surf[1] = (1.0 - rw) * lit_surf[1] + rw * 0.82;
                        lit_surf[2] = (1.0 - rw) * lit_surf[2] + rw * 0.98;
                    }
                    if snow > 0.25 {
                        let sw = ((snow - 0.25) / 4.0).clamp(0.0, 0.50);
                        lit_surf[0] = (1.0 - sw) * lit_surf[0] + sw * 0.84;
                        lit_surf[1] = (1.0 - sw) * lit_surf[1] + sw * 0.89;
                        lit_surf[2] = (1.0 - sw) * lit_surf[2] + sw * 1.00;
                    }
                    lit_surf
                }
            };

            if selected_tile >= 0 && (i as i32) == selected_tile {
                rgb = [
                    (rgb[0] * 0.42 + 0.58).min(1.0),
                    (rgb[1] * 0.42 + 0.52).min(1.0),
                    (rgb[2] * 0.42 + 0.14).min(1.0),
                ];
            }
            out.extend_from_slice(&[
                rgb[0].clamp(0.0, 1.0),
                rgb[1].clamp(0.0, 1.0),
                rgb[2].clamp(0.0, 1.0),
            ]);
        }
        out
    }

    /// Sampled 3D velocity vectors `(fine_idx, velocity_xyz, norm_speed)` on coarse-face parent sites
    /// (`7,292` evenly spaced anchors on size-6 Earth) for `"surface_wind"`, `"jet_stream"`, or `"tidal_current"`.
    pub fn sampled_vector_arrows(&self, arrow_mode: &str) -> Vec<(usize, [f32; 3], f32)> {
        if arrow_mode == "none" || arrow_mode.is_empty() {
            return Vec::new();
        }
        let nf = self.base.fine.grid.faces.len();
        let mut arrows = Vec::new();

        match arrow_mode {
            "jet_stream" => {
                let max_spd = self.max_jet_mps().max(26.0);
                for i in 0..nf {
                    if !matches!(self.base.fine.parentage[i], FineParentage::CoarseFace(_)) {
                        continue;
                    }
                    let spd = self.solver.wind2_speed_mps[i];
                    if spd < 2.5 {
                        continue;
                    }
                    let v3 = self.base.geom.local_uv_to_xyz(
                        i,
                        self.solver.u2_east[i],
                        self.solver.v2_north[i],
                    );
                    arrows.push((i, v3, (spd / max_spd).clamp(0.06, 1.0)));
                }
            }
            "tidal_current" => {
                let max_spd = self
                    .solver
                    .tides
                    .speed_mps
                    .iter()
                    .copied()
                    .fold(0.0_f32, f32::max)
                    .max(0.25);
                for i in 0..nf {
                    if !matches!(self.base.fine.parentage[i], FineParentage::CoarseFace(_)) {
                        continue;
                    }
                    if self.solver.tides.h_total_m[i] <= 0.05 {
                        continue;
                    }
                    let spd = self.solver.tides.speed_mps[i];
                    if spd < 0.008 {
                        continue;
                    }
                    let v3 = self.base.geom.local_uv_to_xyz(
                        i,
                        self.solver.tides.u_east[i],
                        self.solver.tides.v_north[i],
                    );
                    arrows.push((i, v3, (spd / max_spd).clamp(0.05, 1.0)));
                }
            }
            _ => {
                // Default `"surface_wind"` (`~800 hPa` / surface boundary-layer winds):
                let max_spd = self.max_wind_mps().max(12.0);
                for i in 0..nf {
                    if !matches!(self.base.fine.parentage[i], FineParentage::CoarseFace(_)) {
                        continue;
                    }
                    let spd = self.solver.wind1_speed_mps[i];
                    if spd < 0.45 {
                        continue;
                    }
                    let v3 = self.base.geom.local_uv_to_xyz(
                        i,
                        self.solver.u1_east[i],
                        self.solver.v1_north[i],
                    );
                    arrows.push((i, v3, (spd / max_spd).clamp(0.06, 1.0)));
                }
            }
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
        let koeppen = self
            .base
            .prehistory
            .tile_koeppen_ids()
            .get(c_idx)
            .copied()
            .unwrap_or(0);

        let tc = self.solver.tsurf_c[fine_idx];
        let slp = self.solver.slp_hpa[fine_idx];
        let w1 = self.solver.wind1_speed_mps[fine_idx];
        let w2 = self.solver.wind2_speed_mps[fine_idx];
        let u1 = self.solver.u1_east[fine_idx];
        let v1 = self.solver.v1_north[fine_idx];
        // Wind direction azimuth in degrees `[0, 360)` toward which wind blows (`0 = N`, `90 = E`):
        let wind_az_deg = u1.atan2(v1).to_degrees().rem_euclid(360.0);

        let rain = self.solver.rain_mm_hr[fine_idx];
        let snow = self.solver.snow_mm_hr[fine_idx];
        let snow_depth = self.solver.snow_depth_m[fine_idx];
        let cloud_low = self.solver.cloud_low[fine_idx];
        let cloud_high = self.solver.cloud_high[fine_idx];
        let qc = self.solver.qc_kgm2[fine_idx];
        let storm_darkening =
            ((qc / 2.1) + 0.12 * (rain + snow)).clamp(0.0, 1.0);

        let eta_m = self.solver.tides.eta_m[fine_idx];
        let surge_m = self.solver.storm_surge_m[fine_idx];
        let range_m = self.solver.tides.tidal_range_m[fine_idx];

        let (year, day, utc_tau) = self.solver.tides.current_sim_calendar();
        let lon_rad = self.base.geom.lons_rad[fine_idx] as f64;
        let local_tau = (utc_tau + lon_rad / (2.0 * std::f64::consts::PI)).rem_euclid(1.0) as f32;
        let local_h = (local_tau as f64) * self.base.day_hours();
        let l_hh = local_h.floor() as u32;
        let l_mm = ((local_h.fract() * 60.0).floor() as u32).min(59);

        let loc_summary = if elev_m <= 0.0 {
            format!(
                "Ocean ({:.0} m) | SST: {:+.1} °C | SLP: {:.1} hPa | Tide η: {:+.2} m (Surge {:+.2} m)",
                -elev_m, tc, slp, eta_m, surge_m
            )
        } else if self.solver.tides.is_inundated_land(fine_idx) {
            format!(
                "STORM/TIDAL INUNDATION (+{:.1} m bed, {:.2} m water) | {:+.1} °C | {:.1} hPa",
                elev_m, self.solver.tides.h_total_m[fine_idx], tc, slp
            )
        } else {
            format!(
                "{} (+{:.0} m) | Temp: {:+.1} °C | SLP: {:.1} hPa | Soil: {:.0}% | Snow: {:.2} m",
                koeppen_climate_name(koeppen),
                elev_m,
                tc,
                slp,
                self.solver.soil_moisture[fine_idx] * 100.0,
                snow_depth
            )
        };

        let precip_desc = if snow > 0.05 && rain > 0.05 {
            format!("Sleet {:.2} mm/h", rain + snow)
        } else if snow > 0.02 {
            format!("Snow {:.2} mm/h", snow)
        } else if rain > 0.02 {
            format!("Rain {:.2} mm/h", rain)
        } else {
            "Dry".to_string()
        };
        let river_note = if elev_m > 0.0 && self.solver.river_flow_m3s[fine_idx] > 40.0 {
            format!(" · River {:.0} m³/s", self.solver.river_flow_m3s[fine_idx])
        } else {
            String::new()
        };
        let wx_summary = format!(
            "Wind {:.1} m/s (Jet {:.1} m/s) · Cloud {:.0}% · {}{}",
            w1,
            w2,
            (cloud_low.max(cloud_high) * 100.0).clamp(0.0, 100.0),
            precip_desc,
            river_note
        );

        let strat_json = self.base.stratigraphy_json_for_fine(fine_idx);
        let live_env = TileLiveEnvironment {
            sim_locked: true,
            sim_year: year,
            sim_day: day,
            sim_hour_frac: local_tau,
            sim_clock_label: format!("Day {day} · {l_hh:02}:{l_mm:02} Local"),
            tide_offset_m: eta_m,
            tidal_range_m: range_m,
            cloud_low,
            cloud_high,
            rain_mm_hr: rain,
            snow_mm_hr: snow,
            snow_depth_m: snow_depth,
            wind_speed_mps: w1,
            wind_az_deg,
            storm_darkening,
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
                coarse_koeppen_ids: self.base.prehistory.tile_koeppen_ids(),
                coarse_tile_life: self.base.prehistory.tile_life(),
                radius_km: self.base.prehistory.radius_km(),
                day_hours: self.base.day_hours(),
                tilt_deg: self.base.prehistory.tilt_deg(),
                has_atmosphere: true,
                has_life: true,
                current_year: year,
                moon_count: self.solver.tides.moon_count(),
                home_star: &self.base.home_star,
                optics: &self.base.optics,
                solar_system: &self.base.solar_system,
                constellations: &self.base.constellations,
                title: format!("Fine Tile #{fine_idx} (Size {})", self.base.fine.grid.size),
                location_summary: loc_summary,
                population_summary: wx_summary,
                technology: format!(
                    "{} · Glac {:.0}% · {} Moon(s)",
                    self.base.atmosphere.name(),
                    self.solver.glaciation() * 100.0,
                    self.solver.tides.moon_count()
                ),
                stratigraphy_json: strat_json,
                live_env: Some(live_env),
            },
        )?;

        view.sky.moons = self
            .solver
            .tides
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

    /// Formats the 48-hour Meteogram sparkline payload (`Precip` bars + `SLP` line + `Temp` line)
    /// consumed by `paintSparklineCanvas("weather-meteogram-canvas", ...)`.
    pub fn tile_meteogram_json(&self, fine_idx: usize) -> String {
        if fine_idx >= self.base.fine.grid.faces.len() {
            return "{\"series\":[]}".to_string();
        }
        let (temps, slps, precips, winds, _tides) = self.solver.cell_meteogram_series(fine_idx);
        let cur_t = self.solver.tsurf_c[fine_idx];
        let cur_slp = self.solver.slp_hpa[fine_idx];
        let cur_p = self.solver.rain_mm_hr[fine_idx] + self.solver.snow_mm_hr[fine_idx];
        let cur_w = self.solver.wind1_speed_mps[fine_idx];

        let min_t = temps.iter().copied().fold(cur_t, f32::min);
        let max_t = temps.iter().copied().fold(cur_t, f32::max);
        let t_pad = ((max_t - min_t).abs() * 0.22).max(2.0);
        let y_min_t = min_t - t_pad;
        let y_max_t = max_t + t_pad;

        let min_p_hpa = slps.iter().copied().fold(cur_slp, f32::min);
        let max_p_hpa = slps.iter().copied().fold(cur_slp, f32::max);
        let slp_pad = ((max_p_hpa - min_p_hpa).abs() * 0.25).max(3.5);
        let y_min_slp = min_p_hpa - slp_pad;
        let y_max_slp = max_p_hpa + slp_pad;

        let max_pr = precips.iter().copied().fold(cur_p, f32::max).max(1.5);
        let max_wind = winds.iter().copied().fold(cur_w, f32::max);

        let t_str: Vec<String> = temps.iter().map(|v| format!("{v:.1}")).collect();
        let slp_str: Vec<String> = slps.iter().map(|v| format!("{v:.1}")).collect();
        let pr_str: Vec<String> = precips.iter().map(|v| format!("{v:.2}")).collect();

        let current_label = format!("{cur_t:+.1} °C · {cur_slp:.0} hPa · {cur_p:.2} mm/h");
        let subtitle = format!(
            "SLP {min_p_hpa:.0}..{max_p_hpa:.0} hPa | Peak Wind {max_wind:.1} m/s ({}h recorded)",
            temps.len()
        );

        format!(
            "{{\"currentLabel\":{},\"subtitle\":{},\"minLabel\":{},\"maxLabel\":{},\"legend\":[{{\"text\":\"Temp °C\",\"color\":\"#fbbf24\"}},{{\"text\":\"SLP hPa\",\"color\":\"#c084fc\"}},{{\"text\":\"Precip mm/h\",\"color\":\"#38bdf8\"}}],\"series\":[{{\"label\":\"Precip (mm/h)\",\"kind\":\"bars\",\"color\":\"rgba(56,189,248,0.62)\",\"min\":0.0,\"max\":{max_pr:.2},\"values\":[{}]}},{{\"label\":\"SLP (hPa)\",\"kind\":\"line\",\"color\":\"#c084fc\",\"lineWidth\":1.45,\"min\":{y_min_slp:.2},\"max\":{y_max_slp:.2},\"values\":[{}]}},{{\"label\":\"Temp (°C)\",\"kind\":\"line\",\"color\":\"#fbbf24\",\"lineWidth\":1.9,\"min\":{y_min_t:.2},\"max\":{y_max_t:.2},\"values\":[{}]}}]}}",
            json_escape(&current_label),
            json_escape(&subtitle),
            json_escape(&format!("Low: {min_t:+.1} °C")),
            json_escape(&format!("High: {max_t:+.1} °C")),
            pr_str.join(","),
            slp_str.join(","),
            t_str.join(",")
        )
    }
}

#[inline]
fn pressure_and_fronts_rgb(dp_hpa: f32, front_k100km: f32, is_land: bool) -> [f32; 3] {
    // Normalize pressure anomaly across `~ -28 .. +28 hPa` with smooth tanh:
    let x = (dp_hpa / 16.0).tanh();
    let land_tint = if is_land { 0.06 } else { 0.0 };
    let mut rgb = if x < 0.0 {
        // Cyclone low: slate -> vibrant azure -> deep electric cyan-indigo core
        let t = -x;
        [
            0.14 * (1.0 - t) + 0.08 * t + land_tint,
            0.22 + 0.56 * t + land_tint * 0.5,
            0.34 + 0.64 * t,
        ]
    } else {
        // Anticyclone high: slate -> warm amber -> cream-gold ridge
        let t = x;
        [
            0.16 + 0.78 * t + land_tint,
            0.22 + 0.56 * t + land_tint * 0.6,
            0.30 * (1.0 - 0.55 * t),
        ]
    };

    // Overlay sharp baroclinic cold/warm frontal zones (`|\nabla \theta_1| > 1.2 K / 100 km`) in crimson-magenta:
    let front_w = ((front_k100km - 1.15) / 2.6).clamp(0.0, 0.85);
    if front_w > 0.01 {
        rgb[0] = (1.0 - front_w) * rgb[0] + front_w * 0.96;
        rgb[1] = (1.0 - front_w) * rgb[1] + front_w * 0.16;
        rgb[2] = (1.0 - front_w) * rgb[2] + front_w * 0.58;
    }
    rgb
}

#[inline]
fn temperature_thermal_rgb(tc: f32, is_land: bool) -> [f32; 3] {
    // Map `-40 °C .. +40 °C` onto `[0, 1]` with `0 °C` right at `0.50`:
    let t = ((tc + 40.0) / 80.0).clamp(0.0, 1.0);
    let mut rgb = if t < 0.25 {
        // -40 .. -20 C: deep polar violet -> royal blue
        let u = t / 0.25;
        [0.34 - 0.24 * u, 0.08 + 0.22 * u, 0.56 + 0.36 * u]
    } else if t < 0.50 {
        // -20 .. 0 C: royal blue -> crisp freezing cyan
        let u = (t - 0.25) / 0.25;
        [0.10 + 0.10 * u, 0.30 + 0.58 * u, 0.92 + 0.04 * u]
    } else if t < 0.72 {
        // 0 .. +17.6 C: cyan -> emerald -> spring yellow-green
        let u = (t - 0.50) / 0.22;
        [0.20 + 0.62 * u, 0.88 - 0.04 * u, 0.96 - 0.74 * u]
    } else if t < 0.88 {
        // +17.6 .. +30.4 C: yellow -> warm orange
        let u = (t - 0.72) / 0.16;
        [0.82 + 0.15 * u, 0.84 - 0.38 * u, 0.22 - 0.14 * u]
    } else {
        // +30.4 .. +42 C: orange -> scorching desert crimson
        let u = (t - 0.88) / 0.12;
        [0.97 - 0.12 * u, 0.46 - 0.36 * u, 0.08 + 0.08 * u]
    };

    if !is_land {
        rgb[0] *= 0.88;
        rgb[1] *= 0.90;
        rgb[2] *= 0.94;
    }
    rgb
}

#[inline]
fn precipitation_radar_rgb(
    base_surf: [f32; 3],
    is_land: bool,
    cloud_low: f32,
    cloud_high: f32,
    rain_mm_hr: f32,
    snow_mm_hr: f32,
    river_flow_m3s: f32,
) -> [f32; 3] {
    // Dim true-color geography into a crisp dark slate-navy radar basemap:
    let mut rgb = if is_land {
        [
            0.12 + 0.24 * base_surf[0],
            0.14 + 0.24 * base_surf[1],
            0.18 + 0.20 * base_surf[2],
        ]
    } else {
        [0.04, 0.09, 0.18]
    };

    if is_land && river_flow_m3s > 150.0 {
        let rf = ((river_flow_m3s - 150.0) / 2800.0).clamp(0.0, 0.85);
        rgb[0] = (1.0 - rf) * rgb[0] + rf * 0.10;
        rgb[1] = (1.0 - rf) * rgb[1] + rf * 0.62;
        rgb[2] = (1.0 - rf) * rgb[2] + rf * 0.86;
    }

    let cw = (0.65 * cloud_low + 0.45 * cloud_high).clamp(0.0, 0.72);
    for k in 0..3 {
        rgb[k] = (1.0 - cw) * rgb[k] + cw * [0.64, 0.69, 0.76][k];
    }

    if rain_mm_hr > 0.03 {
        let rt = (rain_mm_hr / 6.0).sqrt().clamp(0.0, 1.0);
        let radar = if rt < 0.45 {
            let u = rt / 0.45;
            [0.08 + 0.08 * u, 0.58 + 0.34 * u, 0.46 + 0.50 * u]
        } else if rt < 0.80 {
            let u = (rt - 0.45) / 0.35;
            [0.16 + 0.80 * u, 0.92 - 0.06 * u, 0.96 - 0.76 * u]
        } else {
            let u = (rt - 0.80) / 0.20;
            [0.96, 0.86 - 0.62 * u, 0.20 + 0.68 * u]
        };
        let alpha = (0.35 + 0.62 * rt).clamp(0.0, 0.96);
        for k in 0..3 {
            rgb[k] = (1.0 - alpha) * rgb[k] + alpha * radar[k];
        }
    }

    if snow_mm_hr > 0.03 {
        let st = (snow_mm_hr / 4.0).sqrt().clamp(0.0, 1.0);
        let snow_col = [0.68 + 0.30 * st, 0.74 + 0.24 * st, 0.98];
        let alpha = (0.40 + 0.56 * st).clamp(0.0, 0.96);
        for k in 0..3 {
            rgb[k] = (1.0 - alpha) * rgb[k] + alpha * snow_col[k];
        }
    }
    rgb
}

#[inline]
fn jet_stream_isotach_rgb(base_surf: [f32; 3], is_land: bool, jet_mps: f32) -> [f32; 3] {
    let mut rgb = if is_land {
        [
            0.13 + 0.22 * base_surf[0],
            0.15 + 0.22 * base_surf[1],
            0.18 + 0.18 * base_surf[2],
        ]
    } else {
        [0.04, 0.08, 0.16]
    };

    let t = ((jet_mps - 4.0).max(0.0) / 56.0).clamp(0.0, 1.0);
    if t > 0.01 {
        let iso = if t < 0.35 {
            let u = t / 0.35;
            [0.08 + 0.10 * u, 0.42 + 0.44 * u, 0.68 + 0.22 * u]
        } else if t < 0.70 {
            let u = (t - 0.35) / 0.35;
            [0.18 + 0.78 * u, 0.86 - 0.10 * u, 0.90 - 0.68 * u]
        } else {
            let u = (t - 0.70) / 0.30;
            [0.96 + 0.04 * u, 0.76 - 0.44 * u, 0.22 + 0.72 * u]
        };
        let alpha = (0.20 + 0.78 * t.sqrt()).clamp(0.0, 0.95);
        for k in 0..3 {
            rgb[k] = (1.0 - alpha) * rgb[k] + alpha * iso[k];
        }
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weather_sandbox_modes_meteogram_and_live_sky_env() {
        let mut sb = WeatherSandbox::new_earth();
        assert_eq!(sb.base.fine.grid.size, 8);
        assert_eq!(sb.base.fine.grid.faces.len(), 65_612);

        sb.step_hours(2);
        assert_eq!(sb.solver.tides.elapsed_hours(), 2);
        assert!(sb.global_mean_temp_c().is_finite());
        assert!(sb.max_wind_mps() > 0.05);

        for mode in [
            "satellite",
            "pressure_fronts",
            "temperature",
            "precipitation",
            "winds_jet",
            "storm_surge",
        ] {
            let colors = sb.tile_colors_rgb(mode, 25);
            assert_eq!(colors.len(), 65_612 * 3);
            assert!(colors.iter().all(|c| c.is_finite() && *c >= 0.0 && *c <= 1.0));
        }

        let view = sb
            .inspect_tile_structured(25)
            .expect("valid weather tile inspection view");
        let env = view.live_env.as_ref().expect("live_env populated");
        assert!(env.sim_locked);
        assert!(env.wind_az_deg >= 0.0 && env.wind_az_deg <= 360.0);

        let meteogram = sb.tile_meteogram_json(25);
        assert!(meteogram.contains("\"legend\""));
        assert!(meteogram.contains("\"Precip (mm/h)\""));
        assert!(meteogram.contains("\"SLP (hPa)\""));
        assert!(meteogram.contains("\"Temp (°C)\""));
    }
}
