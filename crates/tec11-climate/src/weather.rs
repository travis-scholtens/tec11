use std::collections::VecDeque;

use tec11_math_grid::{SphericalGrid, Vec3};

use crate::fv_geom::FiniteVolumeSphereGeom;
use crate::tides::{AtmosphericTideForcing, LaplaceTidalSolver};

const WEATHER_SUBSTEPS_PER_HOUR: usize = 10;
const DT_WEATHER_S: f32 = 3600.0 / (WEATHER_SUBSTEPS_PER_HOUR as f32);
const METEOGRAM_HOURS: usize = 48;

#[derive(Clone, Copy, Debug, Default)]
struct MeteogramSample {
    temp_tenths_c: i16,
    slp_tenths_hpa: i16,
    precip_hundredths_mm: u16,
    wind_tenths_mps: u16,
}

/// Initialization inputs derived from coarse `ClimateModel` and fine `FullSphereFineGrid` terrain.
pub struct WeatherInitConfig<'a> {
    pub bed_m: &'a [f32],
    pub mountainosity_norm: &'a [f32],
    pub is_river: &'a [bool],
    /// Baseline ocean SST (°C) interpolated onto the fine grid (`-1.8 .. +31 °C`).
    pub base_sst_c: &'a [f32],
    /// Initial soil moisture fraction (`0.0 .. 1.0`) from climate aridity.
    pub init_soil_moisture: &'a [f32],
    /// Initial snow depth (m) on perennial icecaps / high alpine peaks.
    pub init_snow_depth_m: &'a [f32],
    pub day_hours: f64,
    pub tilt_deg: f64,
    pub days_per_year: f64,
    pub home_init_angle_rad: f64,
    pub solar_tide_factor: f64,
    pub solar_luminosity: f32,
    pub glaciation: f32,
    /// `0` = N2-O2, `1` = CH4-rich, `2` = Dense CO2, `3` = Tholin haze, `4` = Thin CO2, `5` = H2-rich.
    pub atmosphere_id: u32,
    pub moon_count: u32,
    pub primary_mass_lunar: f32,
    pub primary_dist_radii: f32,
}

/// Coupled 2-layer moist baroclinic primitive-equation weather solver + Manabe surface hydrology
/// + dynamic Laplace tides & storm surge on a uniform `coarse + 2` spherical finite-volume grid.
#[derive(Clone, Debug)]
pub struct MoistBaroclinicSolver {
    num_cells: usize,
    unit_centers: Vec<Vec3>,
    surf_elev_m: Vec<f32>,
    surf_gx: Vec<f32>,
    surf_gy: Vec<f32>,
    mountainosity_norm: Vec<f32>,
    is_river: Vec<bool>,
    base_sst_c: Vec<f32>,
    init_soil_moisture: Vec<f32>,
    init_snow_depth_m: Vec<f32>,
    /// Downstream fine-cell receiver (`u32::MAX` if sink/ocean) + topological order high -> low.
    downstream_cell: Vec<u32>,
    topo_order: Vec<u32>,

    solar_luminosity: f32,
    glaciation: f32,
    atmosphere_id: u32,
    p_ref_pa: f32,
    greenhouse_offset_k: f32,

    /// Coupled barotropic astronomical tide + meteorological storm-surge solver.
    pub tides: LaplaceTidalSolver,
    /// Pure astronomical reference tide height (without pressure/wind forcing) for isolating storm surge!
    astro_ref_tides: LaplaceTidalSolver,

    // Prognostic atmospheric & surface fields (`num_cells`):
    /// Sea-level pressure anomaly $\delta p_s$ (Pa).
    pub ps_anom_pa: Vec<f32>,
    /// Sea-level pressure in hPa (`~1013.25 + 0.01 * ps_anom_pa`).
    pub slp_hpa: Vec<f32>,
    /// Lower-tropospheric (`~800 hPa`) edge-normal velocity (m/s).
    pub u1_edge: Vec<f32>,
    /// Upper-tropospheric (`~300 hPa` jet) edge-normal velocity (m/s).
    pub u2_edge: Vec<f32>,
    pub u1_east: Vec<f32>,
    pub v1_north: Vec<f32>,
    pub u2_east: Vec<f32>,
    pub v2_north: Vec<f32>,
    pub wind1_speed_mps: Vec<f32>,
    pub wind2_speed_mps: Vec<f32>,

    /// Lower-tropospheric potential temperature $\theta_1$ (K).
    pub theta1_k: Vec<f32>,
    /// Upper-tropospheric potential temperature $\theta_2$ (K).
    pub theta2_k: Vec<f32>,
    /// Baroclinic frontal sharpness $|\nabla \theta_1|$ (K per 100 km) highlighting cold & warm fronts.
    pub front_strength_k100km: Vec<f32>,
    /// 2m / surface skin temperature (°C).
    pub tsurf_c: Vec<f32>,

    /// Vertically integrated water vapor $q_v$ ($\text{kg m}^{-2}$).
    pub qv_kgm2: Vec<f32>,
    /// Vertically integrated cloud liquid/ice condensate $q_c$ ($\text{kg m}^{-2}$).
    pub qc_kgm2: Vec<f32>,
    /// Diagnostic low/mid cloud cover fraction in `[0.0, 1.0]`.
    pub cloud_low: Vec<f32>,
    /// Diagnostic high cirrus/anvil cloud cover fraction in `[0.0, 1.0]`.
    pub cloud_high: Vec<f32>,
    /// Hourly liquid rainfall rate ($\text{mm hr}^{-1}$).
    pub rain_mm_hr: Vec<f32>,
    /// Hourly snowfall water-equivalent rate ($\text{mm hr}^{-1}$).
    pub snow_mm_hr: Vec<f32>,

    /// Manabe bucket soil moisture fraction in `[0.0, 1.0]`.
    pub soil_moisture: Vec<f32>,
    /// Geometric snowpack depth on ground (m).
    pub snow_depth_m: Vec<f32>,
    /// Routed instantaneous surface runoff / river discharge ($\text{m}^3\text{ s}^{-1}$).
    pub river_flow_m3s: Vec<f32>,
    /// Meteorological storm surge anomaly $\eta_{\text{total}} - \eta_{\text{astro}}$ (m).
    pub storm_surge_m: Vec<f32>,

    // Surface wind stress buffers passed to `LaplaceTidalSolver`:
    tau_east_pa: Vec<f32>,
    tau_north_pa: Vec<f32>,

    // Rolling 48-hour meteogram ring buffer:
    meteogram_history: VecDeque<Vec<MeteogramSample>>,

    // Reusable scratch arrays for allocation-free substeps:
    scratch_phi1: Vec<f32>,
    scratch_phi2: Vec<f32>,
    scratch_div1: Vec<f32>,
    scratch_div2: Vec<f32>,
    scratch_edge_flux: Vec<f32>,
    scratch_lap_a: Vec<f32>,
    scratch_lap_b: Vec<f32>,
    scratch_gx: Vec<f32>,
    scratch_gy: Vec<f32>,
    scratch_u_next: Vec<f32>,
}

