use std::collections::VecDeque;

use tec11_math_grid::SphericalGrid;

use crate::fv_geom::FiniteVolumeSphereGeom;
use crate::tides::{AtmosphericTideForcing, LaplaceTidalSolver};

const METEOGRAM_HOURS: usize = 48;

#[derive(Clone, Copy, Debug, Default)]
struct MeteogramSample {
    temp_tenths_c: i16,
    slp_tenths_hpa: i16,
    precip_hundredths_mm: u16,
    wind_tenths_mps: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct StaticEdgeDrag {
    r_ekman: f32,
    quad_drag1: f32,
    r_upper_base: f32,
    ext_q_weight: f32,
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
    unit_centers: Vec<[f32; 3]>,
    surf_elev_m: Vec<f32>,
    qsat_elev_base: Vec<f32>,
    atmo_alb_term: Vec<f32>,
    extratrop_theta_weight: Vec<f32>,
    edge_drag: Vec<StaticEdgeDrag>,
    surf_gx: Vec<f32>,
    surf_gy: Vec<f32>,
    mountainosity_norm: Vec<f32>,
    is_river: Vec<bool>,
    base_sst_c: Vec<f32>,
    /// Exact orbital-mean daily solar zenith cosine $\overline{\mu}_{\text{ann}}(\phi, \varepsilon)$ per cell.
    annual_mu: Vec<f32>,
    init_soil_moisture: Vec<f32>,
    init_snow_depth_m: Vec<f32>,
    /// Downstream fine-cell receiver (`u32::MAX` if sink/ocean) + topological order high -> low.
    downstream_cell: Vec<u32>,
    topo_order: Vec<u32>,
    min_edge_dist_m: f32,
    n_phys_steps_per_hour: usize,
    n_dyn_per_phys: usize,

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
    scratch_daily_mu: Vec<f32>,
    scratch_pot_sst_c: Vec<f32>,
    scratch_sea_ice_frac: Vec<f32>,
    scratch_runoff_m: Vec<f32>,
    scratch_accum_m3s: Vec<f32>,
    scratch_phi1: Vec<f32>,
    scratch_phi2: Vec<f32>,
    scratch_div1: Vec<f32>,
    scratch_div2: Vec<f32>,
    scratch_zeta1: Vec<f32>,
    scratch_zeta2: Vec<f32>,
    scratch_w12: Vec<f32>,
    scratch_ep_east: Vec<f32>,
    scratch_ep_edge: Vec<f32>,
    scratch_edge_flux: Vec<f32>,
    scratch_edge_flux2: Vec<f32>,
    scratch_moist_edge: Vec<f32>,
    scratch_lap_a: Vec<f32>,
    scratch_lap_b: Vec<f32>,
    scratch_lap_c: Vec<f32>,
    scratch_lap_d: Vec<f32>,
    scratch_gx: Vec<f32>,
    scratch_gy: Vec<f32>,
    scratch_u2_col: Vec<f32>,
    rk_u1_0: Vec<f32>,
    rk_u2_0: Vec<f32>,
    rk_ps_0: Vec<f32>,
    rk_th1_0: Vec<f32>,
    rk_th2_0: Vec<f32>,
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

        let unit_centers: Vec<[f32; 3]> = grid
            .faces
            .iter()
            .map(|&(p, _)| {
                let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-12);
                [
                    (p[0] / len) as f32,
                    (p[1] / len) as f32,
                    (p[2] / len) as f32,
                ]
            })
            .collect();

        let surf_elev_m: Vec<f32> = cfg.bed_m.iter().map(|&zb| zb.max(0.0)).collect();
        let qsat_elev_base: Vec<f32> = surf_elev_m
            .iter()
            .map(|&zs| 9.2 * (-zs / 2400.0).exp())
            .collect();
        let extratrop_theta_weight: Vec<f32> = geom
            .sin_lat
            .iter()
            .map(|&s| ((s.abs() - 0.28) / 0.35).clamp(0.0, 1.0))
            .collect();

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

        let mut annual_mu = Vec::with_capacity(n);
        let mut atmo_alb_term = Vec::with_capacity(n);
        for i in 0..n {
            let amu = orbital_annual_mean_cos_z(geom.sin_lat[i], geom.cos_lat[i], cfg.tilt_deg);
            annual_mu.push(amu);
            let atmo_atten = (-surf_elev_m[i] / 4800.0).exp();
            let slant_scatter = (1.0 - (amu * 2.85).clamp(0.0, 1.0)).clamp(0.0, 1.0);
            atmo_alb_term.push(0.42 * (0.20 + 0.12 * slant_scatter) * atmo_atten);
        }

        let mut min_edge_dist_m = f32::MAX;
        let mut edge_drag = Vec::with_capacity(ne);
        for e in &geom.edges {
            min_edge_dist_m = min_edge_dist_m.min(e.center_dist_m);
            let l = e.left as usize;
            let r = e.right as usize;
            let mount = 0.5 * (cfg.mountainosity_norm[l] + cfg.mountainosity_norm[r]);
            let land_share = 0.5
                * ((if cfg.bed_m[l] > 0.0 { 1.0 } else { 0.0 })
                    + (if cfg.bed_m[r] > 0.0 { 1.0 } else { 0.0 }));
            let r_land = 1.28e-5 * (1.0 + 1.85 * mount);
            let r_ekman = 0.95e-5 + land_share * (r_land - 0.95e-5);
            let quad_drag1 = 1.1e-6 * (1.0 + land_share * mount);
            let r_upper_base = 4.5e-7 + 9.0e-7 * mount;
            let sin_m = 0.5 * (geom.sin_lat[l].abs() + geom.sin_lat[r].abs());
            let ext_q_weight = ((sin_m - 0.36) / 0.30).clamp(0.0, 1.0);
            edge_drag.push(StaticEdgeDrag {
                r_ekman,
                quad_drag1,
                r_upper_base,
                ext_q_weight,
            });
        }
        if !min_edge_dist_m.is_finite() || min_edge_dist_m < 1_000.0 {
            min_edge_dist_m = geom.mean_edge_dist_m.max(10_000.0);
        }

        // Scale-aware Wicker–Skamarock fast/slow operator splitting:
        // - Outer radiative/moist-physics/transport steps (`n_phys_steps_per_hour`: `2` on size <= 6
        //   Earth, `3` on size 7..8 Earth, scaling automatically for smaller/finer planets).
        // - Inner 3-stage RK3 fast acoustic/gravity-wave substeps (`n_dyn_per_phys`, satisfying
        //   `dt_dyn <= 0.65 * d_min / c_fast` across all grid sizes and planetary radii).
        let dt_phys_max = (0.0185 * min_edge_dist_m).clamp(180.0, 1800.0);
        let n_phys_steps_per_hour = ((3600.0 / dt_phys_max).ceil() as usize).clamp(2, 20);
        let dt_phys_s = 3600.0 / (n_phys_steps_per_hour as f32);
        let dt_dyn_max = (0.65 * min_edge_dist_m / 68.0).max(60.0);
        let n_dyn_per_phys = ((dt_phys_s / dt_dyn_max).ceil() as usize).clamp(2, 8);

