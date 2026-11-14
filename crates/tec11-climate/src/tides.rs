use std::collections::VecDeque;
use std::f64::consts::PI;

use tec11_math_grid::{SphericalGrid, Vec3};

use crate::fv_geom::FiniteVolumeSphereGeom;

const RHO_SEAWATER: f32 = 1025.0;
const LOVE_GAMMA2: f64 = 0.69;
const SAL_BETA: f32 = 0.09;
const H_DRY_M: f32 = 0.05;
const H_WAVE_CAP_M: f32 = 4500.0;
const CD_BOTTOM: f32 = 0.0025;
const U_BG_MPS: f32 = 0.08;
const HISTORY_HOURS: usize = 48;
const RANGE_WINDOW_HOURS: usize = 28;

/// Ephemeris and tidal strength of an individual moon in `LaplaceTidalSolver`.
#[derive(Clone, Debug)]
pub struct TidalMoonBody {
    pub index: u32,
    pub mass_lunar: f32,
    pub dist_planet_radii: f32,
    /// Sidereal orbital period in local planetary days (matching `TileSkyMoon::period_days`).
    pub period_local_days: f32,
    pub inclination_deg: f32,
    pub init_angle_rad: f32,
    pub node_asc_rad: f32,
    /// Unreduced quadrupole amplitude $A_m$ (m) multiplying $\gamma_2 P_2(\cos\zeta)$.
    pub eq_amp_m: f64,
}

/// Optional hourly atmospheric surface pressure and wind-stress forcing for coupled storm surge.
#[derive(Clone, Copy, Debug)]
pub struct AtmosphericTideForcing<'a> {
    /// Surface pressure anomaly $\delta p_s$ (Pa) relative to standard reference pressure.
    pub p_anom_pa: &'a [f32],
    /// Eastward surface wind stress $\tau_x$ (Pa).
    pub tau_east_pa: &'a [f32],
    /// Northward surface wind stress $\tau_y$ (Pa).
    pub tau_north_pa: &'a [f32],
}

/// Global barotropic Laplace Tidal Equations (LTE) solver on an unstructured spherical C-grid,
/// supporting multi-moon astronomical tides, solar tides, self-attraction & loading (SAL),
/// topographic internal-wave drag, coastal wetting/drying inundation, and storm surge.
#[derive(Clone, Debug)]
pub struct LaplaceTidalSolver {
    num_cells: usize,
    unit_centers: Vec<Vec3>,
    bed_m: Vec<f32>,
    initial_ocean: Vec<bool>,
    edge_slope_sq: Vec<f32>,

    gravity_mps2: f32,
    day_hours: f64,
    tilt_rad: f64,
    days_per_year: f64,
    home_init_angle_rad: f64,
    solar_tide_factor: f64,
    start_day_of_year: u32,

    moon_count: u32,
    primary_mass_lunar: f32,
    primary_dist_radii: f32,
    moons: Vec<TidalMoonBody>,

    n_substeps_per_hour: usize,
    dt_sub_s: f32,
    elapsed_hours: u64,

    /// Total water column thickness $h_i \ge 0$ (m).
    pub h_total_m: Vec<f32>,
    /// Free-surface tidal/surge height anomaly $\eta_i$ (m) relative to MSL (`0.0` on dry land;
    /// equals $z_{b,i} + h_i$ on wet ocean and inundated coastal land).
    pub eta_m: Vec<f32>,
    /// Instantaneous astronomical equilibrium tide $\eta_{\text{eq},i}$ (m).
    pub eta_eq_m: Vec<f32>,
    /// Edge-normal barotropic velocity $u_e$ (m/s), positive `left -> right`.
    pub u_edge: Vec<f32>,
    /// Reconstructed cell-centered Eastward tidal velocity (m/s).
    pub u_east: Vec<f32>,
    /// Reconstructed cell-centered Northward tidal velocity (m/s).
    pub v_north: Vec<f32>,
    /// Cell-centered barotropic tidal current speed $|\mathbf{u}_i|$ (m/s).
    pub speed_mps: Vec<f32>,
    /// Bottom + internal-wave tidal energy dissipation rate ($\text{W m}^{-2}$).
    pub dissipation_wm2: Vec<f32>,
    /// Rolling ~28-hour peak-to-trough tidal range (m), revealing amphidromic nodes (`~0 m`)
    /// and resonant shelf/bay maxima.
    pub tidal_range_m: Vec<f32>,