fn atmosphere_thermo_params(atmosphere_id: u32) -> (f32, f32) {
    match atmosphere_id {
        1 => (121_590.0, 5.5),  // Methane-rich (1.2 bar, moderate CH4 greenhouse)
        2 => (455_960.0, 22.0), // Dense CO2 (4.5 bar, strong CO2 greenhouse)
        3 => (151_990.0, 3.0),  // Tholin haze (1.5 bar, anti-greenhouse upper haze + IR)
        4 => (8_100.0, -24.0),  // Thin dusty CO2 (0.08 bar, cold Mars-like thin blanket)
        5 => (253_300.0, 14.0), // H2-rich Hycean (2.5 bar, collision-induced H2 warming)
        _ => (101_325.0, 0.0),  // Earth-like N2-O2 + O3 (1.0 bar)
    }
}

impl MoistBaroclinicSolver {
    pub fn new(
        grid: &SphericalGrid,
        adjacency: &[Vec<usize>],
        geom: &FiniteVolumeSphereGeom,
        cfg: WeatherInitConfig<'_>,
    ) -> Self {
        let n = geom.num_cells;
        let ne = geom.edges.len();

        let unit_centers: Vec<Vec3> = grid
            .faces
            .iter()
            .map(|&(p, _)| {
                let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-12);
                [p[0] / len, p[1] / len, p[2] / len]
            })
            .collect();

        let surf_elev_m: Vec<f32> = cfg.bed_m.iter().map(|&zb| zb.max(0.0)).collect();
        let mut surf_gx = vec![0.0_f32; n];
        let mut surf_gy = vec![0.0_f32; n];
        geom.gradient_scalar(&surf_elev_m, &mut surf_gx, &mut surf_gy);