        let mut solver = Self {
            num_cells: n,
            unit_centers,
            surf_elev_m,
            qsat_elev_base,
            atmo_alb_term,
            extratrop_theta_weight,
            edge_drag,
            surf_gx,
            surf_gy,
            mountainosity_norm: cfg.mountainosity_norm.to_vec(),
            is_river: cfg.is_river.to_vec(),
            base_sst_c: cfg.base_sst_c.to_vec(),
            annual_mu,
            init_soil_moisture: cfg.init_soil_moisture.to_vec(),
            init_snow_depth_m: cfg.init_snow_depth_m.to_vec(),
            downstream_cell,
            topo_order: land_indices,
            min_edge_dist_m,
            n_phys_steps_per_hour,
            n_dyn_per_phys,
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
            scratch_daily_mu: vec![0.0; n],
            scratch_pot_sst_c: vec![0.0; n],
            scratch_sea_ice_frac: vec![0.0; n],
            scratch_runoff_m: vec![0.0; n],
            scratch_accum_m3s: vec![0.0; n],
            scratch_phi1: vec![0.0; n],
            scratch_phi2: vec![0.0; n],
            scratch_div1: vec![0.0; n],
            scratch_div2: vec![0.0; n],
            scratch_zeta1: vec![0.0; n],
            scratch_zeta2: vec![0.0; n],
            scratch_w12: vec![0.0; n],
            scratch_ep_east: vec![0.0; n],
            scratch_ep_edge: vec![0.0; ne],
            scratch_edge_flux: vec![0.0; ne],
            scratch_edge_flux2: vec![0.0; ne],
            scratch_moist_edge: vec![0.0; ne],
            scratch_lap_a: vec![0.0; n],
            scratch_lap_b: vec![0.0; n],
            scratch_lap_c: vec![0.0; n],
            scratch_lap_d: vec![0.0; n],
            scratch_gx: vec![0.0; n],
            scratch_gy: vec![0.0; n],
            scratch_u2_col: vec![0.0; ne],
            rk_u1_0: vec![0.0; ne],
            rk_u2_0: vec![0.0; ne],
            rk_ps_0: vec![0.0; n],
            rk_th1_0: vec![0.0; n],
            rk_th2_0: vec![0.0; n],
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
        // `0.0` = icehouse (`-4.9 °C`), `~0.35` = modern Earth (`0.0 °C`), `1.0` = greenhouse (`+9.1 °C`):
        let glac_offset_c = glaciation_temp_offset_c(self.glaciation);
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

            let daily_mu = exact_daily_mean_cos_z(sin_phi, cos_phi, sin_dec);
            let annual_mu = self.annual_mu[i];
            let pot_sst_c = self.base_sst_c[i]
                + 18.0 * self.solar_luminosity * (daily_mu - annual_mu)
                + glac_offset_c
                + 0.65 * self.greenhouse_offset_k;
            let sea_ice_frac = if is_ocean {
                ((-1.8 - pot_sst_c) / 4.2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let sm0 = self.soil_moisture[i].clamp(0.0, 1.0);
            let cover0 = (0.72 * self.init_soil_moisture[i] + 0.28 * sm0).clamp(0.0, 1.0);
            let canopy0 = ((cover0 - 0.12) / 0.30).clamp(0.0, 1.0);
            let veg_alb = 0.30 - 0.14 * canopy0 - 0.04 * sm0;
            let snow_frac = (self.snow_depth_m[i] / 0.10).clamp(0.0, 1.0);
            let ice_sheet_frac = ((self.snow_depth_m[i] - 0.55) / 0.65).clamp(0.0, 1.0);
            let snow_alb = 0.46 + 0.30 * ice_sheet_frac;
            let surf_albedo = if is_ocean {
                (1.0 - sea_ice_frac) * 0.08 + sea_ice_frac * 0.62
            } else {
                (1.0 - snow_frac) * veg_alb + snow_frac * snow_alb
            };

            let t0_c = if is_ocean {
                let rad_ice_c = radiative_equilibrium_surface_c(
                    daily_mu,
                    annual_mu,
                    0.62,
                    self.solar_luminosity,
                    glac_offset_c,
                    self.greenhouse_offset_k,
                    0.0,
                );
                let ice_skin_c = (0.62 * rad_ice_c + 0.38 * (-1.8)).min(0.0);
                ((1.0 - sea_ice_frac) * pot_sst_c.clamp(-1.8, 42.0) + sea_ice_frac * ice_skin_c)
                    .clamp(-45.0, 42.0)
            } else {
                radiative_equilibrium_surface_c(
                    daily_mu,
                    annual_mu,
                    surf_albedo,
                    self.solar_luminosity,
                    glac_offset_c,
                    self.greenhouse_offset_k,
                    zs,
                )
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

            let t1_k = (t0_c + 273.15 - 7.5 + wave_seed).clamp(205.0, 325.0);
            // Potential temperature at sigma1 = 0.78 (`Pi1 ~ 0.931`) and sigma2 = 0.28:
            let stab_trop = 25.0 + 7.0 * (1.0 - (annual_mu * 2.8).clamp(0.0, 1.0));
            self.theta1_k[i] = t1_k + 0.0065 * zs;
            self.theta2_k[i] = self.theta1_k[i] + stab_trop - 0.35 * wave_seed;

            let qsat = saturation_column_vapor_kgm2(t1_k, zs);
            let rh_init = if is_ocean {
                0.70
            } else {
                0.30 + 0.44 * self.soil_moisture[i]
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
        let n_phys = self.n_phys_steps_per_hour;
        let n_dyn = self.n_dyn_per_phys;
        let dt = 3600.0 / (n_phys as f32);
        let dt_dyn = dt / (n_dyn as f32);
        let dt_ratio_360 = dt * (1.0 / 360.0);
        let glac_offset_c = glaciation_temp_offset_c(self.glaciation);

        // Scale-selective 4th-order hyperviscosity & C-grid divergence damping tuned to `d_mean`:
        let d_mean = geom.mean_edge_dist_m.max(10_000.0);
        let d_min = self.min_edge_dist_m.max(5_000.0);
        let nu_div = 0.016 * d_mean * d_mean / dt_dyn;
        // Preserve hourly biharmonic damping while respecting pentagon forward-Euler stability (`< 0.012 * d^4`):
        let dt_nu4 = (0.020 / (n_phys as f32)).min(0.0068) * d_mean.powi(4);

        // Hydrostatic sigma-coordinate & external-mode constants:
        // Integrating upward from surface `sigma = 1.0` (`Pi_s = 1.000`) to `sigma_1 = 0.78` (`Pi_1 = 0.931`)
        // and `sigma_2 = 0.28` (`Pi_2 = 0.695`): `alpha_1 = c_p * (Pi_s - Pi_1) ~ 69 J/(kg K)`,
        // `delta_exner = c_p * (Pi_1 - Pi_2) ~ 228 J/(kg K)`.
        let rho_air = (self.p_ref_pa / (287.05 * 285.0)).clamp(0.08, 6.0);
        let inv_rho_air = 1.0 / rho_air;
        let c_ext_sq = 62.0_f32 * 62.0_f32;
        let alpha_therm1 = 69.0_f32;
        let delta_exner = 228.0_f32;

        self.rain_mm_hr.fill(0.0);
        self.snow_mm_hr.fill(0.0);
        self.scratch_runoff_m.fill(0.0);

        // Solar declination shifts < 0.004 deg over 1 hour; precompute daily-mean insolation,
        // seasonal SST potential, and open-ocean sea-ice fraction once per hour:
        let mid_sun = self.tides.subsolar_unit_xyz(1800.0);
        let sin_dec_hr = mid_sun[2] as f32;
        let cos_dec_hr = (1.0 - sin_dec_hr * sin_dec_hr).max(0.0).sqrt();
        let sst_const_offset = glac_offset_c + 0.65 * self.greenhouse_offset_k;
        let bed_slice = self.tides.bed_m();
        for i in 0..self.num_cells {
            let d_mu = exact_daily_mean_cos_z_with_cos_dec(
                geom.sin_lat[i],
                geom.cos_lat[i],
                sin_dec_hr,
                cos_dec_hr,
            );
            self.scratch_daily_mu[i] = d_mu;
            let pot_sst = self.base_sst_c[i]
                + 18.0 * self.solar_luminosity * (d_mu - self.annual_mu[i])
                + sst_const_offset;
            self.scratch_pot_sst_c[i] = pot_sst;
            self.scratch_sea_ice_frac[i] = if bed_slice[i] < 0.0 || self.tides.is_inundated_land(i)
            {
                ((-1.8 - pot_sst) / 4.2).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }

        let evap_step_cap = 0.25 * dt_ratio_360;
        let conv_frac_cap = (0.20 * dt_ratio_360).min(0.48);

        for sub in 0..n_phys {
            let sub_offset_s = ((sub as f64) + 0.5) * (dt as f64);
            let sun_dir_f64 = self.tides.subsolar_unit_xyz(sub_offset_s);
            let sun_dir = [
                sun_dir_f64[0] as f32,
                sun_dir_f64[1] as f32,
                sun_dir_f64[2] as f32,
            ];
            let sin_dec = sun_dir[2];

            // Compute global & tropical/polar mean theta1, theta2 for anomalies and baroclinic index:
            let mut sum_th1 = 0.0_f64;
            let mut sum_th2 = 0.0_f64;
            let mut sum_area = 0.0_f64;
            let mut trop_th_sum = 0.0_f64;
            let mut trop_a_sum = 0.0_f64;
            let mut pol_th_sum = 0.0_f64;
            let mut pol_a_sum = 0.0_f64;
            for i in 0..self.num_cells {
                let a = geom.areas_m2[i] as f64;
                let th1 = self.theta1_k[i] as f64;
                sum_th1 += th1 * a;
                sum_th2 += (self.theta2_k[i] as f64) * a;
                sum_area += a;
                let abs_sin = geom.sin_lat[i].abs();
                if abs_sin < 0.35 {
                    trop_th_sum += th1 * a;
                    trop_a_sum += a;
                } else if abs_sin > 0.75 {
                    pol_th_sum += th1 * a;
                    pol_a_sum += a;
                }
            }
            let mean_th1 = (sum_th1 / sum_area.max(1.0)) as f32;
            let mean_th2 = (sum_th2 / sum_area.max(1.0)) as f32;
            let dtheta_eq_pole = if trop_a_sum > 0.0 && pol_a_sum > 0.0 {
                ((trop_th_sum / trop_a_sum) - (pol_th_sum / pol_a_sum)).max(0.0) as f32
            } else {
                0.0
            };

            // 1. Diurnal shortwave + Budyko-Sellers OLR + air-surface sensible/latent fluxes:
            for i in 0..self.num_cells {
                let p = self.unit_centers[i];
                let cos_z = (p[0] * sun_dir[0] + p[1] * sun_dir[1] + p[2] * sun_dir[2]).max(0.0);
                let cos_z_sqrt = cos_z.sqrt();
                let is_ocean = self.tides.bed_m()[i] < 0.0 || self.tides.is_inundated_land(i);
                let zs = self.surf_elev_m[i];
                let qsat_base = self.qsat_elev_base[i];

                let daily_mu = self.scratch_daily_mu[i];
                let annual_mu = self.annual_mu[i];
                let pot_sst_c = self.scratch_pot_sst_c[i];
                let sea_ice_frac = self.scratch_sea_ice_frac[i];

                let cloud_tot = (self.cloud_low[i] + 0.55 * self.cloud_high[i]).clamp(0.0, 1.0);
                let surf_albedo = if is_ocean {
                    let omc = 1.0 - cos_z;
                    let open_alb = 0.06 + 0.14 * omc * omc * omc;
                    (1.0 - sea_ice_frac) * open_alb + sea_ice_frac * 0.62
                } else {
                    let sm0 = self.soil_moisture[i].clamp(0.0, 1.0);
                    let cover0 =
                        (0.72 * self.init_soil_moisture[i] + 0.28 * sm0).clamp(0.0, 1.0);
                    let canopy0 = ((cover0 - 0.12) / 0.30).clamp(0.0, 1.0);
                    let veg_alb = 0.30 - 0.14 * canopy0 - 0.04 * sm0;
                    let snow_frac = (self.snow_depth_m[i] / 0.10).clamp(0.0, 1.0);
                    let ice_sheet_frac = ((self.snow_depth_m[i] - 0.55) / 0.65).clamp(0.0, 1.0);
                    let snow_alb = 0.46 + 0.30 * ice_sheet_frac;
                    (1.0 - snow_frac) * veg_alb + snow_frac * snow_alb
                };

                let sw_trans = 1.0 - 0.52 * cloud_tot;
                let diurnal_sw_wm2 = 1361.0
                    * self.solar_luminosity
                    * (1.0 - surf_albedo)
                    * sw_trans
                    * (cos_z - daily_mu);
                let cloud_lw_anom_wm2 = 14.0 * (cloud_tot - 0.25);

                let wspd = self.wind1_speed_mps[i];

                let t_evap_k = if is_ocean {
                    self.tsurf_c[i] + 273.15
                } else {
                    0.60 * (self.tsurf_c[i] + 273.15) + 0.40 * (self.theta1_k[i] - 0.0065 * zs)
                };
                let qsat_surf = qsat_with_elev_base(t_evap_k, qsat_base);
                let sm = self.soil_moisture[i].clamp(0.0, 1.0);
                let beta_sm = ((sm - 0.10) / 0.55).clamp(0.0, 1.0);
                let beta_et = beta_sm * (0.35 + 0.65 * beta_sm);
                let rad_et_gate = 0.15 + 1.05 * cos_z_sqrt;
                let wet_frac = if is_ocean {
                    1.0 - 0.78 * sea_ice_frac
                } else if self.is_river[i] {
                    (beta_et * rad_et_gate).max(0.45 * (0.35 + 0.85 * cos_z_sqrt))
                } else {
                    beta_et * rad_et_gate
                };
                let w_sub_prev = (65_000.0 * (-self.scratch_w12[i]).max(0.0)).min(0.16);
                let q_eq_cap = if is_ocean {
                    qsat_surf * (0.91 - 2.6 * w_sub_prev).clamp(0.54, 0.91)
                } else {
                    0.90 * qsat_surf
                };
                let evap_base = if is_ocean { 1.42e-6 } else { 0.96e-6 };
                let evap_coeff = evap_base * (1.3 + 0.42 * wspd) * wet_frac;
                let evap_kgm2 =
                    (evap_coeff * (q_eq_cap - self.qv_kgm2[i]).max(0.0) * dt).min(evap_step_cap);
                self.qv_kgm2[i] += evap_kgm2;
                if !is_ocean && evap_kgm2 > 0.0 {
                    let soil_draw_share = if self.is_river[i] && wet_frac > 1e-5 {
                        ((beta_et * rad_et_gate) / wet_frac).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    self.soil_moisture[i] =
                        (self.soil_moisture[i] - (evap_kgm2 * soil_draw_share) / 300.0).max(0.0);
                }

                let rad_surf_c = radiative_equilibrium_surface_c_fast(
                    daily_mu,
                    annual_mu,
                    surf_albedo,
                    self.atmo_alb_term[i],
                    self.solar_luminosity,
                    glac_offset_c,
                    self.greenhouse_offset_k,
                    zs,
                );
                let bowen_arid_bonus_c = if is_ocean {
                    0.0
                } else {
                    11.0 * (0.42 - self.soil_moisture[i]).clamp(-0.10, 0.42)
                        * (daily_mu / 0.30).clamp(0.15, 1.35)
                };
                let local_eq_c = rad_surf_c + bowen_arid_bonus_c;

                if is_ocean {
                    let t_air_surf_c = (self.theta1_k[i] + 4.5) - 273.15;
                    let ice_eq_c =
                        (0.45 * rad_surf_c + 0.30 * t_air_surf_c + 0.25 * (-1.8)).min(0.0);
                    let target_sst = (1.0 - sea_ice_frac) * pot_sst_c.clamp(-1.8, 42.0)
                        + sea_ice_frac * ice_eq_c;
                    let c_ocean = (1.0 - sea_ice_frac) * 4.2e6 + sea_ice_frac * 3.4e5;
                    let tau_ocean_s = ((1.0 - sea_ice_frac) * 72.0 + sea_ice_frac * 14.0) * 3600.0;
                    let d_ts = (diurnal_sw_wm2 / c_ocean) * dt
                        + ((target_sst - self.tsurf_c[i]) / tau_ocean_s) * dt;
                    self.tsurf_c[i] = (self.tsurf_c[i] + d_ts).clamp(-45.0, 42.0);
                } else {
                    let t_air_surf_c = (self.theta1_k[i] - 0.0065 * zs + 4.5) - 273.15;
                    let w_air = (0.34 + 0.028 * wspd).clamp(0.34, 0.62);
                    let coupled_target_c = (1.0 - w_air) * local_eq_c + w_air * t_air_surf_c;

                    let c_land = 1.9e5 + 1.6e5 * self.soil_moisture[i];
                    let net_flux = diurnal_sw_wm2 + cloud_lw_anom_wm2;
                    let relax_rate = 1.0 / (10.0 * 3600.0);
                    let d_ts = (net_flux / c_land
                        + (coupled_target_c - self.tsurf_c[i]) * relax_rate)
                        * dt;
                    self.tsurf_c[i] = (self.tsurf_c[i] + d_ts).clamp(-65.0, 54.0);
                }

                // Boundary-layer sensible heat exchange + radiative relaxation on `theta1` & `theta2`:
                let ch_sens = if is_ocean {
                    0.0014 + 0.00030 * wspd
                } else {
                    (0.00125 + 0.00025 * wspd) * (1.0 + 0.35 * self.mountainosity_norm[i])
                };
                let t1_amb_k = (self.theta1_k[i] - 0.0065 * zs).clamp(195.0, 330.0);
                let qsat_amb = qsat_with_elev_base(t1_amb_k, qsat_base).max(0.2);
                let rh_amb = (self.qv_kgm2[i] / qsat_amb).clamp(0.0, 1.5);

                let cover_col =
                    (0.72 * self.init_soil_moisture[i] + 0.28 * self.soil_moisture[i])
                        .clamp(0.0, 1.0);
                let bare_erg = ((0.30 - cover_col) / 0.16).clamp(0.0, 1.0);
                let dry_ir_cool_k = if is_ocean {
                    0.0
                } else {
                    2.6 * bare_erg * ((0.46 - rh_amb) / 0.28).clamp(0.0, 1.0)
                };
                let bowen_col_share = 0.18 + 0.52 * (1.0 - bare_erg);
                let col_eq_c =
                    rad_surf_c + bowen_col_share * bowen_arid_bonus_c - dry_ir_cool_k;
                let ref_surf_c = if is_ocean {
                    self.tsurf_c[i]
                } else {
                    col_eq_c + 0.25 * (self.tsurf_c[i] - local_eq_c).clamp(-8.0, 8.0)
                };
                let ts_pot_k = ref_surf_c + 273.15 + 0.0065 * zs;
                let sensible_rate =
                    (ch_sens * (wspd + 2.5) / 1350.0) * (ts_pot_k - 4.5 - self.theta1_k[i]);

                let rad_eq_th1 = (col_eq_c + 273.15 + 0.0065 * zs - 4.5).clamp(210.0, 328.0);
                let stab_trop = 25.0 + 7.0 * (1.0 - (annual_mu * 2.8).clamp(0.0, 1.0));
                let rad_eq_th2 = rad_eq_th1 + stab_trop;
                let tau_rad1 = 1.0 / (10.0 * 86400.0);
                let tau_rad2 = 1.0 / (6.5 * 86400.0);

                let dry_cbl_mix = if is_ocean {
                    0.06
                } else {
                    (0.06 + 0.24 * ((0.48 - rh_amb) / 0.32).clamp(0.0, 1.0)).clamp(0.06, 0.30)
                };
                let low_sens_share = if is_ocean {
                    1.0
                } else {
                    1.0 - 0.65 * (dry_cbl_mix - 0.06)
                };

                self.theta1_k[i] = (self.theta1_k[i]
                    + dt * (low_sens_share * sensible_rate
                        + (rad_eq_th1 - self.theta1_k[i]) * tau_rad1))
                    .clamp(195.0, 345.0);
                self.theta2_k[i] = (self.theta2_k[i]
                    + dt * ((rad_eq_th2 - self.theta2_k[i]) * tau_rad2
                        + dry_cbl_mix * sensible_rate))
                    .clamp(215.0, 385.0);
            }

            // 2. Scale-aware subgrid Eliassen-Palm baroclinic eddy closure (Rhines 1975):
            let mean_dth_stab = (mean_th2 - mean_th1).clamp(14.0, 60.0);
            let c_bc = (0.25 * delta_exner * mean_dth_stab).sqrt();
            let kappa_eddy_raw = compute_subgrid_ep_eddy_forcing(
                geom,
                c_bc,
                d_mean,
                dtheta_eq_pole,
                sin_dec,
                &mut self.scratch_ep_east,
            );
            let kappa_eddy_max = kappa_eddy_raw.min(0.11 * d_min * d_min / dt);
            for (e_idx, e) in geom.edges.iter().enumerate() {
                self.scratch_ep_edge[e_idx] = 0.5
                    * (self.scratch_ep_east[e.left as usize] * e.nx_left
                        + self.scratch_ep_east[e.right as usize] * e.nx_right);
            }

            // 3. Fast 3-stage Wicker-Skamarock RK3 vector-invariant 2-layer dynamical core:
            for _dyn_step in 0..n_dyn {
                self.rk_u1_0.copy_from_slice(&self.u1_edge);
                self.rk_u2_0.copy_from_slice(&self.u2_edge);
                self.rk_ps_0.copy_from_slice(&self.ps_anom_pa);
                self.rk_th1_0.copy_from_slice(&self.theta1_k);
                self.rk_th2_0.copy_from_slice(&self.theta2_k);

                for (stage_idx, stage_dt) in [dt_dyn / 3.0, 0.5 * dt_dyn, dt_dyn]
                    .into_iter()
                    .enumerate()
                {
                    geom.reconstruct_and_div_2layer(
                        &self.u1_edge,
                        &self.u2_edge,
                        &mut self.u1_east,
                        &mut self.v1_north,
                        &mut self.scratch_div1,
                        &mut self.u2_east,
                        &mut self.v2_north,
                        &mut self.scratch_div2,
                    );
                    // Slowly evolving relative vorticity is evaluated once on Stage 1 per RK3 step,
                    // while fast gravity-wave divergence, geopotential, and Coriolis update every stage:
                    if stage_idx == 0 {
                        geom.curl_2layer(
                            &self.u1_east,
                            &self.v1_north,
                            &self.u2_east,
                            &self.v2_north,
                            &mut self.scratch_edge_flux,
                            &mut self.scratch_edge_flux2,
                            &mut self.scratch_zeta1,
                            &mut self.scratch_zeta2,
                        );
                    }

                    for i in 0..self.num_cells {
                        // Lorenz (1960) upward interface velocity `W_{1->2} = 0.5 * (div2 - div1)`:
                        self.scratch_w12[i] = 0.5 * (self.scratch_div2[i] - self.scratch_div1[i]);

                        let dth1 = self.theta1_k[i] - mean_th1;
                        let dth2 = self.theta2_k[i] - mean_th2;
                        let p_term = self.ps_anom_pa[i] * inv_rho_air;
                        let ke1 = 0.5
                            * (self.u1_east[i] * self.u1_east[i]
                                + self.v1_north[i] * self.v1_north[i]);
                        let ke2 = 0.5
                            * (self.u2_east[i] * self.u2_east[i]
                                + self.v2_north[i] * self.v2_north[i]);

                        let phi1 = p_term + alpha_therm1 * dth1;
                        let phi2 = phi1 + 0.5 * delta_exner * (dth1 + dth2);
                        self.scratch_phi1[i] = phi1 + ke1;
                        self.scratch_phi2[i] = phi2 + ke2;
                    }

                    for (e_idx, e) in geom.edges.iter().enumerate() {
                        let l = e.left as usize;
                        let r = e.right as usize;
                        let u1_cur = self.u1_edge[e_idx];
                        let u2_cur = self.u2_edge[e_idx];
                        let w12_e = 0.5 * (self.scratch_w12[l] + self.scratch_w12[r]);
                        let ep_edge = self.scratch_ep_edge[e_idx];
                        let drag = self.edge_drag[e_idx];
                        let fl = geom.coriolis_f[l];
                        let fr = geom.coriolis_f[r];

                        // Layer 1 vector-invariant momentum: `(zeta1 + f) k x u1 - grad(Phi1 + K1)`:
                        let dbern1_dn =
                            (self.scratch_phi1[r] - self.scratch_phi1[l]) * e.inv_center_dist_m;
                        let av1_l = fl + self.scratch_zeta1[l];
                        let av1_r = fr + self.scratch_zeta1[r];
                        let rot1 = 0.5
                            * (av1_l * (e.nx_left * self.v1_north[l] - e.ny_left * self.u1_east[l])
                                + av1_r
                                    * (e.nx_right * self.v1_north[r]
                                        - e.ny_right * self.u1_east[r]));
                        let du21 = u2_cur - u1_cur;
                        let vert_mom1 = ((-w12_e).max(0.0) + 6.5e-7) * du21;
                        let div_damp1 = nu_div
                            * (self.scratch_div1[r] - self.scratch_div1[l])
                            * e.inv_center_dist_m;

                        let drag1 = drag.r_ekman + drag.quad_drag1 * u1_cur.abs();
                        let rhs1 = rot1 - dbern1_dn + vert_mom1 + div_damp1 + 0.35 * ep_edge;
                        self.u1_edge[e_idx] = ((self.rk_u1_0[e_idx] + stage_dt * rhs1)
                            / (1.0 + stage_dt * drag1))
                            .clamp(-60.0, 60.0);

                        // Layer 2 vector-invariant momentum: `(zeta2 + f) k x u2 - grad(Phi2 + K2)`:
                        let dbern2_dn =
                            (self.scratch_phi2[r] - self.scratch_phi2[l]) * e.inv_center_dist_m;
                        let av2_l = fl + self.scratch_zeta2[l];
                        let av2_r = fr + self.scratch_zeta2[r];
                        let rot2 = 0.5
                            * (av2_l * (e.nx_left * self.v2_north[l] - e.ny_left * self.u2_east[l])
                                + av2_r
                                    * (e.nx_right * self.v2_north[r]
                                        - e.ny_right * self.u2_east[r]));
                        let vert_mom2 = -(w12_e.max(0.0) + 6.5e-7) * du21;
                        let div_damp2 = nu_div
                            * (self.scratch_div2[r] - self.scratch_div2[l])
                            * e.inv_center_dist_m;
                        let r_upper = drag.r_upper_base + 2.0e-7 * u2_cur.abs();
                        let rhs2 = rot2 - dbern2_dn + vert_mom2 + div_damp2 + 1.35 * ep_edge;
                        self.u2_edge[e_idx] = ((self.rk_u2_0[e_idx] + stage_dt * rhs2)
                            / (1.0 + stage_dt * r_upper))
                            .clamp(-110.0, 110.0);
                    }

                    for i in 0..self.num_cells {
                        let col_div = 0.5 * (self.scratch_div1[i] + self.scratch_div2[i]);
                        self.ps_anom_pa[i] = (self.rk_ps_0[i]
                            - stage_dt * rho_air * c_ext_sq * col_div)
                            .clamp(-9500.0, 6500.0);

                        // Lorenz static-stability adiabatic cooling on ascent (`w12 > 0`) & warming on subsidence (`w12 < 0`):
                        let dtheta_stab = (self.theta2_k[i] - self.theta1_k[i]).clamp(12.0, 65.0);
                        let adiab_dth = -0.5 * dtheta_stab * self.scratch_w12[i];
                        self.theta1_k[i] =
                            (self.rk_th1_0[i] + stage_dt * adiab_dth).clamp(195.0, 345.0);
                        self.theta2_k[i] =
                            (self.rk_th2_0[i] + stage_dt * adiab_dth).clamp(215.0, 385.0);
                    }
                }
            }

            // Recompute post-RK3 divergences & column-mass-consistent moisture transport velocity:
            geom.divergence_2layer(
                &self.u1_edge,
                &self.u2_edge,
                &mut self.scratch_div1,
                &mut self.scratch_div2,
            );
            for i in 0..self.num_cells {
                self.scratch_w12[i] = 0.5 * (self.scratch_div2[i] - self.scratch_div1[i]);
                let zs = self.surf_elev_m[i];
                let t1_k = (self.theta1_k[i] - 0.0065 * zs).clamp(195.0, 330.0);
                let qsat = qsat_with_elev_base(t1_k, self.qsat_elev_base[i]).max(0.2);
                self.scratch_phi1[i] = (self.qv_kgm2[i] / qsat).clamp(0.0, 1.5);
            }
            let mut max_u_abs = 1.0_f32;
            for (e_idx, e) in geom.edges.iter().enumerate() {
                let ue1 = self.u1_edge[e_idx];
                let ue2 = self.u2_edge[e_idx];
                max_u_abs = max_u_abs.max(ue1.abs()).max(ue2.abs());
                let u_col = 0.5 * (ue1 + ue2);
                let u_bc = 0.5 * (ue1 - ue2);
                let rh_up = if u_col + 0.5 * u_bc >= 0.0 {
                    self.scratch_phi1[e.left as usize]
                } else {
                    self.scratch_phi1[e.right as usize]
                };
                let low_trap = 0.16 + 0.40 * ((rh_up - 0.25) / 0.30).clamp(0.0, 1.0);
                self.scratch_moist_edge[e_idx] = u_col + low_trap * u_bc;
                self.scratch_u2_col[e_idx] = u_col;
            }

            // 4. Conservative upwind finite-volume advection of (theta1, theta2) and (qv, qc):
            let n_adv_sub = ((dt * max_u_abs / (0.62 * d_min)).ceil() as usize).clamp(1, 6);
            let dt_adv = dt / (n_adv_sub as f32);
            for _ in 0..n_adv_sub {
                advect_scalar_upwind_2layer(
                    geom,
                    &self.u1_edge,
                    &self.u2_edge,
                    &self.scratch_div1,
                    &self.scratch_div2,
                    dt_adv,
                    &mut self.theta1_k,
                    &mut self.theta2_k,
                    &mut self.scratch_edge_flux,
                    &mut self.scratch_edge_flux2,
                    &mut self.scratch_lap_a,
                    &mut self.scratch_lap_b,
                );
            }
            advect_tracer_flux_limited_2layer(
                geom,
                &self.scratch_moist_edge,
                &self.scratch_u2_col,
                dt,
                &mut self.qv_kgm2,
                &mut self.qc_kgm2,
                &mut self.scratch_edge_flux,
                &mut self.scratch_edge_flux2,
                &mut self.scratch_lap_a,
                &mut self.scratch_lap_b,
            );

            // Subgrid baroclinic eddy poleward heat & synoptic moisture diffusion (Stone 1972 / GM):
            if kappa_eddy_max > 0.0 {
                geom.laplacian_scalar(&self.theta1_k, &mut self.scratch_lap_a);
                let dt_ke = dt * kappa_eddy_max;
                for i in 0..self.num_cells {
                    self.theta1_k[i] = (self.theta1_k[i]
                        + dt_ke * self.extratrop_theta_weight[i] * self.scratch_lap_a[i])
                        .clamp(195.0, 345.0);
                    let zs = self.surf_elev_m[i];
                    let t1_k = (self.theta1_k[i] - 0.0065 * zs).clamp(195.0, 330.0);
                    self.scratch_lap_b[i] =
                        qsat_with_elev_base(t1_k, self.qsat_elev_base[i]).max(0.2);
                }
                let kappa_q_ext = 0.68 * kappa_eddy_max;
                let kappa_q_trop = 0.22 * kappa_eddy_max;
                for (e_idx, e) in geom.edges.iter().enumerate() {
                    let l = e.left as usize;
                    let r = e.right as usize;
                    let ext = self.edge_drag[e_idx].ext_q_weight;
                    let mut kappa_q = kappa_q_trop + (kappa_q_ext - kappa_q_trop) * ext;
                    let dq = self.qv_kgm2[r] - self.qv_kgm2[l];
                    if dq < 0.0 {
                        let rh_r = self.qv_kgm2[r] / self.scratch_lap_b[r];
                        let sub_gate =
                            (1.0 + 35_000.0 * self.scratch_w12[r].min(0.0)).clamp(0.15, 1.0);
                        kappa_q *= ((0.85 - rh_r) / 0.15).clamp(0.0, 1.0) * sub_gate;
                    } else if dq > 0.0 {
                        let rh_l = self.qv_kgm2[l] / self.scratch_lap_b[l];
                        let sub_gate =
                            (1.0 + 35_000.0 * self.scratch_w12[l].min(0.0)).clamp(0.15, 1.0);
                        kappa_q *= ((0.85 - rh_l) / 0.15).clamp(0.0, 1.0) * sub_gate;
                    }
                    self.scratch_edge_flux[e_idx] = -kappa_q * dq * e.inv_center_dist_m;
                }
                geom.divergence_from_edge_flux(&self.scratch_edge_flux, &mut self.scratch_lap_a);
                for i in 0..self.num_cells {
                    self.qv_kgm2[i] = (self.qv_kgm2[i] - dt * self.scratch_lap_a[i]).max(0.0);
                }
            }

            // Scale-selective biharmonic (-nu4 * nabla^4) hyperdiffusion on (theta1, theta2):
            apply_biharmonic_smoothing_2layer(
                geom,
                dt_nu4,
                &mut self.theta1_k,
                &mut self.theta2_k,
                &mut self.scratch_lap_a,
                &mut self.scratch_lap_b,
                &mut self.scratch_lap_c,
                &mut self.scratch_lap_d,
            );

            // 5. Moist microphysics, orographic/frontal lift, deep convection & latent heat release:
            geom.reconstruct_cell_vectors(&self.u1_edge, &mut self.u1_east, &mut self.v1_north);
            geom.gradient_scalar(&self.theta1_k, &mut self.scratch_gx, &mut self.scratch_gy);
            for i in 0..self.num_cells {
                let ux = self.u1_east[i];
                let vy = self.v1_north[i];
                self.wind1_speed_mps[i] = (ux * ux + vy * vy).sqrt();

                let is_ocean = self.tides.bed_m()[i] < 0.0 || self.tides.is_inundated_land(i);
                let zs = self.surf_elev_m[i];
                let qsat_base = self.qsat_elev_base[i];
                let w12 = self.scratch_w12[i];
                let t1_k = (self.theta1_k[i] - 0.0065 * zs).clamp(195.0, 330.0);
                let qsat_amb = qsat_with_elev_base(t1_k, qsat_base).max(0.2);
                let rh_amb = (self.qv_kgm2[i] / qsat_amb).clamp(0.0, 2.0);
                let warm_frac = ((t1_k - 268.15) / 22.0).clamp(0.0, 1.0);

                let slope_w = ux * self.surf_gx[i] + vy * self.surf_gy[i];
                let w_orog = slope_w.max(0.0);
                let w_downslope = (-slope_w).max(0.0);
                let warm_adv_ks = -(ux * self.scratch_gx[i] + vy * self.scratch_gy[i]).min(0.0);
                let dth_dz = ((self.theta2_k[i] - self.theta1_k[i]).max(14.0)) / 5500.0;
                let w_frontal = (warm_adv_ks / dth_dz).min(0.12);

                let synop_lcl_gate = ((rh_amb - 0.44) / 0.24).clamp(0.0, 1.0);
                let orog_lcl_gate = ((rh_amb - 0.34) / 0.26).clamp(0.0, 1.0);
                let w_synop = (55_000.0 * w12.max(0.0)
                    + 16_000.0 * (-self.scratch_div1[i]).max(0.0)
                    + w_frontal)
                .min(0.18)
                    * synop_lcl_gate;
                let w_lift = w_orog * orog_lcl_gate + w_synop;
                let hadley_sub_scale = 0.20 + 0.80 * warm_frac;
                let w_subside = hadley_sub_scale * (65_000.0 * (-w12).max(0.0)).min(0.18)
                    + 0.85 * w_downslope;

                let lift_cool_k = (14.0 * w_lift - 15.0 * w_subside).clamp(-6.5, 6.5);
                let qsat_eff = qsat_with_elev_base(t1_k - lift_cool_k, qsat_base);

                let rh = (self.qv_kgm2[i] / qsat_eff.max(0.2)).clamp(0.0, 2.0);
                let subside_cap_k = 1.45 + 72.0 * w_subside;
                let surf_buoy_k = if is_ocean {
                    0.0
                } else {
                    0.55 * (self.tsurf_c[i] + 273.15 + 0.0065 * zs - self.theta1_k[i] - 4.5)
                        .clamp(0.0, 8.0)
                        + 1.35
                };
                let theta_e_excess = (self.theta1_k[i] + surf_buoy_k + 0.95 * self.qv_kgm2[i])
                    - (self.theta2_k[i] - 14.0);
                let conv_rh_min = if is_ocean { 0.80 } else { 0.57 };
                let mut conv_cond = 0.0_f32;
                if theta_e_excess > subside_cap_k && rh > conv_rh_min && rh_amb > 0.49 {
                    let max_conv_rate = if is_ocean { 0.036 } else { 0.024 };
                    let rate = ((theta_e_excess - subside_cap_k) * 0.0055).min(max_conv_rate)
                        * (dt / 3600.0);
                    conv_cond = ((self.qv_kgm2[i] - (conv_rh_min - 0.03) * qsat_eff).max(0.0)
                        * rate)
                        .min(self.qv_kgm2[i] * conv_frac_cap);
                }
                let conv_precip_mm = 0.68 * conv_cond;
                let conv_detrain = conv_cond - conv_precip_mm;
                let qv_after_conv = (self.qv_kgm2[i] - conv_cond).max(0.0);

                let base_rh_crit = if is_ocean {
                    0.79 - 0.06 * (1.0 - warm_frac)
                } else {
                    0.71 - 0.16 * (1.0 - warm_frac)
                };
                let rh_crit = (base_rh_crit + 2.8 * w_subside - 0.50 * w_lift).clamp(0.50, 0.98);
                let q_crit = rh_crit * qsat_eff;
                let ls_cond = if qv_after_conv > q_crit {
                    let base_ls = 0.019 + 0.020 * (1.0 - warm_frac);
                    let rate_ls = (base_ls
                        + 0.34 * w_frontal.min(0.08)
                        + 0.24 * (w_orog * 12.0).min(1.0))
                        * (dt / 3600.0);
                    (qv_after_conv - q_crit) * rate_ls
                } else {
                    let evap_rate = (0.030 + 2.2 * w_subside).min(0.65) * (dt / 1800.0);
                    let sub_def = (q_crit - qv_after_conv) * evap_rate;
                    -sub_def.min(self.qc_kgm2[i])
                };

                let ls_clamped = ls_cond.clamp(-self.qc_kgm2[i], qv_after_conv * 0.70);
                let net_cond = conv_cond + ls_clamped;
                self.qv_kgm2[i] = (self.qv_kgm2[i] - net_cond).max(0.0);
                self.qc_kgm2[i] = (self.qc_kgm2[i] + conv_detrain + ls_clamped).max(0.0);

                let latent_dtheta = 0.42 * net_cond;
                self.theta1_k[i] = (self.theta1_k[i] + 0.40 * latent_dtheta).clamp(195.0, 345.0);
                self.theta2_k[i] = (self.theta2_k[i] + 0.60 * latent_dtheta).clamp(215.0, 385.0);

                let qc = self.qc_kgm2[i];
                let qc_crit = 0.025 + 0.095 * warm_frac;
                let auto_precip_mm = if qc > qc_crit {
                    let orog_boost = 1.0 + 2.4 * (w_orog * 12.0).min(1.0);
                    let auto = (qc - qc_crit) * orog_boost * (dt / 7200.0);
                    auto.min(qc * 0.65)
                } else {
                    0.0
                };
                self.qc_kgm2[i] = (qc - auto_precip_mm).max(0.0);
                let precip_step_mm = conv_precip_mm + auto_precip_mm;

                let snow_frac = ((1.2 - self.tsurf_c[i]) / 2.8).clamp(0.0, 1.0);
                let snow_mm = precip_step_mm * snow_frac;
                let rain_mm = precip_step_mm - snow_mm;
                self.rain_mm_hr[i] += rain_mm;
                self.snow_mm_hr[i] += snow_mm;

                // Land surface snowpack & deep root-zone Manabe bucket update (`300 mm` field capacity):
                if !is_ocean {
                    self.snow_depth_m[i] += snow_mm * 0.008;
                    let melt_depth_m = if self.tsurf_c[i] > 0.2 && self.snow_depth_m[i] > 0.0 {
                        (0.00085 * self.tsurf_c[i] * (dt / 3600.0)).min(self.snow_depth_m[i])
                    } else {
                        0.0
                    };
                    self.snow_depth_m[i] -= melt_depth_m;
                    let liquid_in_mm = rain_mm + melt_depth_m * 125.0;
                    if liquid_in_mm > 0.0 {
                        let sat = self.soil_moisture[i].clamp(0.0, 1.0);
                        let runoff_frac =
                            (sat * sat * sat * (0.16 + 0.64 * self.mountainosity_norm[i]))
                                .clamp(0.02, 1.0);
                        let infil_mm = liquid_in_mm * (1.0 - runoff_frac);
                        let new_sm = self.soil_moisture[i] + infil_mm / 300.0;
                        let bucket_overflow_mm = ((new_sm - 1.0).max(0.0)) * 300.0;
                        self.soil_moisture[i] = new_sm.clamp(0.0, 1.0);
                        let total_runoff_mm = liquid_in_mm * runoff_frac + bucket_overflow_mm;
                        self.scratch_runoff_m[i] += total_runoff_mm * 0.001;
                    }
                } else {
                    self.snow_depth_m[i] = 0.0;
                }

                // Update cloud fractions for subsequent substeps:
                self.cloud_low[i] = ((self.qc_kgm2[i] / 1.25).sqrt()).clamp(0.0, 1.0);
            }
        }

        // Reconstruct final end-of-hour winds in a single Morton stencil pass:
        if geom.all_deg_le_6 {
            for st in &geom.morton_perot6 {
                let mut u1x = 0.0_f32;
                let mut v1y = 0.0_f32;
                let mut u2x = 0.0_f32;
                let mut v2y = 0.0_f32;
                for k in 0..6 {
                    let e = st.edge_idx[k] as usize;
                    let wx = st.perot_wx[k];
                    let wy = st.perot_wy[k];
                    let ue1 = self.u1_edge[e];
                    let ue2 = self.u2_edge[e];
                    u1x += ue1 * wx;
                    v1y += ue1 * wy;
                    u2x += ue2 * wx;
                    v2y += ue2 * wy;
                }
                let c = st.cell as usize;
                self.u1_east[c] = u1x;
                self.v1_north[c] = v1y;
                self.u2_east[c] = u2x;
                self.v2_north[c] = v2y;
            }
        } else {
            geom.reconstruct_cell_vectors(&self.u1_edge, &mut self.u1_east, &mut self.v1_north);
            geom.reconstruct_cell_vectors(&self.u2_edge, &mut self.u2_east, &mut self.v2_north);
        }

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
        for i in 0..self.num_cells {
            self.scratch_accum_m3s[i] = if self.scratch_runoff_m[i] > 0.0 {
                self.scratch_runoff_m[i] * geom.areas_m2[i] * (1.0 / 3600.0)
            } else {
                0.0
            };
        }
        for &u_idx in &self.topo_order {
            let u = u_idx as usize;
            let dst = self.downstream_cell[u];
            if dst != u32::MAX {
                let q = self.scratch_accum_m3s[u];
                self.scratch_accum_m3s[dst as usize] += q;
            }
        }
        for i in 0..self.num_cells {
            // Smooth 6-hour river channel reservoir response so storm hydrographs crest & recede naturally:
            self.river_flow_m3s[i] =
                0.78 * self.river_flow_m3s[i] + 0.22 * self.scratch_accum_m3s[i];
        }

        // Step coupled dynamic tides + storm surge AND unforced astronomical reference tide:
        self.tides.step_hour_paired_storm_surge(
            &mut self.astro_ref_tides,
            geom,
            AtmosphericTideForcing {
                p_anom_pa: &self.ps_anom_pa,
                tau_east_pa: &self.tau_east_pa,
                tau_north_pa: &self.tau_north_pa,
            },
            &mut self.storm_surge_m,
        );

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
        let mut frame = if self.meteogram_history.len() >= METEOGRAM_HOURS {
            self.meteogram_history
                .pop_front()
                .unwrap_or_else(|| vec![MeteogramSample::default(); self.num_cells])
        } else {
            vec![MeteogramSample::default(); self.num_cells]
        };
        if frame.len() != self.num_cells {
            frame.resize(self.num_cells, MeteogramSample::default());
        }
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
            frame[i] = MeteogramSample {
                temp_tenths_c: t10,
                slp_tenths_hpa: slp10,
                precip_hundredths_mm: p100,
                wind_tenths_mps: w10,
            };
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

/// Fused 2-layer advective form $\partial_t \theta + \mathbf{u}\cdot\nabla\theta = 0$ written as
/// $\nabla\cdot(\theta\mathbf{u}) - \theta\nabla\cdot\mathbf{u}$.
#[allow(clippy::too_many_arguments)]
fn advect_scalar_upwind_2layer(
    geom: &FiniteVolumeSphereGeom,
    u1_edge: &[f32],
    u2_edge: &[f32],
    div_u1: &[f32],
    div_u2: &[f32],
    dt: f32,
    field1: &mut [f32],
    field2: &mut [f32],
    scratch_flux1: &mut [f32],
    scratch_flux2: &mut [f32],
    scratch_div_fu1: &mut [f32],
    scratch_div_fu2: &mut [f32],
) {
    for (e_idx, e) in geom.edges.iter().enumerate() {
        let l = e.left as usize;
        let r = e.right as usize;
        let u1 = u1_edge[e_idx];
        let u2 = u2_edge[e_idx];
        scratch_flux1[e_idx] = u1 * if u1 >= 0.0 { field1[l] } else { field1[r] };
        scratch_flux2[e_idx] = u2 * if u2 >= 0.0 { field2[l] } else { field2[r] };
    }
    geom.divergence_2layer(
        scratch_flux1,
        scratch_flux2,
        scratch_div_fu1,
        scratch_div_fu2,
    );
    for i in 0..geom.num_cells {
        field1[i] -= dt * (scratch_div_fu1[i] - field1[i] * div_u1[i]);
        field2[i] -= dt * (scratch_div_fu2[i] - field2[i] * div_u2[i]);
    }
}

/// Fused 2-tracer positivity-preserving upwind flux transport for non-negative moisture tracers (`qv`, `qc`).
#[allow(clippy::too_many_arguments)]
fn advect_tracer_flux_limited_2layer(
    geom: &FiniteVolumeSphereGeom,
    u1_edge: &[f32],
    u2_edge: &[f32],
    dt: f32,
    tracer1: &mut [f32],
    tracer2: &mut [f32],
    scratch_flux1: &mut [f32],
    scratch_flux2: &mut [f32],
    scratch_outflow1: &mut [f32],
    scratch_outflow2: &mut [f32],
) {
    scratch_outflow1.fill(0.0);
    scratch_outflow2.fill(0.0);
    for (e_idx, e) in geom.edges.iter().enumerate() {
        let l = e.left as usize;
        let r = e.right as usize;
        let dt_len = dt * e.dual_len_m;

        let u1 = u1_edge[e_idx];
        if u1 > 0.0 {
            let q = u1 * tracer1[l];
            scratch_flux1[e_idx] = q;
            scratch_outflow1[l] += dt_len * q;
        } else if u1 < 0.0 {
            let q = u1 * tracer1[r];
            scratch_flux1[e_idx] = q;
            scratch_outflow1[r] += dt_len * (-q);
        } else {
            scratch_flux1[e_idx] = 0.0;
        }

        let u2 = u2_edge[e_idx];
        if u2 > 0.0 {
            let q = u2 * tracer2[l];
            scratch_flux2[e_idx] = q;
            scratch_outflow2[l] += dt_len * q;
        } else if u2 < 0.0 {
            let q = u2 * tracer2[r];
            scratch_flux2[e_idx] = q;
            scratch_outflow2[r] += dt_len * (-q);
        } else {
            scratch_flux2[e_idx] = 0.0;
        }
    }

    for (e_idx, e) in geom.edges.iter().enumerate() {
        let l = e.left as usize;
        let r = e.right as usize;

        let q1 = scratch_flux1[e_idx];
        if q1 > 0.0 {
            let avail = 0.90 * tracer1[l] * geom.areas_m2[l];
            if scratch_outflow1[l] > avail && scratch_outflow1[l] > 1e-9 {
                scratch_flux1[e_idx] = q1 * (avail / scratch_outflow1[l]);
            }
        } else if q1 < 0.0 {
            let avail = 0.90 * tracer1[r] * geom.areas_m2[r];
            if scratch_outflow1[r] > avail && scratch_outflow1[r] > 1e-9 {
                scratch_flux1[e_idx] = q1 * (avail / scratch_outflow1[r]);
            }
        }

        let q2 = scratch_flux2[e_idx];
        if q2 > 0.0 {
            let avail = 0.90 * tracer2[l] * geom.areas_m2[l];
            if scratch_outflow2[l] > avail && scratch_outflow2[l] > 1e-9 {
                scratch_flux2[e_idx] = q2 * (avail / scratch_outflow2[l]);
            }
        } else if q2 < 0.0 {
            let avail = 0.90 * tracer2[r] * geom.areas_m2[r];
            if scratch_outflow2[r] > avail && scratch_outflow2[r] > 1e-9 {
                scratch_flux2[e_idx] = q2 * (avail / scratch_outflow2[r]);
            }
        }
    }

    geom.divergence_2layer(
        scratch_flux1,
        scratch_flux2,
        scratch_outflow1,
        scratch_outflow2,
    );
    for i in 0..geom.num_cells {
        tracer1[i] = (tracer1[i] - dt * scratch_outflow1[i]).max(0.0);
        tracer2[i] = (tracer2[i] - dt * scratch_outflow2[i]).max(0.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_biharmonic_smoothing_2layer(
    geom: &FiniteVolumeSphereGeom,
    dt_nu4: f32,
    field1: &mut [f32],
    field2: &mut [f32],
    lap1_a: &mut [f32],
    lap2_a: &mut [f32],
    lap1_b: &mut [f32],
    lap2_b: &mut [f32],
) {
    geom.laplacian_2layer(field1, field2, lap1_a, lap2_a);
    geom.laplacian_2layer(lap1_a, lap2_a, lap1_b, lap2_b);
    for i in 0..geom.num_cells {
        field1[i] -= dt_nu4 * lap1_b[i];
        field2[i] -= dt_nu4 * lap2_b[i];
    }
}

/// Orbital-mean (16-point solar longitude average) daily solar zenith cosine for arbitrary
/// axial tilt `tilt_deg` (`0..89 deg`).
pub fn orbital_annual_mean_cos_z(sin_phi: f32, cos_phi: f32, tilt_deg: f64) -> f32 {
    let sin_tilt = (tilt_deg.clamp(0.0, 89.0).to_radians().sin()) as f32;
    let mut sum_mu = 0.0_f32;
    for k in 0..16 {
        let ls = (k as f32) * (std::f32::consts::TAU / 16.0);
        sum_mu += exact_daily_mean_cos_z(sin_phi, cos_phi, sin_tilt * ls.sin());
    }
    sum_mu * (1.0 / 16.0)
}

/// Unclamped open-ocean thermal potential (°C) at modern Earth baseline (`glaciation = 0.35`),
/// driven purely by orbital-mean insolation `annual_mu` and gyre `sst_anomaly` (no `sin^2(lat)`
/// assumption, so high-obliquity planets naturally place cooler waters in low-insolation belts).
#[inline]
pub fn equilibrium_open_sst_c(annual_mu: f32, solar_luminosity: f32, sst_anomaly: f32) -> f32 {
    (-29.7 + 190.0 * solar_luminosity * annual_mu + 5.2 * sst_anomaly).clamp(-32.0, 34.0)
}

/// Uniform radiative greenhouse/glaciation offset (°C) mapping `0.0` (icehouse, `-6.65 °C`),
/// `0.35` (present-day Earth, `0.0 °C`), `0.50` (tectonic average, `+2.85 °C`), and
/// `1.0` (greenhouse, `+12.35 °C`). Cold-region / ice-margin amplification emerges physically
/// from sea-ice and snow albedo + sea-ice thermal insulation rather than latitude formulas.
#[inline]
pub fn glaciation_temp_offset_c(glaciation: f32) -> f32 {
    19.0 * (glaciation.clamp(0.0, 1.0) - 0.35)
}

/// Budyko–Sellers / North & Coakley (1979) radiative-convective surface equilibrium (°C)
/// driven directly by daily-mean (`daily_mu`) and orbital-mean (`annual_mu`) solar zenith cosines
/// (valid for arbitrary axial tilt `0..89 deg`, preserving bright ice-sheet albedo and high-plateau thin air).
#[inline]
fn radiative_equilibrium_surface_c(
    daily_mu: f32,
    annual_mu: f32,
    surf_albedo: f32,
    solar_luminosity: f32,
    glac_offset_c: f32,
    greenhouse_offset_k: f32,
    zs_m: f32,
) -> f32 {
    let atmo_atten = (-zs_m.max(0.0) / 4800.0).exp();
    let slant_scatter = (1.0 - (annual_mu * 2.85).clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let atmo_alb_term = 0.42 * (0.20 + 0.12 * slant_scatter) * atmo_atten;
    radiative_equilibrium_surface_c_fast(
        daily_mu,
        annual_mu,
        surf_albedo,
        atmo_alb_term,
        solar_luminosity,
        glac_offset_c,
        greenhouse_offset_k,
        zs_m,
    )
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn radiative_equilibrium_surface_c_fast(
    daily_mu: f32,
    annual_mu: f32,
    surf_albedo: f32,
    atmo_alb_term: f32,
    solar_luminosity: f32,
    glac_offset_c: f32,
    greenhouse_offset_k: f32,
    zs_m: f32,
) -> f32 {
    let eff_mu = annual_mu + 0.58 * (daily_mu - annual_mu);
    let base_alb = 0.58 * surf_albedo + atmo_alb_term;
    let ice_floor = if surf_albedo > 0.55 {
        0.88 * surf_albedo
    } else {
        0.0
    };
    let eff_albedo = base_alb.max(ice_floor).clamp(0.06, 0.74);
    (-38.0
        + solar_luminosity * (1.0 - eff_albedo) * (235.0 * eff_mu + 38.0 * annual_mu)
        + glac_offset_c
        + greenhouse_offset_k
        - 0.0065 * zs_m)
        .clamp(-62.0, 52.0)
}

/// Scale-aware Rhines (1975) / Eliassen–Palm subgrid baroclinic eddy closure.
fn compute_subgrid_ep_eddy_forcing(
    geom: &FiniteVolumeSphereGeom,
    c_bc: f32,
    d_mean: f32,
    dtheta_eq_pole: f32,
    sin_dec: f32,
    out_ep_east: &mut [f32],
) -> f32 {
    let omega_abs = geom.omega_rad_s.abs() as f32;
    let u_rot = omega_abs * (geom.radius_m as f32);
    let s_rhines = u_rot / (11.33 * c_bc.max(10.0));
    let m_eddy = if s_rhines < 0.75 {
        0
    } else {
        (s_rhines + 0.5).floor() as u32
    };

    let baroc_scale = (dtheta_eq_pole / 38.0).clamp(0.0, 2.0);
    let f_mid = (1.4142 * omega_abs).max(1.0e-6);
    let inv_45_d = 1.0 / (4.5 * d_mean);
    let ld_mid_r = (c_bc / f_mid) * inv_45_d;
    let chi_mid = 1.0 / (1.0 + ld_mid_r * ld_mid_r);
    let kappa_eddy = if omega_abs > 1.0e-7 {
        3.6e5 * chi_mid * baroc_scale
    } else {
        0.0
    };

    if m_eddy == 0 || baroc_scale <= 0.02 {
        out_ep_east.fill(0.0);
        return kappa_eddy;
    }

    let n_cells = (2 * m_eddy + 1) as f32;
    let dec_rad = sin_dec.clamp(-1.0, 1.0).asin();
    let phi_itcz = 0.32 * dec_rad / (n_cells / 3.0).sqrt();
    let amp = 1.85e-4 * baroc_scale;
    let f_floor = (0.25 * omega_abs).max(1.0e-6);
    let half_pi = std::f32::consts::FRAC_PI_2;
    let c_bc_scaled = c_bc * inv_45_d;

    let mut pos_torque = 0.0_f64;
    let mut neg_torque = 0.0_f64;
    for i in 0..geom.num_cells {
        let ld_r = c_bc_scaled / geom.coriolis_f[i].abs().max(f_floor);
        let chi_sg = 1.0 / (1.0 + ld_r * ld_r);
        let phi_star = (geom.lats_rad[i] - phi_itcz).clamp(-half_pi, half_pi);
        let phi_abs = phi_star.abs();
        let eq_taper = 0.35 + 0.65 * (phi_abs * (1.0 / 0.32)).min(1.0);
        let a_raw = -amp * chi_sg * eq_taper * (2.0 * n_cells * phi_abs).sin();
        out_ep_east[i] = a_raw;

        let moment = (geom.areas_m2[i] as f64) * (geom.cos_lat[i] as f64);
        if a_raw > 0.0 {
            pos_torque += moment * (a_raw as f64);
        } else {
            neg_torque += moment * (-a_raw as f64);
        }
    }

    if pos_torque > 1.0e-12 && neg_torque > 1.0e-12 {
        let pos_scale = (neg_torque / pos_torque).sqrt().clamp(0.4, 2.5) as f32;
        let neg_scale = (pos_torque / neg_torque).sqrt().clamp(0.4, 2.5) as f32;
        for v in out_ep_east.iter_mut() {
            if *v > 0.0 {
                *v *= pos_scale;
            } else {
                *v *= neg_scale;
            }
        }
    }
    kappa_eddy
}

#[inline]
fn exact_daily_mean_cos_z_with_cos_dec(
    sin_phi: f32,
    cos_phi: f32,
    sin_dec: f32,
    cos_dec: f32,
) -> f32 {
    let denom = cos_phi * cos_dec;
    if denom <= 1.0e-6 {
        return (sin_phi * sin_dec).max(0.0);
    }
    let arg = (-sin_phi * sin_dec / denom).clamp(-1.0, 1.0);
    let h0 = arg.acos();
    let sin_h0 = (1.0 - arg * arg).max(0.0).sqrt();
    (std::f32::consts::FRAC_1_PI * (h0 * sin_phi * sin_dec + denom * sin_h0)).max(0.0)
}

/// Exact 24-hour mean of $\max(0, \cos z) = \max(0, \sin\phi\sin\delta + \cos\phi\cos\delta\cos H)$
/// integrated over hour angle $H \in [-\pi, \pi]$ via sunrise/sunset half-day angle $h_0$.
#[inline]
fn exact_daily_mean_cos_z(sin_phi: f32, cos_phi: f32, sin_dec: f32) -> f32 {
    let cos_dec = (1.0 - sin_dec * sin_dec).max(0.0).sqrt();
    exact_daily_mean_cos_z_with_cos_dec(sin_phi, cos_phi, sin_dec, cos_dec)
}

#[inline]
fn qsat_with_elev_base(t_k: f32, qsat_elev_base: f32) -> f32 {
    let tc = (t_k - 273.15).clamp(-65.0, 52.0);
    let es_ratio = (17.67 * tc / (tc + 243.5)).exp();
    (qsat_elev_base * es_ratio).clamp(0.05, 110.0)
}

/// Bolton (1980) Clausius–Clapeyron saturation column water vapor ($\text{kg m}^{-2}$)
/// as a function of lower-tropospheric temperature `t_k` (K) and surface elevation `zs_m` (m).
#[inline]
pub fn saturation_column_vapor_kgm2(t_k: f32, zs_m: f32) -> f32 {
    let qsat_base = 9.2 * (-zs_m.max(0.0) / 2400.0).exp();
    qsat_with_elev_base(t_k, qsat_base)
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

    #[test]
    fn test_exact_daily_mean_cos_z_has_zero_diurnal_bias_at_all_latitudes_and_seasons() {
        for &dec_deg in &[-23.44_f32, 0.0, 23.44] {
            let dec = dec_deg.to_radians();
            let sin_dec = dec.sin();
            let cos_dec = dec.cos();
            for lat_step in -89..=89 {
                let phi = (lat_step as f32).to_radians();
                let sin_phi = phi.sin();
                let cos_phi = phi.cos();
                let analytic = exact_daily_mean_cos_z(sin_phi, cos_phi, sin_dec);

                let n_samples = 720;
                let mut sum = 0.0_f32;
                for k in 0..n_samples {
                    let h = -std::f32::consts::PI
                        + 2.0 * std::f32::consts::PI * ((k as f32 + 0.5) / (n_samples as f32));
                    let cos_z = (sin_phi * sin_dec + cos_phi * cos_dec * h.cos()).max(0.0);
                    sum += cos_z;
                }
                let numeric = sum / (n_samples as f32);
                assert!(
                    (analytic - numeric).abs() < 1.0e-4,
                    "daily mean cos_z mismatch at lat={lat_step} dec={dec_deg}: analytic={analytic} vs numeric={numeric}"
                );
            }
        }
    }

    #[test]
    fn test_glaciation_convention_matches_icehouse_zero_to_greenhouse_one() {
        let grid = SphericalGrid::with_size(3);
        let adj = grid.adjacency();
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, 24.0);
        let n = geom.num_cells;
        let bed_m = vec![200.0_f32; n];
        let mount = vec![0.1_f32; n];
        let is_river = vec![false; n];
        let base_sst = vec![20.0_f32; n];
        let soil = vec![0.5_f32; n];
        let snow = vec![0.0_f32; n];

        let make_solver = |glaciation: f32| {
            MoistBaroclinicSolver::new(
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
                    glaciation,
                    atmosphere_id: 0,
                    moon_count: 1,
                    primary_mass_lunar: 1.0,
                    primary_dist_radii: 60.27,
                },
            )
        };

        let mut icehouse = make_solver(0.0);
        let mut greenhouse = make_solver(1.0);
        icehouse.step_hour(&geom);
        greenhouse.step_hour(&geom);

        let mean_ice: f32 = icehouse.tsurf_c.iter().sum::<f32>() / (n as f32);
        let mean_green: f32 = greenhouse.tsurf_c.iter().sum::<f32>() / (n as f32);
        assert!(
            mean_green > mean_ice + 8.0,
            "expected greenhouse (1.0) warmer than icehouse (0.0), got greenhouse={mean_green:.2} vs icehouse={mean_ice:.2}"
        );
    }

    #[test]
    fn test_principled_multi_cell_zonal_winds_for_1_3_5_7_cell_rotators() {
        let grid = SphericalGrid::with_size(5);
        let adj = grid.adjacency();
        let n = grid.faces.len();

        let bed_m = vec![-3500.0_f32; n];
        let mount = vec![0.0_f32; n];
        let is_river = vec![false; n];
        let soil = vec![1.0_f32; n];
        let snow = vec![0.0_f32; n];
        let base_sst: Vec<f32> = grid
            .faces
            .iter()
            .map(|&(p, _)| (28.5 - 30.0 * (p[2] * p[2]) as f32).max(-1.8))
            .collect();

        for &(day_hours, expected_cells) in &[(64.0, 1usize), (24.0, 3), (12.0, 5), (8.0, 7)] {
            let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, day_hours);
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
                    day_hours,
                    tilt_deg: 0.0,
                    days_per_year: 365.0,
                    home_init_angle_rad: 0.0,
                    solar_tide_factor: 0.0,
                    solar_luminosity: 1.0,
                    glaciation: 0.35,
                    atmosphere_id: 0,
                    moon_count: 0,
                    primary_mass_lunar: 0.0,
                    primary_dist_radii: 60.27,
                },
            );

            // Spin up 12 Earth-equivalent days (`288 h`) from rest at equinox (`tilt = 0`):
            let mut cell_band_u_sum = vec![0.0_f64; expected_cells];
            let mut cell_band_w_sum = vec![0.0_f64; expected_cells];
            for h in 0..288 {
                solver.step_hour(&geom);
                if h >= 192 {
                    for i in 0..n {
                        let abs_lat_deg = (geom.lats_rad[i].abs().to_degrees() as f64).min(89.999);
                        // Sample interior core (`20%..80%`) of each theoretical `90 / N` cell band:
                        let cell_width = 90.0 / (expected_cells as f64);
                        let band = (abs_lat_deg / cell_width).floor() as usize;
                        let frac = (abs_lat_deg - (band as f64) * cell_width) / cell_width;
                        if band < expected_cells && (0.20..=0.80).contains(&frac) {
                            let w = geom.areas_m2[i] as f64;
                            cell_band_u_sum[band] += (solver.u1_east[i] as f64) * w;
                            cell_band_w_sum[band] += w;
                        }
                    }
                }
            }

            assert!(
                solver.u1_east.iter().all(|v| v.is_finite())
                    && solver.u2_east.iter().all(|v| v.is_finite()),
                "non-finite winds at day_hours={day_hours}"
            );

            let band_means: Vec<f64> = (0..expected_cells)
                .map(|b| cell_band_u_sum[b] / cell_band_w_sum[b].max(1.0))
                .collect();

            for (b, &u_mean) in band_means.iter().enumerate() {
                let expect_westerly = (b % 2) == 1;
                if expect_westerly {
                    assert!(
                        u_mean > 0.5,
                        "day_hours={day_hours} ({expected_cells} cells): expected westerly (> +0.5 m/s) in band {b}, got {band_means:?}"
                    );
                } else {
                    assert!(
                        u_mean < -0.5,
                        "day_hours={day_hours} ({expected_cells} cells): expected easterly (< -0.5 m/s) in band {b}, got {band_means:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_subtropical_desert_heat_low_stays_arid_near_sea() {
        let grid = SphericalGrid::with_size(4);
        let adj = grid.adjacency();
        let n = grid.faces.len();
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, 24.0);

        let mut bed_m = vec![-3500.0_f32; n];
        let mut soil = vec![1.0_f32; n];
        let mount = vec![0.0_f32; n];
        let is_river = vec![false; n];
        let snow = vec![0.0_f32; n];
        let mut desert_cells = Vec::new();

        for i in 0..n {
            let lat_deg = geom.lats_rad[i].to_degrees();
            let lon_deg = geom.lons_rad[i].to_degrees();
            if (16.0..=32.0).contains(&lat_deg) && (-15.0..=55.0).contains(&lon_deg) {
                bed_m[i] = 450.0;
                soil[i] = 0.06;
                if (20.0..=28.0).contains(&lat_deg) && (0.0..=40.0).contains(&lon_deg) {
                    desert_cells.push(i);
                }
            }
        }
        assert!(!desert_cells.is_empty());

        let base_sst: Vec<f32> = (0..n)
            .map(|i| (28.5 - 30.2 * geom.sin_lat[i].powi(2)).max(-1.8))
            .collect();

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
                tilt_deg: 23.5,
                days_per_year: 365.25,
                home_init_angle_rad: std::f64::consts::FRAC_PI_2, // Northern summer solstice
                solar_tide_factor: 0.0,
                solar_luminosity: 1.0,
                glaciation: 0.35,
                atmosphere_id: 0,
                moon_count: 0,
                primary_mass_lunar: 0.0,
                primary_dist_radii: 60.27,
            },
        );

        let mut desert_rain_mm = 0.0_f64;
        for h in 0..240 {
            solver.step_hour(&geom);
            if h >= 72 {
                for &c in &desert_cells {
                    desert_rain_mm += (solver.rain_mm_hr[c] + solver.snow_mm_hr[c]) as f64;
                }
            }
        }
        let mean_mm_per_day = desert_rain_mm / ((desert_cells.len() as f64) * 7.0);
        assert!(
            mean_mm_per_day < 1.0,
            "expected hot subtropical desert interior to remain arid (< 1.0 mm/day) despite summer thermal low, got {mean_mm_per_day:.2} mm/day"
        );
    }

    #[test]
    fn test_equatorial_basin_retains_soil_moisture_without_desert_collapse() {
        let grid = SphericalGrid::with_size(4);
        let adj = grid.adjacency();
        let n = grid.faces.len();
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, 24.0);

        let mut bed_m = vec![-3500.0_f32; n];
        let mut soil = vec![1.0_f32; n];
        let mount = vec![0.0_f32; n];
        let is_river = vec![false; n];
        let snow = vec![0.0_f32; n];
        let mut interior_cells = Vec::new();

        for i in 0..n {
            let lat_deg = geom.lats_rad[i].to_degrees();
            let lon_deg = geom.lons_rad[i].to_degrees();
            if (-10.0..=10.0).contains(&lat_deg) && (-75.0..=-35.0).contains(&lon_deg) {
                bed_m[i] = 120.0;
                soil[i] = 0.45;
                if (-6.0..=6.0).contains(&lat_deg) && (-68.0..=-45.0).contains(&lon_deg) {
                    interior_cells.push(i);
                }
            }
        }
        assert!(!interior_cells.is_empty());

        let base_sst: Vec<f32> = (0..n)
            .map(|i| (28.5 - 30.2 * geom.sin_lat[i].powi(2)).max(-1.8))
            .collect();

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
                tilt_deg: 23.5,
                days_per_year: 365.25,
                home_init_angle_rad: 0.0,
                solar_tide_factor: 0.0,
                solar_luminosity: 1.0,
                glaciation: 0.35,
                atmosphere_id: 0,
                moon_count: 0,
                primary_mass_lunar: 0.0,
                primary_dist_radii: 60.27,
            },
        );

        let mut interior_rain_mm = 0.0_f64;
        for h in 0..360 {
            solver.step_hour(&geom);
            if h >= 120 {
                for &c in &interior_cells {
                    interior_rain_mm += (solver.rain_mm_hr[c] + solver.snow_mm_hr[c]) as f64;
                }
            }
        }
        let mean_sm: f32 = interior_cells
            .iter()
            .map(|&c| solver.soil_moisture[c])
            .sum::<f32>()
            / (interior_cells.len() as f32);
        let mean_mm_per_day = interior_rain_mm / ((interior_cells.len() as f64) * 10.0);
        assert!(
            mean_sm > 0.32 && mean_mm_per_day > 1.2,
            "expected equatorial interior basin to retain soil moisture (> 0.32) and rain (> 1.2 mm/day), got sm={mean_sm:.3}, rain={mean_mm_per_day:.2} mm/day"
        );
    }

    #[test]
    fn test_tilt_general_ocean_sst_and_sea_ice_amplification() {
        let grid = SphericalGrid::with_size(4);
        let adj = grid.adjacency();
        let n = grid.faces.len();
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, 24.0);
        let bed_m = vec![-3500.0_f32; n];
        let mount = vec![0.0_f32; n];
        let is_river = vec![false; n];
        let soil = vec![1.0_f32; n];
        let snow = vec![0.0_f32; n];

        let run_aquaplanet = |tilt_deg: f64, glaciation: f32| -> (f64, f64, f64) {
            let base_sst: Vec<f32> = (0..n)
                .map(|i| {
                    let amu =
                        orbital_annual_mean_cos_z(geom.sin_lat[i], geom.cos_lat[i], tilt_deg);
                    equilibrium_open_sst_c(amu, 1.0, 0.0)
                })
                .collect();
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
                    tilt_deg,
                    days_per_year: 365.25,
                    home_init_angle_rad: 0.0,
                    solar_tide_factor: 0.0,
                    solar_luminosity: 1.0,
                    glaciation,
                    atmosphere_id: 0,
                    moon_count: 0,
                    primary_mass_lunar: 0.0,
                    primary_dist_radii: 60.27,
                },
            );
            for _ in 0..48 {
                solver.step_hour(&geom);
            }
            let mut g_sum = 0.0_f64;
            let mut g_a = 0.0_f64;
            let mut eq_sum = 0.0_f64;
            let mut eq_a = 0.0_f64;
            let mut pol_sum = 0.0_f64;
            let mut pol_a = 0.0_f64;
            for i in 0..n {
                let a = geom.areas_m2[i] as f64;
                let t = solver.tsurf_c[i] as f64;
                g_sum += t * a;
                g_a += a;
                let abs_lat = geom.lats_rad[i].abs().to_degrees();
                if abs_lat < 15.0 {
                    eq_sum += t * a;
                    eq_a += a;
                } else if abs_lat > 70.0 {
                    pol_sum += t * a;
                    pol_a += a;
                }
            }
            (g_sum / g_a, eq_sum / eq_a, pol_sum / pol_a)
        };

        let (gm_ice, eq_ice, pol_ice) = run_aquaplanet(23.5, 0.0);
        let (gm_earth, _eq_earth, _pol_earth) = run_aquaplanet(23.5, 0.35);
        let (gm_green, eq_green, pol_green) = run_aquaplanet(23.5, 1.0);

        // Ice-free/ice-capped aquaplanet (no elevated Antarctica/continents) runs ~2 C warmer than
        // continental Earth, spanning ~8..11.5 C (icehouse) -> ~16..19 C (Earth) -> ~27..32 C (greenhouse):
        assert!(
            (7.5..=12.0).contains(&gm_ice)
                && (15.5..=19.0).contains(&gm_earth)
                && (27.0..=32.5).contains(&gm_green),
            "expected aquaplanet GMST to span icehouse..modern..greenhouse, got ice={gm_ice:.2}, earth={gm_earth:.2}, green={gm_green:.2}"
        );
        let d_trop = eq_green - eq_ice;
        let d_pole = pol_green - pol_ice;
        assert!(
            d_pole > d_trop * 1.25,
            "expected emergent sea-ice polar amplification at 23.5 deg tilt (d_pole={d_pole:.2} > 1.25 * d_trop={d_trop:.2})"
        );

        // At 75 deg obliquity, annual insolation is higher at the poles than at the equator,
        // so the equatorial belt must be cooler than the polar mean without any sin^2(lat) bias:
        let (_gm_hi, eq_hi, pol_hi) = run_aquaplanet(75.0, 0.0);
        assert!(
            pol_hi > eq_hi + 5.0,
            "expected 75 deg obliquity aquaplanet to have warmer poles ({pol_hi:.2} C) than equator ({eq_hi:.2} C)"
        );
    }
}