    /// Rolling window of recent hourly `eta_m` snapshots (stored in millimeters as `i16`, `+-32.7 m`)
    /// for computing 28h `tidal_range_m` and instant 48h Tide Gauge sparklines on any clicked tile.
    eta_history_mm: VecDeque<Vec<i16>>,

    // Scratch buffers to avoid per-substep heap allocations.
    scratch_flux: Vec<f32>,
    scratch_outflow_vol: Vec<f32>,
    scratch_outflow_scale: Vec<f32>,
    scratch_u_next: Vec<f32>,
    scratch_edge_diss: Vec<f32>,
}

#[inline]
fn ecliptic_to_equatorial(lam_rad: f64, beta_rad: f64, eps_rad: f64) -> Vec3 {
    let cos_b = beta_rad.cos();
    let x_ecl = cos_b * lam_rad.cos();
    let y_ecl = cos_b * lam_rad.sin();
    let z_ecl = beta_rad.sin();
    let cos_e = eps_rad.cos();
    let sin_e = eps_rad.sin();
    [
        x_ecl,
        y_ecl * cos_e - z_ecl * sin_e,
        y_ecl * sin_e + z_ecl * cos_e,
    ]
}

#[inline]
fn equatorial_to_subpoint_unit_xyz(eq: Vec3, gmst_rad: f64) -> Vec3 {
    let sin_dec = eq[2].clamp(-1.0, 1.0);
    let cos_dec = (eq[0] * eq[0] + eq[1] * eq[1]).sqrt();
    let ra = eq[1].atan2(eq[0]);
    let lon_sub = ra - gmst_rad;
    [
        cos_dec * lon_sub.cos(),
        -cos_dec * lon_sub.sin(),
        sin_dec,
    ]
}