        // Precompute steepest-descent downstream cell & topological order for instant river routing:
        let mut downstream_cell = vec![u32::MAX; n];
        for i in 0..n {
            if cfg.bed_m[i] <= 0.0 {
                continue;
            }
            let mut best_nb = u32::MAX;
            let mut best_elev = cfg.bed_m[i];
            // Prefer established fine river channels when nearly flat so runoff follows carved valleys:
            for &nb in &adjacency[i] {
                let eff_elev = if cfg.is_river[nb] {
                    cfg.bed_m[nb] - 4.0
                } else {
                    cfg.bed_m[nb]
                };
                if eff_elev < best_elev {
                    best_elev = eff_elev;
                    best_nb = nb as u32;
                }
            }
            downstream_cell[i] = best_nb;
        }
        let mut land_indices: Vec<u32> = (0..n as u32)
            .filter(|&i| cfg.bed_m[i as usize] > 0.0)
            .collect();
        land_indices.sort_by(|&a, &b| {
            cfg.bed_m[b as usize]
                .partial_cmp(&cfg.bed_m[a as usize])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let tides = LaplaceTidalSolver::new(
            grid,
            geom,
            cfg.bed_m.to_vec(),
            cfg.day_hours,
            cfg.tilt_deg,
            cfg.days_per_year,
            cfg.home_init_angle_rad,
            cfg.solar_tide_factor,
            cfg.moon_count,
            cfg.primary_mass_lunar,
            cfg.primary_dist_radii,
        );
        let astro_ref_tides = tides.clone();
        let (p_ref_pa, greenhouse_offset_k) = atmosphere_thermo_params(cfg.atmosphere_id);

        let mut solver = Self {
            num_cells: n,
            unit_centers,
            surf_elev_m,
            surf_gx,
            surf_gy,
            mountainosity_norm: cfg.mountainosity_norm.to_vec(),
            is_river: cfg.is_river.to_vec(),
            base_sst_c: cfg.base_sst_c.to_vec(),
            init_soil_moisture: cfg.init_soil_moisture.to_vec(),
            init_snow_depth_m: cfg.init_snow_depth_m.to_vec(),
            downstream_cell,
            topo_order: land_indices,
            solar_luminosity: cfg.solar_luminosity.clamp(0.2, 4.0),
            glaciation: cfg.glaciation.clamp(0.0, 1.0),
            atmosphere_id: cfg.atmosphere_id,
            p_ref_pa,
            greenhouse_offset_k,
            tides,
            astro_ref_tides,
            ps_anom_pa: vec![0.0; n],
            slp_hpa: vec![1013.25; n],
            u1_edge: vec![0.0; ne],
            u2_edge: vec![0.0; ne],
            u1_east: vec![0.0; n],
            v1_north: vec![0.0; n],
            u2_east: vec![0.0; n],
            v2_north: vec![0.0; n],
            wind1_speed_mps: vec![0.0; n],
            wind2_speed_mps: vec![0.0; n],
            theta1_k: vec![288.0; n],
            theta2_k: vec![318.0; n],
            front_strength_k100km: vec![0.0; n],
            tsurf_c: vec![15.0; n],
            qv_kgm2: vec![10.0; n],
            qc_kgm2: vec![0.0; n],
            cloud_low: vec![0.0; n],
            cloud_high: vec![0.0; n],
            rain_mm_hr: vec![0.0; n],
            snow_mm_hr: vec![0.0; n],
            soil_moisture: cfg.init_soil_moisture.to_vec(),
            snow_depth_m: cfg.init_snow_depth_m.to_vec(),
            river_flow_m3s: vec![0.0; n],
            storm_surge_m: vec![0.0; n],
            tau_east_pa: vec![0.0; n],
            tau_north_pa: vec![0.0; n],
            meteogram_history: VecDeque::with_capacity(METEOGRAM_HOURS + 1),
            scratch_phi1: vec![0.0; n],
            scratch_phi2: vec![0.0; n],
            scratch_div1: vec![0.0; n],
            scratch_div2: vec![0.0; n],
            scratch_edge_flux: vec![0.0; ne],
            scratch_lap_a: vec![0.0; n],
            scratch_lap_b: vec![0.0; n],
            scratch_gx: vec![0.0; n],
            scratch_gy: vec![0.0; n],
            scratch_u_next: vec![0.0; ne],
        };
        solver.reset_state(geom);
        solver
    }

    /// Resets the atmosphere, land hydrology, and ocean tides cleanly to `t = 0` radiative equilibrium
    /// at rest (`u1 = u2 = 0`, `precip = 0`, `surge = 0`), with no hidden initial spin-up.
    pub fn reset_state(&mut self, geom: &FiniteVolumeSphereGeom) {
        self.tides.reset_state(geom);
        self.astro_ref_tides.reset_state(geom);

        let sun_dir = self.tides.subsolar_unit_xyz(0.0);
        let sin_dec = sun_dir[2] as f32;
        let glac_offset_c = 12.0 * (0.5 - self.glaciation);
        let p_hpa_base = self.p_ref_pa * 0.01;

        self.ps_anom_pa.fill(0.0);
        self.slp_hpa.fill(p_hpa_base);
        self.u1_edge.fill(0.0);
        self.u2_edge.fill(0.0);
        self.u1_east.fill(0.0);
        self.v1_north.fill(0.0);
        self.u2_east.fill(0.0);
        self.v2_north.fill(0.0);
        self.wind1_speed_mps.fill(0.0);
        self.wind2_speed_mps.fill(0.0);
        self.qc_kgm2.fill(0.0);
        self.cloud_low.fill(0.0);
        self.cloud_high.fill(0.0);
        self.rain_mm_hr.fill(0.0);
        self.snow_mm_hr.fill(0.0);
        self.river_flow_m3s.fill(0.0);
        self.storm_surge_m.fill(0.0);
        self.tau_east_pa.fill(0.0);
        self.tau_north_pa.fill(0.0);

        self.soil_moisture.copy_from_slice(&self.init_soil_moisture);
        self.snow_depth_m.copy_from_slice(&self.init_snow_depth_m);

        for i in 0..self.num_cells {
            let sin_phi = geom.sin_lat[i];
            let cos_phi = geom.cos_lat[i];
            let lon = geom.lons_rad[i];
            let zs = self.surf_elev_m[i];
            let is_ocean = self.tides.bed_m()[i] < 0.0;

            // Meridional radiative-convective baseline + seasonal tilt shift:
            let eq_pole_c = 28.5 - 48.0 * sin_phi * sin_phi + 11.0 * sin_phi * sin_dec
                + glac_offset_c
                + self.greenhouse_offset_k;
            let lapse_c = -0.0065 * zs;

            let t0_c = if is_ocean {
                (self.base_sst_c[i] + 0.55 * glac_offset_c + 0.45 * self.greenhouse_offset_k)
                    .clamp(-1.8, 36.0)
            } else {
                let snow_cool = -5.0 * (self.snow_depth_m[i] / 0.10).clamp(0.0, 1.0);
                (eq_pole_c + lapse_c + snow_cool).clamp(-55.0, 48.0)
            };
            self.tsurf_c[i] = t0_c;

            // Tiny synoptic wavenumber-4/5/6 baroclinic seed (`+-0.65 K`) in midlatitudes
            // (Jablonowski & Williamson 2006) so baroclinic Rossby waves break symmetry cleanly.
            let midlat_env = (2.0 * sin_phi * cos_phi).powi(2);
            let wave_seed = 0.65
                * midlat_env
                * ((4.0 * lon + 0.4).sin()
                    + 0.65 * (5.0 * lon - 1.1).cos()
                    + 0.40 * (6.0 * lon + 2.1).sin());

            let t1_k = (t0_c + 273.15 - 9.0 + wave_seed).clamp(205.0, 325.0);
            // Potential temperature at sigma1 = 0.78 (`Pi1 ~ 0.931`) and sigma2 = 0.30:
            self.theta1_k[i] = t1_k + 0.0032 * zs;
            self.theta2_k[i] = self.theta1_k[i] + 29.0 + 6.0 * sin_phi * sin_phi - 0.35 * wave_seed;

            let qsat = saturation_column_vapor_kgm2(t1_k, zs);
            let rh_init = if is_ocean {
                0.76
            } else {
                0.32 + 0.44 * self.soil_moisture[i]
            };
            self.qv_kgm2[i] = (rh_init * qsat).clamp(0.05, 85.0);
        }

        self.update_front_strength(geom);
        self.meteogram_history.clear();
        self.push_meteogram_frame();
    }

    pub fn set_environment_params(
        &mut self,
        atmosphere_id: u32,
        glaciation: f32,
        moon_count: u32,
        primary_mass: f32,
        primary_dist_radii: f32,
    ) {
        self.atmosphere_id = atmosphere_id.min(5);
        let (p_ref_pa, gh_k) = atmosphere_thermo_params(self.atmosphere_id);
        self.p_ref_pa = p_ref_pa;
        self.greenhouse_offset_k = gh_k;
        self.glaciation = glaciation.clamp(0.0, 1.0);
        self.tides
            .set_moon_params(moon_count, primary_mass, primary_dist_radii);
        self.astro_ref_tides
            .set_moon_params(moon_count, primary_mass, primary_dist_radii);
        for i in 0..self.num_cells {
            self.slp_hpa[i] = (self.p_ref_pa + self.ps_anom_pa[i]) * 0.01;
        }
    }

    pub fn set_day_of_year(&mut self, day: u32) {
        self.tides.set_day_of_year(day);
        self.astro_ref_tides.set_day_of_year(day);
    }

    pub fn atmosphere_id(&self) -> u32 {
        self.atmosphere_id
    }

    pub fn glaciation(&self) -> f32 {
        self.glaciation
    }

    /// Advances the coupled 2-layer moist baroclinic atmosphere, land hydrology, and dynamic
    /// tides + storm surge by `1 hour`.
    pub fn step_hour(&mut self, geom: &FiniteVolumeSphereGeom) {
        let dt = DT_WEATHER_S;
        let inv_sub = 1.0 / (WEATHER_SUBSTEPS_PER_HOUR as f32);
        let glac_offset_c = 12.0 * (0.5 - self.glaciation);

        // Scale-selective 4th-order hyperviscosity coefficient `nu4` tuned to grid spacing `d_mean`:
        let d_mean = geom.mean_edge_dist_m.max(10_000.0);
        let nu2_scalar = 0.0035 * d_mean * d_mean / dt;
        let nu4_scalar = 0.0022 * d_mean.powi(4) / dt;

        // Baroclinic hydrostatic & external-mode constants:
        let rho_air = (self.p_ref_pa / (287.05 * 285.0)).clamp(0.08, 6.0);
        let inv_rho_air = 1.0 / rho_air;
        let c_baroclinic_sq = 52.0_f32 * 52.0_f32; // L_Rossby = 52 / 1e-4 = 520 km!
        let alpha_therm1 = 128.0_f32;
        let delta_exner = 224.0_f32;

        self.rain_mm_hr.fill(0.0);
        self.snow_mm_hr.fill(0.0);
        let mut hourly_runoff_m = vec![0.0_f32; self.num_cells];

        for sub in 0..WEATHER_SUBSTEPS_PER_HOUR {
            let sub_offset_s = ((sub as f64) + 0.5) * (dt as f64);
            let sun_dir = self.tides.subsolar_unit_xyz(sub_offset_s);
            let sin_dec = sun_dir[2] as f32;

            // Compute global mean reference theta1, theta2 so pressure gradients act on anomalies:
            let mut sum_th1 = 0.0_f64;
            let mut sum_th2 = 0.0_f64;
            let mut sum_area = 0.0_f64;
            for i in 0..self.num_cells {
                let a = geom.areas_m2[i] as f64;
                sum_th1 += (self.theta1_k[i] as f64) * a;
                sum_th2 += (self.theta2_k[i] as f64) * a;
                sum_area += a;
            }
            let mean_th1 = (sum_th1 / sum_area.max(1.0)) as f32;
            let mean_th2 = (sum_th2 / sum_area.max(1.0)) as f32;

            // 1. Diurnal shortwave + grey-atmosphere OLR + sensible/latent surface fluxes:
            for i in 0..self.num_cells {
                let p = self.unit_centers[i];
                let cos_z = ((p[0] * sun_dir[0] + p[1] * sun_dir[1] + p[2] * sun_dir[2]) as f32)
                    .max(0.0);
                let is_ocean = self.tides.bed_m()[i] < 0.0 || self.tides.is_inundated_land(i);
                let zs = self.surf_elev_m[i];
                let sin_phi = geom.sin_lat[i];

                let cloud_tot = (self.cloud_low[i] + 0.55 * self.cloud_high[i]).clamp(0.0, 1.0);
                let surf_albedo = if is_ocean {
                    if self.tsurf_c[i] < -1.7 {
                        0.60
                    } else {
                        0.06 + 0.14 * (1.0 - cos_z).powi(3)
                    }
                } else {
                    let veg_alb = 0.28 - 0.14 * self.soil_moisture[i];
                    let snow_frac = (self.snow_depth_m[i] / 0.08).clamp(0.0, 1.0);
                    (1.0 - snow_frac) * veg_alb + snow_frac * 0.76
                };

                // Instantaneous absorbed shortwave vs daily-mean radiative target:
                // Normalized diurnal anomaly `(cos_z - daily_mean_cos_z)` drives realistic `+6..+13 C`
                // afternoon land warming and nighttime cooling while keeping multi-day climate anchored!
                let cos_lat = geom.cos_lat[i];
                let approx_daily_mu = (0.318 * cos_lat * (1.0 - sin_dec * sin_dec).sqrt()
                    + 0.5 * (sin_phi * sin_dec).max(0.0))
                .clamp(0.02, 0.45);
                let sw_trans = 1.0 - 0.56 * cloud_tot;
                let diurnal_sw_wm2 =
                    1361.0 * self.solar_luminosity * (1.0 - surf_albedo) * sw_trans * (cos_z - approx_daily_mu);
                let cloud_lw_trap_wm2 = 42.0 * cloud_tot - 28.0 * (surf_albedo - 0.20);

                if is_ocean {
                    let target_sst = (self.base_sst_c[i]
                        + 0.55 * glac_offset_c
                        + 0.45 * self.greenhouse_offset_k)
                        .clamp(-1.8, 36.0);
                    // Small ocean diurnal skin ripple (`~0.3 C`) + slow relaxation to seasonal SST:
                    let d_ts = (diurnal_sw_wm2 / 4.2e6) * dt
                        + ((target_sst - self.tsurf_c[i]) / (72.0 * 3600.0)) * dt;
                    self.tsurf_c[i] = (self.tsurf_c[i] + d_ts).clamp(-2.0, 38.0);
                } else {
                    let eq_clim_c = 28.0 - 48.0 * sin_phi * sin_phi + 11.5 * sin_phi * sin_dec
                        + glac_offset_c
                        + self.greenhouse_offset_k
                        - 0.0065 * zs;
                    // Low thermal inertia over dry land (`~2.2e5 J/m2/K`), higher over wet soil:
                    let c_land = 1.9e5 + 1.6e5 * self.soil_moisture[i];
                    let net_flux = diurnal_sw_wm2 + cloud_lw_trap_wm2;
                    let relax_rate = 1.0 / (16.0 * 3600.0);
                    let d_ts = (net_flux / c_land + (eq_clim_c - self.tsurf_c[i]) * relax_rate) * dt;
                    self.tsurf_c[i] = (self.tsurf_c[i] + d_ts).clamp(-65.0, 54.0);
                }

                // Sensible heat exchange + Held-Suarez radiative relaxation on theta1 & theta2:
                let wspd = self.wind1_speed_mps[i];
                let ts_pot_k = self.tsurf_c[i] + 273.15 + 0.0065 * zs;
                let ch_sens = (0.0012 + 0.00025 * wspd) * (1.0 + 0.6 * self.mountainosity_norm[i]);
                let sensible_rate = (ch_sens * (wspd + 2.5) / 1100.0) * (ts_pot_k - 3.5 - self.theta1_k[i]);

                let rad_eq_th1 = 297.0 - 46.0 * sin_phi * sin_phi
                    + 10.0 * sin_phi * sin_dec
                    + glac_offset_c
                    + self.greenhouse_offset_k;
                let rad_eq_th2 = rad_eq_th1 + 29.0 + 8.0 * sin_phi * sin_phi;
                let tau_rad1 = 1.0 / (8.0 * 86400.0);
                let tau_rad2 = 1.0 / (14.0 * 86400.0);

                self.theta1_k[i] = (self.theta1_k[i]
                    + dt * (sensible_rate + (rad_eq_th1 - self.theta1_k[i]) * tau_rad1))
                    .clamp(195.0, 345.0);
                self.theta2_k[i] = (self.theta2_k[i]
                    + dt * ((rad_eq_th2 - self.theta2_k[i]) * tau_rad2
                        + 0.08 * sensible_rate.max(0.0)))
                    .clamp(215.0, 380.0);

                // WISHE surface evaporation:
                let qsat_surf = saturation_column_vapor_kgm2(self.tsurf_c[i] + 273.15, zs);
                let wet_frac = if is_ocean {
                    1.0
                } else if self.is_river[i] {
                    self.soil_moisture[i].clamp(0.45, 1.0)
                } else {
                    self.soil_moisture[i].clamp(0.0, 1.0)
                };
                let evap_coeff = 1.35e-6 * (2.2 + 0.85 * wspd) * wet_frac;
                let evap_kgm2 = (evap_coeff * (qsat_surf - self.qv_kgm2[i]).max(0.0) * dt).min(1.5);
                self.qv_kgm2[i] += evap_kgm2;
                if !is_ocean && evap_kgm2 > 0.0 {
                    self.soil_moisture[i] = (self.soil_moisture[i] - evap_kgm2 / 150.0).max(0.0);
                }

                // Hydrostatic baroclinic geopotentials Phi1' (800 hPa) and Phi2' (300 hPa):
                let dth1 = self.theta1_k[i] - mean_th1;
                let dth2 = self.theta2_k[i] - mean_th2;
                let p_term = self.ps_anom_pa[i] * inv_rho_air;
                self.scratch_phi1[i] = p_term - alpha_therm1 * dth1;
                self.scratch_phi2[i] =
                    p_term + (delta_exner - alpha_therm1) * dth1 + 0.55 * delta_exner * dth2;
            }

            // 2. Reconstruct cell winds & advance 2-layer C-grid edge momentum (Coriolis + PGF + Ekman/mountain drag):
            geom.reconstruct_cell_vectors(&self.u1_edge, &mut self.u1_east, &mut self.v1_north);
            geom.reconstruct_cell_vectors(&self.u2_edge, &mut self.u2_east, &mut self.v2_north);

            // Update Layer 1 (lower troposphere, 800 hPa):
            for (e_idx, e) in geom.edges.iter().enumerate() {
                let l = e.left as usize;
                let r = e.right as usize;
                let dphi1_dn = (self.scratch_phi1[r] - self.scratch_phi1[l]) * e.inv_center_dist_m;
                let cor_l =
                    geom.coriolis_f[l] * (e.nx_left * self.v1_north[l] - e.ny_left * self.u1_east[l]);
                let cor_r =
                    geom.coriolis_f[r] * (e.nx_right * self.v1_north[r] - e.ny_right * self.u1_east[r]);
                let cor = 0.5 * (cor_l + cor_r);

                // Vertical shear exchange between upper jet and lower layer (baroclinic momentum coupling):
                let shear = (self.u2_edge[e_idx] - self.u1_edge[e_idx]) * 1.4e-5;

                // Surface Ekman friction (stronger over rough continental mountain ranges than smooth ocean):
                let is_land_edge = self.tides.bed_m()[l] > 0.0 || self.tides.bed_m()[r] > 0.0;
                let mount = 0.5 * (self.mountainosity_norm[l] + self.mountainosity_norm[r]);
                let r_ekman = if is_land_edge {
                    2.2e-5 * (1.0 + 1.8 * mount)
                } else {
                    0.85e-5
                };
                let u_old = self.u1_edge[e_idx];
                let quad_drag = 1.6e-6 * u_old.abs() * (1.0 + mount);
                let u_new = ((u_old + dt * (cor - dphi1_dn + shear))
                    / (1.0 + dt * (r_ekman + quad_drag)))
                    .clamp(-55.0, 55.0);
                self.scratch_u_next[e_idx] = u_new;
            }
            self.u1_edge.copy_from_slice(&self.scratch_u_next);

            // Update Layer 2 (upper troposphere / 300 hPa jet stream):
            for (e_idx, e) in geom.edges.iter().enumerate() {
                let l = e.left as usize;
                let r = e.right as usize;
                let dphi2_dn = (self.scratch_phi2[r] - self.scratch_phi2[l]) * e.inv_center_dist_m;
                let cor_l =
                    geom.coriolis_f[l] * (e.nx_left * self.v2_north[l] - e.ny_left * self.u2_east[l]);
                let cor_r =
                    geom.coriolis_f[r] * (e.nx_right * self.v2_north[r] - e.ny_right * self.u2_east[r]);
                let cor = 0.5 * (cor_l + cor_r);
                let shear = (self.u1_edge[e_idx] - self.u2_edge[e_idx]) * 1.1e-5;
                let r_upper = 2.6e-6 + 6.0e-7 * self.u2_edge[e_idx].abs();
                let u_new = ((self.u2_edge[e_idx] + dt * (cor - dphi2_dn + shear))
                    / (1.0 + dt * r_upper))
                    .clamp(-95.0, 95.0);
                self.scratch_u_next[e_idx] = u_new;
            }
            self.u2_edge.copy_from_slice(&self.scratch_u_next);

            // 3. Layer mass divergences & surface pressure anomaly evolution:
            for (e_idx, _e) in geom.edges.iter().enumerate() {
                self.scratch_edge_flux[e_idx] = self.u1_edge[e_idx];
            }
            geom.divergence_from_edge_flux(&self.scratch_edge_flux, &mut self.scratch_div1);

            for (e_idx, _e) in geom.edges.iter().enumerate() {
                self.scratch_edge_flux[e_idx] = self.u2_edge[e_idx];
            }
            geom.divergence_from_edge_flux(&self.scratch_edge_flux, &mut self.scratch_div2);

            for i in 0..self.num_cells {
                let col_div = 0.48 * self.scratch_div1[i] + 0.52 * self.scratch_div2[i];
                let d_ps = -rho_air * c_baroclinic_sq * col_div * dt;
                // Gentle mass-restoring sponge prevents multi-week drift:
                self.ps_anom_pa[i] =
                    ((self.ps_anom_pa[i] + d_ps) * (1.0 - 1.5e-6 * dt)).clamp(-9500.0, 6500.0);
            }
            // Smooth external-pressure grid-scale ripples with mild Laplacian damping:
            geom.laplacian_scalar(&self.ps_anom_pa, &mut self.scratch_lap_a);
            for i in 0..self.num_cells {
                self.ps_anom_pa[i] += dt * (2.5 * nu2_scalar) * self.scratch_lap_a[i];
            }

            // 4. Conservative upwind finite-volume advection of theta1, theta2, qv, qc:
            advect_scalar_upwind(
                geom,
                &self.u1_edge,
                &self.scratch_div1,
                dt,
                &mut self.theta1_k,
                &mut self.scratch_edge_flux,
                &mut self.scratch_lap_a,
            );
            advect_scalar_upwind(
                geom,
                &self.u2_edge,
                &self.scratch_div2,
                dt,
                &mut self.theta2_k,
                &mut self.scratch_edge_flux,
                &mut self.scratch_lap_a,
            );
            advect_tracer_flux_limited(
                geom,
                &self.u1_edge,
                dt,
                &mut self.qv_kgm2,
                &mut self.scratch_edge_flux,
                &mut self.scratch_lap_a,
            );
            advect_tracer_flux_limited(
                geom,
                &self.u1_edge,
                dt,
                &mut self.qc_kgm2,
                &mut self.scratch_edge_flux,
                &mut self.scratch_lap_a,
            );

            // Scale-selective biharmonic (-nu4 * nabla^4) hyperdiffusion on theta1 & theta2
            // keeps frontal gradients sharp while eliminating grid-scale checkerboarding:
            apply_biharmonic_smoothing(
                geom,
                dt * nu4_scalar,
                &mut self.theta1_k,
                &mut self.scratch_lap_a,
                &mut self.scratch_lap_b,
            );
            apply_biharmonic_smoothing(
                geom,
                dt * nu4_scalar,
                &mut self.theta2_k,
                &mut self.scratch_lap_a,
                &mut self.scratch_lap_b,
            );

            // 5. Moist microphysics, orographic upslope lift, deep convection & latent heat release:
            geom.reconstruct_cell_vectors(&self.u1_edge, &mut self.u1_east, &mut self.v1_north);
            for i in 0..self.num_cells {
                let ux = self.u1_east[i];
                let vy = self.v1_north[i];
                self.wind1_speed_mps[i] = (ux * ux + vy * vy).sqrt();

                let zs = self.surf_elev_m[i];
                // Orographic upslope velocity `u1 . grad(z_s)` + boundary-layer convergence lift:
                let w_orog = (ux * self.surf_gx[i] + vy * self.surf_gy[i]).max(0.0);
                let w_conv = (-1400.0 * self.scratch_div1[i]).max(0.0);
                let w_lift = w_orog + w_conv;

                // Adiabatic cooling from upslope/convergent ascent lowers effective saturation capacity:
                let t1_k = (self.theta1_k[i] - 0.0032 * zs).clamp(195.0, 330.0);
                let lift_cool_k = (18.0 * w_lift).clamp(0.0, 12.0);
                let qsat_eff = saturation_column_vapor_kgm2(t1_k - lift_cool_k, zs);

                // Deep moist convective instability when warm humid lower air exceeds upper stability:
                let rh = (self.qv_kgm2[i] / qsat_eff.max(0.2)).clamp(0.0, 2.0);
                let theta_e_excess = (self.theta1_k[i] + 1.8 * self.qv_kgm2[i]) - self.theta2_k[i];
                let mut conv_cond = 0.0_f32;
                if theta_e_excess > 2.0 && rh > 0.68 {
                    let rate = ((theta_e_excess - 2.0) * 0.035).min(0.30) * (dt / 3600.0);
                    conv_cond = (self.qv_kgm2[i] - 0.62 * qsat_eff).max(0.0) * rate;
                }

                // Large-scale / orographic supersaturation condensation:
                let ls_cond = if self.qv_kgm2[i] > qsat_eff {
                    0.65 * (self.qv_kgm2[i] - qsat_eff)
                } else {
                    // Subsaturated dry-air cloud evaporation (creates crisp leeward rain shadows!):
                    let sub_def = (qsat_eff - self.qv_kgm2[i]) * 0.18 * (dt / 1800.0);
                    -sub_def.min(self.qc_kgm2[i])
                };

                let net_cond = (conv_cond + ls_cond)
                    .clamp(-self.qc_kgm2[i], self.qv_kgm2[i] * 0.85);
                self.qv_kgm2[i] = (self.qv_kgm2[i] - net_cond).max(0.0);
                self.qc_kgm2[i] = (self.qc_kgm2[i] + net_cond).max(0.0);

                // Latent heat release warms the atmospheric column (drives warm-core cyclones & fronts!):
                let latent_dtheta = 0.42 * net_cond;
                self.theta1_k[i] = (self.theta1_k[i] + 0.55 * latent_dtheta).clamp(195.0, 345.0);
                self.theta2_k[i] = (self.theta2_k[i] + 0.45 * latent_dtheta).clamp(215.0, 385.0);
                // Convective latent heating also deepens warm-core low-pressure centers:
                if net_cond > 0.02 {
                    self.ps_anom_pa[i] =
                        (self.ps_anom_pa[i] - 32.0 * net_cond).clamp(-9500.0, 6500.0);
                }

                //essler autoconversion of cloud condensate `qc -> surface precipitation`:
                let qc = self.qc_kgm2[i];
                let precip_step_mm = if qc > 0.28 {
                    let auto = (qc - 0.28) * (0.42 + 0.35 * (w_lift * 10.0).min(1.0)) * (dt / 1200.0);
                    auto.min(qc * 0.78)
                } else {
                    0.0
                };
                self.qc_kgm2[i] = (qc - precip_step_mm).max(0.0);

                let snow_frac = ((1.2 - self.tsurf_c[i]) / 2.8).clamp(0.0, 1.0);
                let snow_mm = precip_step_mm * snow_frac;
                let rain_mm = precip_step_mm - snow_mm;
                self.rain_mm_hr[i] += rain_mm;
                self.snow_mm_hr[i] += snow_mm;

                // Land surface snowpack & Manabe bucket update:
                if self.tides.bed_m()[i] > 0.0 && !self.tides.is_inundated_land(i) {
                    // 1 mm SWE = 0.008 m snowpack depth (`8:1` settled snow ratio):
                    self.snow_depth_m[i] += snow_mm * 0.008;
                    let melt_depth_m = if self.tsurf_c[i] > 0.2 && self.snow_depth_m[i] > 0.0 {
                        (0.00045 * self.tsurf_c[i] * (dt / 3600.0)).min(self.snow_depth_m[i])
                    } else {
                        0.0
                    };
                    self.snow_depth_m[i] -= melt_depth_m;
                    let liquid_in_mm = rain_mm + melt_depth_m * 125.0;
                    if liquid_in_mm > 0.0 {
                        // High soil saturation + steep terrain generate direct runoff:
                        let sat = self.soil_moisture[i].clamp(0.0, 1.0);
                        let runoff_frac =
                            (sat * sat * (0.55 + 0.45 * self.mountainosity_norm[i])).clamp(0.05, 1.0);
                        let infil_mm = liquid_in_mm * (1.0 - runoff_frac);
                        let new_sm = self.soil_moisture[i] + infil_mm / 150.0;
                        let bucket_overflow_mm = ((new_sm - 1.0).max(0.0)) * 150.0;
                        self.soil_moisture[i] = new_sm.clamp(0.0, 1.0);
                        let total_runoff_mm = liquid_in_mm * runoff_frac + bucket_overflow_mm;
                        hourly_runoff_m[i] += total_runoff_mm * 0.001;
                    }
                } else {
                    self.snow_depth_m[i] = 0.0;
                }

                // Update cloud fractions for rendering:
                self.cloud_low[i] =
                    (self.cloud_low[i] + ((self.qc_kgm2[i] / 1.35).sqrt().clamp(0.0, 1.0)) * inv_sub)
                        .min(1.0);
            }
        }

        // Reconstruct final end-of-hour winds, sea-level pressure, high clouds, river routing, & storm surge:
        geom.reconstruct_cell_vectors(&self.u1_edge, &mut self.u1_east, &mut self.v1_north);
        geom.reconstruct_cell_vectors(&self.u2_edge, &mut self.u2_east, &mut self.v2_north);

        // Zero global mean of `ps_anom_pa` so total dry atmospheric mass is strictly invariant:
        let mut ps_sum = 0.0_f64;
        let mut area_sum = 0.0_f64;
        for i in 0..self.num_cells {
            let a = geom.areas_m2[i] as f64;
            ps_sum += (self.ps_anom_pa[i] as f64) * a;
            area_sum += a;
        }
        let ps_mean = (ps_sum / area_sum.max(1.0)) as f32;

        for i in 0..self.num_cells {
            self.ps_anom_pa[i] -= ps_mean;
            self.slp_hpa[i] = (self.p_ref_pa + self.ps_anom_pa[i]) * 0.01;

            let u1 = self.u1_east[i];
            let v1 = self.v1_north[i];
            let spd1 = (u1 * u1 + v1 * v1).sqrt();
            self.wind1_speed_mps[i] = spd1;

            let u2 = self.u2_east[i];
            let v2 = self.v2_north[i];
            self.wind2_speed_mps[i] = (u2 * u2 + v2 * v2).sqrt();

            let precip_tot = self.rain_mm_hr[i] + self.snow_mm_hr[i];
            self.cloud_low[i] = ((self.qc_kgm2[i] / 1.25).sqrt() + 0.10 * precip_tot.sqrt())
                .clamp(0.0, 1.0);
            self.cloud_high[i] = (((self.qc_kgm2[i] - 0.30).max(0.0) / 1.40)
                + 0.18 * (precip_tot / 2.5).min(1.0))
            .clamp(0.0, 1.0);

            // Charnock wind stress `tau = rho_air * Cd * |u1| * u1` for coastal storm surge:
            let cd_wind = (0.0011 + 0.000045 * spd1).min(0.0032);
            let stress_mag = rho_air * cd_wind * spd1;
            self.tau_east_pa[i] = stress_mag * u1;
            self.tau_north_pa[i] = stress_mag * v1;
        }

        // Route hourly surface runoff downstream in topological order to compute `river_flow_m3s`:
        let mut accum_m3s = vec![0.0_f32; self.num_cells];
        for i in 0..self.num_cells {
            if hourly_runoff_m[i] > 0.0 {
                accum_m3s[i] = hourly_runoff_m[i] * geom.areas_m2[i] / 3600.0;
            }
        }
        for &u_idx in &self.topo_order {
            let u = u_idx as usize;
            let dst = self.downstream_cell[u];
            if dst != u32::MAX {
                let q = accum_m3s[u];
                accum_m3s[dst as usize] += q;
            }
        }
        for i in 0..self.num_cells {
            // Smooth 6-hour river channel reservoir response so storm hydrographs crest & recede naturally:
            self.river_flow_m3s[i] = 0.78 * self.river_flow_m3s[i] + 0.22 * accum_m3s[i];
        }

        // Step coupled dynamic tides + storm surge AND unforced astronomical reference tide:
        self.tides.step_hour(
            geom,
            Some(AtmosphericTideForcing {
                p_anom_pa: &self.ps_anom_pa,
                tau_east_pa: &self.tau_east_pa,
                tau_north_pa: &self.tau_north_pa,
            }),
        );
        self.astro_ref_tides.step_hour(geom, None);
        for i in 0..self.num_cells {
            self.storm_surge_m[i] = self.tides.eta_m[i] - self.astro_ref_tides.eta_m[i];
        }

        self.update_front_strength(geom);
        self.push_meteogram_frame();
    }

    fn update_front_strength(&mut self, geom: &FiniteVolumeSphereGeom) {
        geom.gradient_scalar(&self.theta1_k, &mut self.scratch_gx, &mut self.scratch_gy);
        for i in 0..self.num_cells {
            let gmag =
                (self.scratch_gx[i] * self.scratch_gx[i] + self.scratch_gy[i] * self.scratch_gy[i])
                    .sqrt();
            // Convert K/m to K per 100 km (`* 1e5`):
            self.front_strength_k100km[i] = gmag * 1.0e5;
        }
    }

    fn push_meteogram_frame(&mut self) {
        if self.meteogram_history.len() >= METEOGRAM_HOURS {
            self.meteogram_history.pop_front();
        }
        let mut frame = Vec::with_capacity(self.num_cells);
        for i in 0..self.num_cells {
            let t10 = (self.tsurf_c[i] * 10.0).round().clamp(-3000.0, 3000.0) as i16;
            let slp10 = ((self.slp_hpa[i] - 1000.0) * 10.0)
                .round()
                .clamp(-30_000.0, 30_000.0) as i16;
            let p100 = ((self.rain_mm_hr[i] + self.snow_mm_hr[i]) * 100.0)
                .round()
                .clamp(0.0, 65_000.0) as u16;
            let w10 = (self.wind1_speed_mps[i] * 10.0)
                .round()
                .clamp(0.0, 65_000.0) as u16;
            frame.push(MeteogramSample {
                temp_tenths_c: t10,
                slp_tenths_hpa: slp10,
                precip_hundredths_mm: p100,
                wind_tenths_mps: w10,
            });
        }
        self.meteogram_history.push_back(frame);
    }

    /// Returns `(temp_c, slp_hpa, precip_mm_hr, wind_mps, tide_m)` up-to-48-hour series for `cell`.
    pub fn cell_meteogram_series(
        &self,
        cell: usize,
    ) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
        if cell >= self.num_cells {
            return (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        }
        let n = self.meteogram_history.len();
        let mut temps = Vec::with_capacity(n);
        let mut slps = Vec::with_capacity(n);
        let mut precips = Vec::with_capacity(n);
        let mut winds = Vec::with_capacity(n);
        for f in &self.meteogram_history {
            let s = f[cell];
            temps.push((s.temp_tenths_c as f32) * 0.1);
            slps.push(1000.0 + (s.slp_tenths_hpa as f32) * 0.1);
            precips.push((s.precip_hundredths_mm as f32) * 0.01);
            winds.push((s.wind_tenths_mps as f32) * 0.1);
        }
        let tides = self.tides.cell_eta_series_m(cell);
        (temps, slps, precips, winds, tides)
    }
}

/// Advective form $\partial_t \theta + \mathbf{u}\cdot\nabla\theta = 0$ written as
/// $\nabla\cdot(\theta\mathbf{u}) - \theta\nabla\cdot\mathbf{u}$ so a constant scalar field
/// is preserved identically even when $\nabla\cdot\mathbf{u} \ne 0$.
fn advect_scalar_upwind(
    geom: &FiniteVolumeSphereGeom,
    u_edge: &[f32],
    div_u: &[f32],
    dt: f32,
    field: &mut [f32],
    scratch_flux: &mut [f32],
    scratch_div_fu: &mut [f32],
) {
    for (e_idx, e) in geom.edges.iter().enumerate() {
        let u = u_edge[e_idx];
        let f_up = if u >= 0.0 {
            field[e.left as usize]
        } else {
            field[e.right as usize]
        };
        scratch_flux[e_idx] = u * f_up;
    }
    geom.divergence_from_edge_flux(scratch_flux, scratch_div_fu);
    for i in 0..geom.num_cells {
        let adv = scratch_div_fu[i] - field[i] * div_u[i];
        field[i] -= dt * adv;
    }
}

/// Conservative positivity-preserving upwind flux transport for non-negative moisture tracers (`qv`, `qc`).
fn advect_tracer_flux_limited(
    geom: &FiniteVolumeSphereGeom,
    u_edge: &[f32],
    dt: f32,
    tracer: &mut [f32],
    scratch_flux: &mut [f32],
    scratch_outflow: &mut [f32],
) {
    scratch_outflow.fill(0.0);
    for (e_idx, e) in geom.edges.iter().enumerate() {
        let u = u_edge[e_idx];
        if u > 0.0 {
            let l = e.left as usize;
            let q = u * tracer[l];
            scratch_flux[e_idx] = q;
            scratch_outflow[l] += dt * e.dual_len_m * q;
        } else if u < 0.0 {
            let r = e.right as usize;
            let q = u * tracer[r];
            scratch_flux[e_idx] = q;
            scratch_outflow[r] += dt * e.dual_len_m * (-q);
        } else {
            scratch_flux[e_idx] = 0.0;
        }
    }
    // Scale outgoing fluxes where needed so `tracer >= 0` strictly without creating/destroying mass:
    for (e_idx, e) in geom.edges.iter().enumerate() {
        let q = scratch_flux[e_idx];
        if q > 0.0 {
            let l = e.left as usize;
            let avail = 0.90 * tracer[l] * geom.areas_m2[l];
            if scratch_outflow[l] > avail && scratch_outflow[l] > 1e-9 {
                scratch_flux[e_idx] = q * (avail / scratch_outflow[l]);
            }
        } else if q < 0.0 {
            let r = e.right as usize;
            let avail = 0.90 * tracer[r] * geom.areas_m2[r];
            if scratch_outflow[r] > avail && scratch_outflow[r] > 1e-9 {
                scratch_flux[e_idx] = q * (avail / scratch_outflow[r]);
            }
        }
    }
    for i in 0..geom.num_cells {
        let mut div_q = 0.0_f32;
        for inc in geom.incident_edges(i) {
            div_q += scratch_flux[inc.edge_idx as usize] * inc.flux_factor;
        }
        tracer[i] = (tracer[i] - dt * div_q).max(0.0);
    }
}

fn apply_biharmonic_smoothing(
    geom: &FiniteVolumeSphereGeom,
    dt_nu4: f32,
    field: &mut [f32],
    lap1: &mut [f32],
    lap2: &mut [f32],
) {
    geom.laplacian_scalar(field, lap1);
    geom.laplacian_scalar(lap1, lap2);
    for i in 0..geom.num_cells {
        field[i] -= dt_nu4 * lap2[i];
    }
}

/// Bolton (1980) Clausius–Clapeyron saturation column water vapor ($\text{kg m}^{-2}$)
/// as a function of lower-tropospheric temperature `t_k` (K) and surface elevation `zs_m` (m).
#[inline]
pub fn saturation_column_vapor_kgm2(t_k: f32, zs_m: f32) -> f32 {
    let tc = (t_k - 273.15).clamp(-65.0, 52.0);
    let es_ratio = (17.67 * tc / (tc + 243.5)).exp();
    let elev_atten = (-zs_m.max(0.0) / 2400.0).exp();
    (9.2 * elev_atten * es_ratio).clamp(0.05, 110.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moist_baroclinic_solver_48h_stability_diurnal_coriolis_and_surge() {
        let grid = SphericalGrid::with_size(4);
        let adj = grid.adjacency();
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, 24.0);
        let n = geom.num_cells;

        let mut bed_m = vec![-3200.0_f32; n];
        let mut mount = vec![0.0_f32; n];
        let mut is_river = vec![false; n];
        let mut base_sst = vec![24.0_f32; n];
        let mut soil = vec![0.55_f32; n];
        let snow = vec![0.0_f32; n];

        let mut land_sample = 0usize;
        let mut ocean_sample = 0usize;
        for i in 0..n {
            let p = grid.faces[i].0;
            base_sst[i] = (27.0 - 28.0 * (p[2] * p[2]) as f32).max(-1.5);
            if p[0] > 0.35 && p[2].abs() < 0.65 {
                bed_m[i] = 450.0 + 1800.0 * ((p[0] - 0.35) as f32);
                mount[i] = 0.45;
                if p[2].abs() < 0.15 {
                    land_sample = i;
                    is_river[i] = true;
                }
            } else if p[0] > 0.22 && p[2].abs() < 0.65 {
                bed_m[i] = -35.0; // shallow coastal shelf for storm surge test
            } else if p[0] < -0.5 && p[2].abs() < 0.15 {
                ocean_sample = i;
            }
            if bed_m[i] <= 0.0 {
                soil[i] = 1.0;
            }
        }

        let mut solver = MoistBaroclinicSolver::new(
            &grid,
            &adj,
            &geom,
            WeatherInitConfig {
                bed_m: &bed_m,
                mountainosity_norm: &mount,
                is_river: &is_river,
                base_sst_c: &base_sst,
                init_soil_moisture: &soil,
                init_snow_depth_m: &snow,
                day_hours: 24.0,
                tilt_deg: 23.44,
                days_per_year: 365.25,
                home_init_angle_rad: 0.0,
                solar_tide_factor: 1.0,
                solar_luminosity: 1.0,
                glaciation: 0.5,
                atmosphere_id: 0,
                moon_count: 1,
                primary_mass_lunar: 1.0,
                primary_dist_radii: 60.27,
            },
        );

        // Integrate 48 hours from t = 0 and verify 100% finite + non-negative tracers + diurnal land > ocean amplitude:
        for _ in 0..48 {
            solver.step_hour(&geom);
        }

        assert!(solver.tsurf_c.iter().all(|v| v.is_finite()));
        assert!(solver.slp_hpa.iter().all(|v| v.is_finite() && *v > 850.0 && *v < 1150.0));
        assert!(solver.qv_kgm2.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(solver.qc_kgm2.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(solver.snow_depth_m.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(solver.soil_moisture.iter().all(|v| v.is_finite() && *v >= 0.0));

        let (land_temps, _, _, _, _) = solver.cell_meteogram_series(land_sample);
        let (sea_temps, _, _, _, _) = solver.cell_meteogram_series(ocean_sample);
        assert_eq!(land_temps.len(), 48);
        let land_span = land_temps.iter().copied().fold(f32::MIN, f32::max)
            - land_temps.iter().copied().fold(f32::MAX, f32::min);
        let sea_span = sea_temps.iter().copied().fold(f32::MIN, f32::max)
            - sea_temps.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            land_span > sea_span * 2.5 && land_span > 2.0,
            "expected much stronger diurnal temperature wave over land ({land_span:.2} C) than ocean ({sea_span:.2} C)"
        );

        // Verify a deep coastal low-pressure anomaly (`-4500 Pa` = `-45 hPa`) produces positive inverse-barometer storm surge:
        for i in 0..n {
            if bed_m[i] < 0.0 && bed_m[i] > -80.0 {
                solver.ps_anom_pa[i] = -4500.0;
            }
        }
        for _ in 0..3 {
            solver.step_hour(&geom);
        }
        let max_surge = solver
            .storm_surge_m
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        assert!(
            max_surge > 0.12,
            "expected positive coastal storm surge under deep cyclone pressure drop, got {max_surge} m"
        );
    }
}