impl LaplaceTidalSolver {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        grid: &SphericalGrid,
        geom: &FiniteVolumeSphereGeom,
        bed_m: Vec<f32>,
        day_hours: f64,
        tilt_deg: f64,
        days_per_year: f64,
        home_init_angle_rad: f64,
        solar_tide_factor: f64,
        moon_count: u32,
        primary_mass_lunar: f32,
        primary_dist_radii: f32,
    ) -> Self {
        let n = geom.num_cells;
        let ne = geom.edges.len();
        debug_assert_eq!(bed_m.len(), n);

        let unit_centers: Vec<Vec3> = grid
            .faces
            .iter()
            .map(|&(p, _)| {
                let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-12);
                [p[0] / len, p[1] / len, p[2] / len]
            })
            .collect();

        let gravity_mps2 = (9.80665 * (geom.radius_m / 6_371_000.0)).clamp(3.5, 22.0) as f32;

        let mut min_edge_dist_m = f32::MAX;
        let mut edge_slope_sq = Vec::with_capacity(ne);
        for e in &geom.edges {
            min_edge_dist_m = min_edge_dist_m.min(e.center_dist_m);
            let dz = (bed_m[e.right as usize] - bed_m[e.left as usize]) * e.inv_center_dist_m;
            edge_slope_sq.push((dz * dz).min(0.04));
        }
        if !min_edge_dist_m.is_finite() || min_edge_dist_m < 500.0 {
            min_edge_dist_m = geom.mean_edge_dist_m.max(5_000.0);
        }

        let c_max = (gravity_mps2 * H_WAVE_CAP_M).sqrt();
        let dt_cfl = (0.48 * min_edge_dist_m / c_max.max(1.0)) as f64;
        let n_substeps_per_hour = ((3600.0 / dt_cfl.max(10.0)).ceil() as usize).clamp(12, 240);
        let dt_sub_s = (3600.0 / (n_substeps_per_hour as f64)) as f32;

        let initial_ocean: Vec<bool> = bed_m.iter().map(|&zb| zb < 0.0).collect();
        let h_total_m: Vec<f32> = bed_m.iter().map(|&zb| (-zb).max(0.0)).collect();

        let mut solver = Self {
            num_cells: n,
            unit_centers,
            bed_m,
            initial_ocean,
            edge_slope_sq,
            gravity_mps2,
            day_hours: day_hours.clamp(2.0, 240.0),
            tilt_rad: tilt_deg.clamp(0.0, 89.0) * PI / 180.0,
            days_per_year: days_per_year.clamp(8.0, 5000.0),
            home_init_angle_rad,
            solar_tide_factor: solar_tide_factor.max(0.0),
            start_day_of_year: 80,
            moon_count: 0,
            primary_mass_lunar: 1.0,
            primary_dist_radii: 60.27,
            moons: Vec::new(),
            n_substeps_per_hour,
            dt_sub_s,
            elapsed_hours: 0,
            h_total_m,
            eta_m: vec![0.0; n],
            eta_eq_m: vec![0.0; n],
            u_edge: vec![0.0; ne],
            u_east: vec![0.0; n],
            v_north: vec![0.0; n],
            speed_mps: vec![0.0; n],
            dissipation_wm2: vec![0.0; n],
            tidal_range_m: vec![0.0; n],
            eta_history_mm: VecDeque::with_capacity(HISTORY_HOURS + 1),
            scratch_flux: vec![0.0; ne],
            scratch_outflow_vol: vec![0.0; n],
            scratch_outflow_scale: vec![1.0; n],
            scratch_u_next: vec![0.0; ne],
            scratch_edge_diss: vec![0.0; ne],
        };
        solver.set_moon_params(moon_count, primary_mass_lunar, primary_dist_radii);
        solver.reset_state(geom);
        solver
    }

    /// Resets the ocean to a calm initial equilibrium sea surface (`eta = 0`, `u = 0`) at `t = 0`.
    pub fn reset_state(&mut self, geom: &FiniteVolumeSphereGeom) {
        self.elapsed_hours = 0;
        for i in 0..self.num_cells {
            self.h_total_m[i] = (-self.bed_m[i]).max(0.0);
            self.eta_m[i] = 0.0;
            self.u_east[i] = 0.0;
            self.v_north[i] = 0.0;
            self.speed_mps[i] = 0.0;
            self.dissipation_wm2[i] = 0.0;
            self.tidal_range_m[i] = 0.0;
        }
        self.u_edge.fill(0.0);
        self.evaluate_equilibrium_tide(geom, 0.0);
        self.eta_history_mm.clear();
        self.push_history_frame();
    }

    /// Configures `0..=6` moons in a Laplace $2^m:1$ resonance chain with Keplerian period scaling.
    pub fn set_moon_params(
        &mut self,
        count: u32,
        primary_mass_lunar: f32,
        primary_dist_radii: f32,
    ) {
        let count = count.min(6);
        let mass0 = primary_mass_lunar.clamp(0.0, 10.0);
        let dist0 = primary_dist_radii.clamp(15.0, 160.0);
        self.moon_count = count;
        self.primary_mass_lunar = mass0;
        self.primary_dist_radii = dist0;

        let spin = (24.0 / self.day_hours).max(0.1);
        let dist_ratio = (dist0 as f64) / 60.27;
        let base_period_earth_days = 27.32166 * dist_ratio.powf(1.5);
        let base_local_days = (base_period_earth_days * spin).max(1.2) as f32;

        // Unreduced Earth-Moon P2 amplitude: 0.358 m (giving gamma2 * A = 0.247 m sublunar height).
        let base_amp_earth_m = 0.358_f64;

        self.moons.clear();
        for m in 0..count {
            let period_mult = (1u32 << m) as f64;
            let dist_m = (dist0 as f64) * period_mult.powf(2.0 / 3.0);
            let mass_m = (mass0 as f64) * 0.85_f64.powi(m as i32);
            let eq_amp_m = base_amp_earth_m * mass_m * (60.27 / dist_m).powi(3);
            self.moons.push(TidalMoonBody {
                index: m,
                mass_lunar: mass_m as f32,
                dist_planet_radii: dist_m as f32,
                period_local_days: base_local_days * (period_mult as f32),
                inclination_deg: 5.14 + (m as f32) * 1.25,
                init_angle_rad: ((m as f32) * 1.854) % std::f32::consts::TAU,
                node_asc_rad: ((m as f32) * 2.399) % std::f32::consts::TAU,
                eq_amp_m,
            });
        }
    }

    pub fn set_day_of_year(&mut self, day: u32) {
        let max_day = (self.days_per_year.round() as u32).max(1);
        self.start_day_of_year = day % max_day;
    }

    pub fn set_solar_tide_factor(&mut self, factor: f64) {
        self.solar_tide_factor = factor.max(0.0);
    }

    pub fn moon_bodies(&self) -> &[TidalMoonBody] {
        &self.moons
    }

    pub fn moon_count(&self) -> u32 {
        self.moon_count
    }

    pub fn primary_mass_lunar(&self) -> f32 {
        self.primary_mass_lunar
    }

    pub fn primary_dist_radii(&self) -> f32 {
        self.primary_dist_radii
    }

    pub fn elapsed_hours(&self) -> u64 {
        self.elapsed_hours
    }

    pub fn substeps_per_hour(&self) -> usize {
        self.n_substeps_per_hour
    }

    pub fn gravity_mps2(&self) -> f32 {
        self.gravity_mps2
    }

    pub fn bed_m(&self) -> &[f32] {
        &self.bed_m
    }

    /// Current simulation `(year, day_in_year, utc_hour_frac)` synced with `SkyVantage`.
    pub fn current_sim_calendar(&self) -> (u32, u32, f64) {
        let days_per_yr = (self.days_per_year.round() as u64).max(1);
        // Start at 12:00 UTC (`utc_tau = 0.5`) at `t = 0` so Prime Meridian starts in daylight.
        let total_local_days =
            (self.start_day_of_year as f64) + 0.5 + (self.elapsed_hours as f64) / self.day_hours;
        let whole_days = total_local_days.floor().max(0.0) as u64;
        let year = (whole_days / days_per_yr) as u32;
        let day = (whole_days % days_per_yr) as u32;
        let utc_tau = total_local_days.rem_euclid(1.0);
        (year, day, utc_tau)
    }

    /// Instantaneous unit 3D vector `[x, y, z]` pointing toward the subsolar point on the globe.
    pub fn subsolar_unit_xyz(&self, elapsed_seconds_offset: f64) -> Vec3 {
        let total_days = (self.start_day_of_year as f64)
            + 0.5
            + ((self.elapsed_hours as f64) * 3600.0 + elapsed_seconds_offset)
                / (self.day_hours * 3600.0);
        let base_days = total_days.floor();
        let utc_tau = total_days - base_days;

        let lam_noon =
            self.home_init_angle_rad + 2.0 * PI * (base_days + 0.5) / self.days_per_year;
        let sun_noon_eq = ecliptic_to_equatorial(lam_noon, 0.0, self.tilt_rad);
        let ra_noon = sun_noon_eq[1].atan2(sun_noon_eq[0]);
        let gmst = ra_noon + 2.0 * PI * (utc_tau - 0.5);

        let lam_now = self.home_init_angle_rad + 2.0 * PI * total_days / self.days_per_year;
        let sun_now_eq = ecliptic_to_equatorial(lam_now, 0.0, self.tilt_rad);
        equatorial_to_subpoint_unit_xyz(sun_now_eq, gmst)
    }

    /// Evaluates the combined lunar + solar quadrupole equilibrium tide $\eta_{\text{eq},i}$ (m),
    /// removing the instantaneous wet-ocean area mean so total water volume is undisturbed.
    fn evaluate_equilibrium_tide(&mut self, geom: &FiniteVolumeSphereGeom, sim_seconds: f64) {
        let total_days =
            (self.start_day_of_year as f64) + 0.5 + sim_seconds / (self.day_hours * 3600.0);
        let base_days = total_days.floor();
        let utc_tau = total_days - base_days;

        let lam_noon =
            self.home_init_angle_rad + 2.0 * PI * (base_days + 0.5) / self.days_per_year;
        let sun_noon_eq = ecliptic_to_equatorial(lam_noon, 0.0, self.tilt_rad);
        let ra_noon = sun_noon_eq[1].atan2(sun_noon_eq[0]);
        let gmst = ra_noon + 2.0 * PI * (utc_tau - 0.5);

        let mut bodies: Vec<(Vec3, f64)> = Vec::with_capacity(self.moons.len() + 1);
        let radius_scale = self.geom_radius_scale(geom);

        if self.solar_tide_factor > 1e-6 {
            let lam_sun = self.home_init_angle_rad + 2.0 * PI * total_days / self.days_per_year;
            let sun_eq = ecliptic_to_equatorial(lam_sun, 0.0, self.tilt_rad);
            let r_sun = equatorial_to_subpoint_unit_xyz(sun_eq, gmst);
            let amp_sun = LOVE_GAMMA2 * 0.1648 * self.solar_tide_factor * radius_scale;
            bodies.push((r_sun, amp_sun));
        }

        for moon in &self.moons {
            if moon.eq_amp_m <= 1e-6 {
                continue;
            }
            let period = (moon.period_local_days as f64).max(1.0);
            let inc_rad = (moon.inclination_deg as f64) * PI / 180.0;
            let lam = (moon.init_angle_rad as f64) + 2.0 * PI * total_days / period;
            let node = (moon.node_asc_rad as f64) - 2.0 * PI * total_days / (period * 248.0);
            let beta = (inc_rad.sin() * (lam - node).sin()).clamp(-1.0, 1.0).asin();
            let moon_eq = ecliptic_to_equatorial(lam, beta, self.tilt_rad);
            let r_moon = equatorial_to_subpoint_unit_xyz(moon_eq, gmst);
            bodies.push((r_moon, LOVE_GAMMA2 * moon.eq_amp_m * radius_scale));
        }

        if bodies.is_empty() {
            self.eta_eq_m.fill(0.0);
            return;
        }

        // Smooth 4-hour sin^2 ramp from t = 0 so starting at rest excites clean Kelvin/Poincaré
        // waves without an unphysical step-function shockwave on hour 1.
        let ramp = if sim_seconds < 4.0 * 3600.0 {
            let s = ((sim_seconds / (4.0 * 3600.0)).clamp(0.0, 1.0) * 0.5 * PI).sin();
            s * s
        } else {
            1.0
        };

        let mut wet_sum = 0.0_f64;
        let mut wet_area = 0.0_f64;
        for i in 0..self.num_cells {
            let p = self.unit_centers[i];
            let mut val = 0.0_f64;
            for &(rb, amp) in &bodies {
                let cos_z = p[0] * rb[0] + p[1] * rb[1] + p[2] * rb[2];
                val += amp * 0.5 * (3.0 * cos_z * cos_z - 1.0);
            }
            val *= ramp;
            self.eta_eq_m[i] = val as f32;
            if self.h_total_m[i] > H_DRY_M {
                let a = geom.areas_m2[i] as f64;
                wet_sum += val * a;
                wet_area += a;
            }
        }

        if wet_area > 1.0 {
            let mean_eq = (wet_sum / wet_area) as f32;
            for v in &mut self.eta_eq_m {
                *v -= mean_eq;
            }
        }
    }

    #[inline]
    fn geom_radius_scale(&self, geom: &FiniteVolumeSphereGeom) -> f64 {
        (geom.radius_m / 6_371_000.0).clamp(0.25, 4.0)
    }

    /// Advances the global tidal ocean by `1 hour` (`n_substeps_per_hour` forward–backward Sielecki
    /// substeps), optionally forced by atmospheric pressure and wind stress (`forcing`).
    pub fn step_hour(
        &mut self,
        geom: &FiniteVolumeSphereGeom,
        forcing: Option<AtmosphericTideForcing<'_>>,
    ) {
        let base_seconds = (self.elapsed_hours as f64) * 3600.0;
        let dt = self.dt_sub_s;
        let g = self.gravity_mps2;
        let inv_rho = 1.0 / RHO_SEAWATER;

        // Mild abyssal radiative relaxation rate (`~16 h` timescale) accounting for unmodeled
        // baroclinic internal-tide radiation into the deep ocean so open-ocean M2 equilibrates cleanly.
        let r_rad = 1.7e-5_f32;
        let kappa_wave = 1.6e4_f32;

        self.dissipation_wm2.fill(0.0);
        let inv_sub = 1.0 / (self.n_substeps_per_hour as f32);

        for sub in 0..self.n_substeps_per_hour {
            let t_sec = base_seconds + ((sub as f64) + 0.5) * (dt as f64);
            self.evaluate_equilibrium_tide(geom, t_sec);
            geom.reconstruct_cell_vectors(&self.u_edge, &mut self.u_east, &mut self.v_north);

            // 1. Momentum update on C-grid edges (Audusse hydrostatic reconstruction + implicit drag).
            for (e_idx, e) in geom.edges.iter().enumerate() {
                let l = e.left as usize;
                let r = e.right as usize;
                let hl = self.h_total_m[l];
                let hr = self.h_total_m[r];

                if hl <= H_DRY_M && hr <= H_DRY_M {
                    self.scratch_u_next[e_idx] = 0.0;
                    self.scratch_edge_diss[e_idx] = 0.0;
                    continue;
                }

                let zbl = self.bed_m[l];
                let zbr = self.bed_m[r];
                let zetal = zbl + hl;
                let zetar = zbr + hr;
                let zb_star = zbl.max(zbr);
                let hl_star = (zetal - zb_star).max(0.0);
                let hr_star = (zetar - zb_star).max(0.0);

                if hl_star <= H_DRY_M && hr_star <= H_DRY_M {
                    self.scratch_u_next[e_idx] = 0.0;
                    self.scratch_edge_diss[e_idx] = 0.0;
                    continue;
                }

                let zeta_l_eff = zb_star + hl_star;
                let zeta_r_eff = zb_star + hr_star;
                let h_edge = 0.5 * (hl_star + hr_star);

                // Wave-speed depth cap `H_WAVE_CAP_M` scales barotropic pressure-gradient acceleration
                // in ultra-deep abyssal trenches (`> 4500 m`) so explicit substeps remain fast & stable.
                let wave_scale = if h_edge > H_WAVE_CAP_M {
                    H_WAVE_CAP_M / h_edge
                } else {
                    1.0
                };

                //SAL anomaly relative to resting sea level (`0` on ocean, `zb` on dry land):
                let eta_anom_l = zetal - zbl.max(0.0);
                let eta_anom_r = zetar - zbr.max(0.0);
                let sal_weight = (h_edge / 200.0).clamp(0.0, 1.0) * SAL_BETA;

                let d_zeta_dn = (zeta_r_eff - zeta_l_eff) * e.inv_center_dist_m;
                let d_sal_dn = (eta_anom_r - eta_anom_l) * e.inv_center_dist_m;
                let d_eq_dn = (self.eta_eq_m[r] - self.eta_eq_m[l]) * e.inv_center_dist_m;

                // Coriolis normal component: `f * (nx * v_north - ny * u_east)` averaged across left & right.
                let cor_l = geom.coriolis_f[l] * (e.nx_left * self.v_north[l] - e.ny_left * self.u_east[l]);
                let cor_r = geom.coriolis_f[r] * (e.nx_right * self.v_north[r] - e.ny_right * self.u_east[r]);
                let cor_acc = 0.5 * (cor_l + cor_r);

                let mut atm_acc = 0.0_f32;
                if let Some(ref f_atm) = forcing {
                    let dp_dn = (f_atm.p_anom_pa[r] - f_atm.p_anom_pa[l]) * e.inv_center_dist_m;
                    let tau_l = f_atm.tau_east_pa[l] * e.nx_left + f_atm.tau_north_pa[l] * e.ny_left;
                    let tau_r = f_atm.tau_east_pa[r] * e.nx_right + f_atm.tau_north_pa[r] * e.ny_right;
                    let tau_edge = 0.5 * (tau_l + tau_r);
                    // Shallow shelves amplify wind-driven storm surge (`tau / (rho * h)`).
                    atm_acc = -inv_rho * dp_dn + (tau_edge * inv_rho) / h_edge.max(2.5);
                }

                let accel = cor_acc
                    + wave_scale * (-g * d_zeta_dn + g * sal_weight * d_sal_dn + g * d_eq_dn + atm_acc);

                let u_old = self.u_edge[e_idx];
                let vt_l = e.nx_left * self.v_north[l] - e.ny_left * self.u_east[l];
                let vt_r = e.nx_right * self.v_north[r] - e.ny_right * self.u_east[r];
                let vt_avg = 0.5 * (vt_l + vt_r);
                let speed_mag = (u_old * u_old + vt_avg * vt_avg + U_BG_MPS * U_BG_MPS).sqrt();

                let r_bot = CD_BOTTOM * speed_mag / h_edge.max(1.5);
                let n_bv = 0.0025 * (h_edge / 300.0).clamp(0.0, 1.0);
                let r_wave = kappa_wave * n_bv * self.edge_slope_sq[e_idx] / h_edge.max(25.0);
                let r_tot = r_bot + r_wave + r_rad;

                let u_new = ((u_old + dt * accel) / (1.0 + dt * r_tot)).clamp(-12.0, 12.0);
                self.scratch_u_next[e_idx] = u_new;
                self.scratch_edge_diss[e_idx] =
                    RHO_SEAWATER * h_edge * (r_bot + r_wave) * u_new * u_new;
            }
            self.u_edge.copy_from_slice(&self.scratch_u_next);

            // 2. Upwind thickness flux with positivity-preserving outflow limiter (strict mass conservation).
            self.scratch_outflow_vol.fill(0.0);
            for (e_idx, e) in geom.edges.iter().enumerate() {
                let u = self.u_edge[e_idx];
                if u.abs() <= 1e-8 {
                    self.scratch_flux[e_idx] = 0.0;
                    continue;
                }
                let l = e.left as usize;
                let r = e.right as usize;
                let zb_star = self.bed_m[l].max(self.bed_m[r]);
                let h_up = if u > 0.0 {
                    (self.bed_m[l] + self.h_total_m[l] - zb_star).max(0.0).min(self.h_total_m[l])
                } else {
                    (self.bed_m[r] + self.h_total_m[r] - zb_star).max(0.0).min(self.h_total_m[r])
                };
                let q = h_up * u;
                self.scratch_flux[e_idx] = q;
                if q > 0.0 {
                    self.scratch_outflow_vol[l] += dt * e.dual_len_m * q;
                } else if q < 0.0 {
                    self.scratch_outflow_vol[r] += dt * e.dual_len_m * (-q);
                }
            }

            for i in 0..self.num_cells {
                let avail = 0.95 * self.h_total_m[i] * geom.areas_m2[i];
                let out_v = self.scratch_outflow_vol[i];
                self.scratch_outflow_scale[i] = if out_v > avail && out_v > 1e-9 {
                    (avail / out_v).clamp(0.0, 1.0)
                } else {
                    1.0
                };
            }

            for (e_idx, e) in geom.edges.iter().enumerate() {
                let q = self.scratch_flux[e_idx];
                if q > 0.0 {
                    self.scratch_flux[e_idx] = q * self.scratch_outflow_scale[e.left as usize];
                } else if q < 0.0 {
                    self.scratch_flux[e_idx] = q * self.scratch_outflow_scale[e.right as usize];
                }
            }

            // 3. Conservative thickness update & dissipation accumulation.
            for i in 0..self.num_cells {
                let mut div_hu = 0.0_f32;
                let mut diss_sum = 0.0_f32;
                let mut edge_cnt = 0.0_f32;
                for inc in geom.incident_edges(i) {
                    let e_idx = inc.edge_idx as usize;
                    div_hu += self.scratch_flux[e_idx] * inc.flux_factor;
                    diss_sum += self.scratch_edge_diss[e_idx];
                    edge_cnt += 1.0;
                }
                self.h_total_m[i] = (self.h_total_m[i] - dt * div_hu).max(0.0);
                if edge_cnt > 0.0 {
                    self.dissipation_wm2[i] += (diss_sum / edge_cnt) * inv_sub;
                }
            }
        }

        // Reconstruct final end-of-hour velocity, speed, free-surface anomaly `eta_m`, and 28h range.
        geom.reconstruct_cell_vectors(&self.u_edge, &mut self.u_east, &mut self.v_north);
        for i in 0..self.num_cells {
            let ux = self.u_east[i];
            let vy = self.v_north[i];
            self.speed_mps[i] = (ux * ux + vy * vy).sqrt();
            let h = self.h_total_m[i];
            self.eta_m[i] = if h > H_DRY_M {
                self.bed_m[i] + h
            } else {
                0.0
            };
        }

        self.elapsed_hours += 1;
        self.push_history_frame();
        self.recompute_rolling_tidal_range();
    }

    fn push_history_frame(&mut self) {
        if self.eta_history_mm.len() >= HISTORY_HOURS {
            self.eta_history_mm.pop_front();
        }
        let frame: Vec<i16> = self
            .eta_m
            .iter()
            .map(|&eta| (eta * 1000.0).round().clamp(-32_000.0, 32_000.0) as i16)
            .collect();
        self.eta_history_mm.push_back(frame);
    }

    fn recompute_rolling_tidal_range(&mut self) {
        let len = self.eta_history_mm.len();
        if len < 2 {
            self.tidal_range_m.fill(0.0);
            return;
        }
        let skip = len.saturating_sub(RANGE_WINDOW_HOURS);
        let mut mins = vec![i16::MAX; self.num_cells];
        let mut maxs = vec![i16::MIN; self.num_cells];
        for frame in self.eta_history_mm.iter().skip(skip) {
            for i in 0..self.num_cells {
                let v = frame[i];
                if v < mins[i] {
                    mins[i] = v;
                }
                if v > maxs[i] {
                    maxs[i] = v;
                }
            }
        }
        for i in 0..self.num_cells {
            if self.h_total_m[i] > H_DRY_M || self.initial_ocean[i] {
                self.tidal_range_m[i] = ((maxs[i] as i32 - mins[i] as i32).max(0) as f32) * 0.001;
            } else {
                self.tidal_range_m[i] = 0.0;
            }
        }
    }

    /// Returns up to `48` hourly free-surface height samples (m) for `cell` (chronological order).
    pub fn cell_eta_series_m(&self, cell: usize) -> Vec<f32> {
        if cell >= self.num_cells {
            return Vec::new();
        }
        self.eta_history_mm
            .iter()
            .map(|f| (f[cell] as f32) * 0.001)
            .collect()
    }

    /// Returns `true` when `cell` was originally dry land (`bed_m >= 0`) and is currently inundated
    /// by high tide or storm surge (`h_total_m > H_DRY_M`).
    #[inline]
    pub fn is_inundated_land(&self, cell: usize) -> bool {
        !self.initial_ocean[cell] && self.h_total_m[cell] > H_DRY_M
    }

    /// Counts how many originally-land fine cells are currently inundated by tide/surge.
    pub fn inundated_land_count(&self) -> u32 {
        (0..self.num_cells)
            .filter(|&i| self.is_inundated_land(i))
            .count() as u32
    }

    /// Total global ocean water volume ($\text{m}^3$) across all cells.
    pub fn total_water_volume_m3(&self, geom: &FiniteVolumeSphereGeom) -> f64 {
        self.h_total_m
            .iter()
            .zip(geom.areas_m2.iter())
            .map(|(&h, &a)| (h as f64) * (a as f64))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_ocean(size: u32) -> (SphericalGrid, FiniteVolumeSphereGeom, Vec<f32>) {
        let grid = SphericalGrid::with_size(size);
        let adj = grid.adjacency();
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, 6_371_000.0, 24.0);
        // Continent centered around (+X, +Z) plus coastal shelf (`-80 m` to `+2 m`) and deep basin (`-3600 m`).
        let bed_m: Vec<f32> = grid
            .faces
            .iter()
            .map(|&(p, _)| {
                let d = p[0] * 0.8 + p[2] * 0.6;
                if d > 0.52 {
                    120.0 * ((d - 0.52) as f32) + 0.8
                } else if d > 0.36 {
                    // Gently sloping continental shelf (-150 m .. -5 m)
                    -5.0 - 900.0 * ((0.52 - d) as f32)
                } else {
                    -3600.0
                }
            })
            .collect();
        (grid, geom, bed_m)
    }

    #[test]
    fn test_zero_forcing_lake_at_rest_and_mass_conservation() {
        let (grid, geom, bed_m) = make_test_ocean(4);
        let mut solver = LaplaceTidalSolver::new(
            &grid, &geom, bed_m, 24.0, 23.44, 365.25, 0.0, 0.0, 0, 0.0, 60.27,
        );
        let v0 = solver.total_water_volume_m3(&geom);
        for _ in 0..12 {
            solver.step_hour(&geom, None);
        }
        let max_eta = solver
            .eta_m
            .iter()
            .fold(0.0_f32, |acc, &v| acc.max(v.abs()));
        assert!(
            max_eta < 1e-6,
            "zero moons + zero sun lake-at-rest drifted: max |eta| = {max_eta}"
        );
        let v1 = solver.total_water_volume_m3(&geom);
        assert!(((v1 - v0) / v0).abs() < 1e-6);
    }

    #[test]
    fn test_dynamic_tides_volume_conservation_positivity_and_moon_mass_scaling() {
        let (grid, geom, bed_m) = make_test_ocean(4);
        let mut solver_1x = LaplaceTidalSolver::new(
            &grid,
            &geom,
            bed_m.clone(),
            24.0,
            23.44,
            365.25,
            0.0,
            1.0,
            1,
            1.0,
            60.27,
        );
        let v0 = solver_1x.total_water_volume_m3(&geom);
        for _ in 0..24 {
            solver_1x.step_hour(&geom, None);
        }
        let v24 = solver_1x.total_water_volume_m3(&geom);
        let rel_vol_err = ((v24 - v0) / v0).abs();
        assert!(
            rel_vol_err < 1e-5,
            "24h tidal integration failed mass conservation: rel_vol_err={rel_vol_err}"
        );
        assert!(solver_1x.h_total_m.iter().all(|&h| h.is_finite() && h >= 0.0));

        let max_range_1x = solver_1x
            .tidal_range_m
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        assert!(
            max_range_1x > 0.25 && max_range_1x < 12.0,
            "expected physical 1x lunar tidal range, got {max_range_1x} m"
        );

        let mut solver_2x = LaplaceTidalSolver::new(
            &grid, &geom, bed_m, 24.0, 23.44, 365.25, 0.0, 0.0, 1, 2.0, 60.27,
        );
        let mut solver_base = LaplaceTidalSolver::new(
            &grid,
            &geom,
            solver_1x.bed_m().to_vec(),
            24.0,
            23.44,
            365.25,
            0.0,
            0.0,
            1,
            1.0,
            60.27,
        );
        for _ in 0..24 {
            solver_base.step_hour(&geom, None);
            solver_2x.step_hour(&geom, None);
        }
        let r1 = solver_base
            .tidal_range_m
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        let r2 = solver_2x
            .tidal_range_m
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        let ratio = r2 / r1.max(1e-5);
        assert!(
            (1.65..=2.25).contains(&ratio),
            "expected ~2x tidal range when doubling moon mass, got r1={r1}, r2={r2}, ratio={ratio}"
        );
    }
}
