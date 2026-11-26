use std::collections::VecDeque;
use std::f64::consts::PI;

use tec11_math_grid::{FiniteVolumeSphereGeom, SphericalGrid, Vec3};

const RHO_SEAWATER: f32 = 1025.0;
const LOVE_GAMMA2: f64 = 0.69;
const SAL_BETA: f32 = 0.09;
const H_DRY_M: f32 = 0.05;
const H_WAVE_CAP_M: f32 = 4500.0;
const CD_BOTTOM: f32 = 0.0025;
const U_BG_MPS: f32 = 0.08;
const HISTORY_HOURS: usize = 48;
const RANGE_WINDOW_HOURS: usize = 28;
const BASE_INUNDATION_CEIL_M: f32 = 55.0;

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
    unit_centers: Vec<[f32; 3]>,
    bed_m: Vec<f32>,
    initial_ocean: Vec<bool>,
    edge_slope_sq: Vec<f32>,
    cell_inv_deg: Vec<f32>,

    inundation_ceil_m: f32,
    active_domain_dirty: bool,
    active_cells: Vec<u32>,
    active_edges: Vec<u32>,
    eta_eq_nonzero: bool,
    astro_dirty: bool,

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

    // Scratch buffers to avoid per-substep and per-hour heap allocations.
    scratch_flux: Vec<f32>,
    scratch_outflow_vol: Vec<f32>,
    scratch_outflow_scale: Vec<f32>,
    scratch_edge_diss: Vec<f32>,
    scratch_atm_dp_acc: Vec<f32>,
    scratch_atm_tau_acc: Vec<f32>,
    scratch_range_min: Vec<i16>,
    scratch_range_max: Vec<i16>,
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

#[inline]
fn reconstruct_active_vectors(
    geom: &FiniteVolumeSphereGeom,
    active_cells: &[u32],
    u_edge: &[f32],
    out_u_east: &mut [f32],
    out_v_north: &mut [f32],
) {
    if geom.all_deg_le_6 {
        for &c_u32 in active_cells {
            let c = c_u32 as usize;
            let st = &geom.cell_perot6[c];
            let mut ux = 0.0_f32;
            let mut vy = 0.0_f32;
            for k in 0..6 {
                let ue = u_edge[st.edge_idx[k] as usize];
                ux += ue * st.perot_wx[k];
                vy += ue * st.perot_wy[k];
            }
            out_u_east[c] = ux;
            out_v_north[c] = vy;
        }
    } else {
        geom.reconstruct_cell_vectors(u_edge, out_u_east, out_v_north);
    }
}

#[inline]
fn advance_active_thickness(
    geom: &FiniteVolumeSphereGeom,
    active_cells: &[u32],
    active_edges: &[u32],
    dt: f32,
    bed_m: &[f32],
    u_edge: &[f32],
    h_total_m: &mut [f32],
    scratch_flux: &mut [f32],
    scratch_outflow_vol: &mut [f32],
    scratch_outflow_scale: &mut [f32],
) {
    for &c_u32 in active_cells {
        scratch_outflow_vol[c_u32 as usize] = 0.0;
    }

    for &e_u32 in active_edges {
        let e_idx = e_u32 as usize;
        let u = u_edge[e_idx];
        if u.abs() <= 1e-8 {
            scratch_flux[e_idx] = 0.0;
            continue;
        }
        let e = &geom.edges[e_idx];
        let l = e.left as usize;
        let r = e.right as usize;
        let zb_star = bed_m[l].max(bed_m[r]);
        let h_up = if u > 0.0 {
            (bed_m[l] + h_total_m[l] - zb_star)
                .max(0.0)
                .min(h_total_m[l])
        } else {
            (bed_m[r] + h_total_m[r] - zb_star)
                .max(0.0)
                .min(h_total_m[r])
        };
        let q = h_up * u;
        scratch_flux[e_idx] = q;
        if q > 0.0 {
            scratch_outflow_vol[l] += dt * e.dual_len_m * q;
        } else if q < 0.0 {
            scratch_outflow_vol[r] += dt * e.dual_len_m * (-q);
        }
    }

    let mut any_limited = false;
    for &c_u32 in active_cells {
        let i = c_u32 as usize;
        let avail = 0.95 * h_total_m[i] * geom.areas_m2[i];
        let out_v = scratch_outflow_vol[i];
        if out_v > avail && out_v > 1e-9 {
            scratch_outflow_scale[i] = (avail / out_v).clamp(0.0, 1.0);
            any_limited = true;
        } else {
            scratch_outflow_scale[i] = 1.0;
        }
    }

    if any_limited {
        for &e_u32 in active_edges {
            let e_idx = e_u32 as usize;
            let q = scratch_flux[e_idx];
            if q > 0.0 {
                scratch_flux[e_idx] = q * scratch_outflow_scale[geom.edges[e_idx].left as usize];
            } else if q < 0.0 {
                scratch_flux[e_idx] = q * scratch_outflow_scale[geom.edges[e_idx].right as usize];
            }
        }
    }

    if geom.all_deg_le_6 {
        for &c_u32 in active_cells {
            let i = c_u32 as usize;
            let st = &geom.cell_div6[i];
            let mut div_hu = 0.0_f32;
            for k in 0..6 {
                div_hu += scratch_flux[st.edge_idx[k] as usize] * st.flux_factor[k];
            }
            h_total_m[i] = (h_total_m[i] - dt * div_hu).max(0.0);
        }
    } else {
        for &c_u32 in active_cells {
            let i = c_u32 as usize;
            let mut div_hu = 0.0_f32;
            for inc in geom.incident_edges(i) {
                div_hu += scratch_flux[inc.edge_idx as usize] * inc.flux_factor;
            }
            h_total_m[i] = (h_total_m[i] - dt * div_hu).max(0.0);
        }
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn advance_active_thickness_paired(
    geom: &FiniteVolumeSphereGeom,
    active_cells: &[u32],
    active_edges: &[u32],
    dt: f32,
    bed_m: &[f32],
    u_edge_a: &[f32],
    h_total_a: &mut [f32],
    scratch_flux_a: &mut [f32],
    scratch_out_vol_a: &mut [f32],
    scratch_out_scale_a: &mut [f32],
    u_edge_b: &[f32],
    h_total_b: &mut [f32],
    scratch_flux_b: &mut [f32],
    scratch_out_vol_b: &mut [f32],
    scratch_out_scale_b: &mut [f32],
) {
    for &c_u32 in active_cells {
        let c = c_u32 as usize;
        scratch_out_vol_a[c] = 0.0;
        scratch_out_vol_b[c] = 0.0;
    }

    for &e_u32 in active_edges {
        let e_idx = e_u32 as usize;
        let ua = u_edge_a[e_idx];
        let ub = u_edge_b[e_idx];
        if ua.abs() <= 1e-8 && ub.abs() <= 1e-8 {
            scratch_flux_a[e_idx] = 0.0;
            scratch_flux_b[e_idx] = 0.0;
            continue;
        }
        let e = &geom.edges[e_idx];
        let l = e.left as usize;
        let r = e.right as usize;
        let zbl = bed_m[l];
        let zbr = bed_m[r];
        let zb_star = zbl.max(zbr);
        let dt_len = dt * e.dual_len_m;

        if ua.abs() <= 1e-8 {
            scratch_flux_a[e_idx] = 0.0;
        } else {
            let h_up_a = if ua > 0.0 {
                (zbl + h_total_a[l] - zb_star).max(0.0).min(h_total_a[l])
            } else {
                (zbr + h_total_a[r] - zb_star).max(0.0).min(h_total_a[r])
            };
            let qa = h_up_a * ua;
            scratch_flux_a[e_idx] = qa;
            if qa > 0.0 {
                scratch_out_vol_a[l] += dt_len * qa;
            } else if qa < 0.0 {
                scratch_out_vol_a[r] += dt_len * (-qa);
            }
        }

        if ub.abs() <= 1e-8 {
            scratch_flux_b[e_idx] = 0.0;
        } else {
            let h_up_b = if ub > 0.0 {
                (zbl + h_total_b[l] - zb_star).max(0.0).min(h_total_b[l])
            } else {
                (zbr + h_total_b[r] - zb_star).max(0.0).min(h_total_b[r])
            };
            let qb = h_up_b * ub;
            scratch_flux_b[e_idx] = qb;
            if qb > 0.0 {
                scratch_out_vol_b[l] += dt_len * qb;
            } else if qb < 0.0 {
                scratch_out_vol_b[r] += dt_len * (-qb);
            }
        }
    }

    let mut lim_a = false;
    let mut lim_b = false;
    for &c_u32 in active_cells {
        let i = c_u32 as usize;
        let area95 = 0.95 * geom.areas_m2[i];
        let avail_a = area95 * h_total_a[i];
        let out_a = scratch_out_vol_a[i];
        if out_a > avail_a && out_a > 1e-9 {
            scratch_out_scale_a[i] = (avail_a / out_a).clamp(0.0, 1.0);
            lim_a = true;
        } else {
            scratch_out_scale_a[i] = 1.0;
        }

        let avail_b = area95 * h_total_b[i];
        let out_b = scratch_out_vol_b[i];
        if out_b > avail_b && out_b > 1e-9 {
            scratch_out_scale_b[i] = (avail_b / out_b).clamp(0.0, 1.0);
            lim_b = true;
        } else {
            scratch_out_scale_b[i] = 1.0;
        }
    }

    if lim_a || lim_b {
        for &e_u32 in active_edges {
            let e_idx = e_u32 as usize;
            let e = &geom.edges[e_idx];
            if lim_a {
                let qa = scratch_flux_a[e_idx];
                if qa > 0.0 {
                    scratch_flux_a[e_idx] = qa * scratch_out_scale_a[e.left as usize];
                } else if qa < 0.0 {
                    scratch_flux_a[e_idx] = qa * scratch_out_scale_a[e.right as usize];
                }
            }
            if lim_b {
                let qb = scratch_flux_b[e_idx];
                if qb > 0.0 {
                    scratch_flux_b[e_idx] = qb * scratch_out_scale_b[e.left as usize];
                } else if qb < 0.0 {
                    scratch_flux_b[e_idx] = qb * scratch_out_scale_b[e.right as usize];
                }
            }
        }
    }

    if geom.all_deg_le_6 {
        for &c_u32 in active_cells {
            let i = c_u32 as usize;
            let st = &geom.cell_div6[i];
            let mut div_a = 0.0_f32;
            let mut div_b = 0.0_f32;
            for k in 0..6 {
                let e = st.edge_idx[k] as usize;
                let ff = st.flux_factor[k];
                div_a += scratch_flux_a[e] * ff;
                div_b += scratch_flux_b[e] * ff;
            }
            h_total_a[i] = (h_total_a[i] - dt * div_a).max(0.0);
            h_total_b[i] = (h_total_b[i] - dt * div_b).max(0.0);
        }
    } else {
        for &c_u32 in active_cells {
            let i = c_u32 as usize;
            let mut div_a = 0.0_f32;
            let mut div_b = 0.0_f32;
            for inc in geom.incident_edges(i) {
                let e = inc.edge_idx as usize;
                div_a += scratch_flux_a[e] * inc.flux_factor;
                div_b += scratch_flux_b[e] * inc.flux_factor;
            }
            h_total_a[i] = (h_total_a[i] - dt * div_a).max(0.0);
            h_total_b[i] = (h_total_b[i] - dt * div_b).max(0.0);
        }
    }
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

        let cell_inv_deg: Vec<f32> = (0..n)
            .map(|i| {
                let deg = geom.incident_edges(i).len();
                if deg > 0 {
                    1.0 / (deg as f32)
                } else {
                    0.0
                }
            })
            .collect();

        let c_max = (gravity_mps2 * H_WAVE_CAP_M).sqrt();
        // Forward-backward Sielecki C-grid integration on a hexagonal mesh has stability limit
        // `c_max * dt / d_min <= 2 / sqrt(6) = 0.8165` (`0.68` gives >16% margin even on pentagons):
        let dt_cfl = (0.68 * min_edge_dist_m / c_max.max(1.0)) as f64;
        let n_substeps_per_hour = ((3600.0 / dt_cfl.max(10.0)).ceil() as usize).clamp(6, 240);
        let dt_sub_s = (3600.0 / (n_substeps_per_hour as f64)) as f32;

        let initial_ocean: Vec<bool> = bed_m.iter().map(|&zb| zb < 0.0).collect();
        let h_total_m: Vec<f32> = bed_m.iter().map(|&zb| (-zb).max(0.0)).collect();

        let mut solver = Self {
            num_cells: n,
            unit_centers,
            bed_m,
            initial_ocean,
            edge_slope_sq,
            cell_inv_deg,
            inundation_ceil_m: BASE_INUNDATION_CEIL_M,
            active_domain_dirty: true,
            active_cells: Vec::with_capacity(n),
            active_edges: Vec::with_capacity(ne),
            eta_eq_nonzero: false,
            astro_dirty: false,
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
            scratch_edge_diss: vec![0.0; ne],
            scratch_atm_dp_acc: vec![0.0; ne],
            scratch_atm_tau_acc: vec![0.0; ne],
            scratch_range_min: vec![0; n],
            scratch_range_max: vec![0; n],
        };
        solver.set_moon_params(moon_count, primary_mass_lunar, primary_dist_radii);
        solver.reset_state(geom);
        solver
    }

    fn target_inundation_ceil_m(&self) -> f32 {
        let eq_sum: f64 = self.moons.iter().map(|m| m.eq_amp_m).sum();
        (BASE_INUNDATION_CEIL_M + 4.5 * (eq_sum as f32)).max(BASE_INUNDATION_CEIL_M)
    }

    fn ensure_active_domain(&mut self, geom: &FiniteVolumeSphereGeom) {
        if !self.active_domain_dirty {
            return;
        }
        let ceil = self.inundation_ceil_m.max(self.target_inundation_ceil_m());
        self.inundation_ceil_m = ceil;
        self.active_cells.clear();
        for &c_u32 in &geom.morton_cells {
            let c = c_u32 as usize;
            if self.bed_m[c] <= ceil || self.h_total_m[c] > 0.0 {
                self.active_cells.push(c_u32);
            }
        }
        self.active_edges.clear();
        for (e_idx, e) in geom.edges.iter().enumerate() {
            let l = e.left as usize;
            let r = e.right as usize;
            if self.bed_m[l].max(self.bed_m[r]) <= ceil
                || self.h_total_m[l] > 0.0
                || self.h_total_m[r] > 0.0
            {
                self.active_edges.push(e_idx as u32);
            }
        }
        self.active_domain_dirty = false;
    }

    #[inline]
    pub fn has_astro_forcing(&self) -> bool {
        self.solar_tide_factor > 1e-6 || self.moons.iter().any(|m| m.eq_amp_m > 1e-6)
    }

    /// Resets the ocean to a calm initial equilibrium sea surface (`eta = 0`, `u = 0`) at `t = 0`.
    pub fn reset_state(&mut self, geom: &FiniteVolumeSphereGeom) {
        self.elapsed_hours = 0;
        self.inundation_ceil_m = self.target_inundation_ceil_m();
        self.active_domain_dirty = true;
        self.astro_dirty = false;
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
        self.scratch_flux.fill(0.0);
        self.scratch_edge_diss.fill(0.0);
        self.eta_eq_m.fill(0.0);
        self.eta_eq_nonzero = false;
        self.ensure_active_domain(geom);
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
        if self.target_inundation_ceil_m() > self.inundation_ceil_m {
            self.active_domain_dirty = true;
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

    /// Computes stack-allocated `([rx, ry, rz], ramped_amp_m)` quadrupole forcing bodies at `sim_seconds`.
    fn compute_astro_bodies(
        &self,
        geom: &FiniteVolumeSphereGeom,
        sim_seconds: f64,
    ) -> ([([f32; 3], f32); 7], usize) {
        let mut bodies = [([0.0_f32; 3], 0.0_f32); 7];
        let mut n_bodies = 0usize;
        if !self.has_astro_forcing() {
            return (bodies, 0);
        }

        // Smooth 4-hour sin^2 ramp from t = 0 so starting at rest excites clean Kelvin/Poincaré
        // waves without an unphysical step-function shockwave on hour 1.
        let ramp = if sim_seconds < 4.0 * 3600.0 {
            let s = ((sim_seconds / (4.0 * 3600.0)).clamp(0.0, 1.0) * 0.5 * PI).sin();
            s * s
        } else {
            1.0
        };
        if ramp <= 0.0 {
            return (bodies, 0);
        }

        let total_days =
            (self.start_day_of_year as f64) + 0.5 + sim_seconds / (self.day_hours * 3600.0);
        let base_days = total_days.floor();
        let utc_tau = total_days - base_days;

        let lam_noon =
            self.home_init_angle_rad + 2.0 * PI * (base_days + 0.5) / self.days_per_year;
        let sun_noon_eq = ecliptic_to_equatorial(lam_noon, 0.0, self.tilt_rad);
        let ra_noon = sun_noon_eq[1].atan2(sun_noon_eq[0]);
        let gmst = ra_noon + 2.0 * PI * (utc_tau - 0.5);
        let radius_scale = self.geom_radius_scale(geom);

        if self.solar_tide_factor > 1e-6 {
            let lam_sun = self.home_init_angle_rad + 2.0 * PI * total_days / self.days_per_year;
            let sun_eq = ecliptic_to_equatorial(lam_sun, 0.0, self.tilt_rad);
            let r_sun = equatorial_to_subpoint_unit_xyz(sun_eq, gmst);
            let amp_sun = LOVE_GAMMA2 * 0.1648 * self.solar_tide_factor * radius_scale * ramp;
            bodies[n_bodies] = (
                [r_sun[0] as f32, r_sun[1] as f32, r_sun[2] as f32],
                amp_sun as f32,
            );
            n_bodies += 1;
        }

        for moon in &self.moons {
            if moon.eq_amp_m <= 1e-6 || n_bodies >= 7 {
                continue;
            }
            let period = (moon.period_local_days as f64).max(1.0);
            let inc_rad = (moon.inclination_deg as f64) * PI / 180.0;
            let lam = (moon.init_angle_rad as f64) + 2.0 * PI * total_days / period;
            let node = (moon.node_asc_rad as f64) - 2.0 * PI * total_days / (period * 248.0);
            let beta = (inc_rad.sin() * (lam - node).sin()).clamp(-1.0, 1.0).asin();
            let moon_eq = ecliptic_to_equatorial(lam, beta, self.tilt_rad);
            let r_moon = equatorial_to_subpoint_unit_xyz(moon_eq, gmst);
            let amp_moon = LOVE_GAMMA2 * moon.eq_amp_m * radius_scale * ramp;
            bodies[n_bodies] = (
                [r_moon[0] as f32, r_moon[1] as f32, r_moon[2] as f32],
                amp_moon as f32,
            );
            n_bodies += 1;
        }

        (bodies, n_bodies)
    }

    /// Evaluates the combined lunar + solar quadrupole equilibrium tide $\eta_{\text{eq},i}$ (m),
    /// removing the instantaneous wet-ocean area mean so total water volume is undisturbed.
    fn evaluate_equilibrium_tide(&mut self, geom: &FiniteVolumeSphereGeom, sim_seconds: f64) {
        let (bodies, n_bodies) = self.compute_astro_bodies(geom, sim_seconds);
        if n_bodies == 0 {
            if self.eta_eq_nonzero {
                for &c_u32 in &self.active_cells {
                    self.eta_eq_m[c_u32 as usize] = 0.0;
                }
                self.eta_eq_nonzero = false;
            }
            return;
        }
        self.eta_eq_nonzero = true;
        let body_slice = &bodies[..n_bodies];

        let mut wet_sum = 0.0_f64;
        let mut wet_area = 0.0_f64;
        for &c_u32 in &self.active_cells {
            let i = c_u32 as usize;
            let p = self.unit_centers[i];
            let mut val = 0.0_f32;
            for &(rb, amp) in body_slice {
                let cos_z = p[0] * rb[0] + p[1] * rb[1] + p[2] * rb[2];
                val += amp * 0.5 * (3.0 * cos_z * cos_z - 1.0);
            }
            self.eta_eq_m[i] = val;
            if self.h_total_m[i] > H_DRY_M {
                let a = geom.areas_m2[i] as f64;
                wet_sum += (val as f64) * a;
                wet_area += a;
            }
        }

        if wet_area > 1.0 {
            let mean_eq = (wet_sum / wet_area) as f32;
            for &c_u32 in &self.active_cells {
                self.eta_eq_m[c_u32 as usize] -= mean_eq;
            }
        }
    }

    #[inline]
    fn geom_radius_scale(&self, geom: &FiniteVolumeSphereGeom) -> f64 {
        (geom.radius_m / 6_371_000.0).clamp(0.25, 4.0)
    }

    fn precompute_hourly_atm_edge_forcing(
        &mut self,
        geom: &FiniteVolumeSphereGeom,
        forcing: AtmosphericTideForcing<'_>,
    ) {
        let inv_rho = 1.0 / RHO_SEAWATER;
        for &e_u32 in &self.active_edges {
            let e_idx = e_u32 as usize;
            let e = &geom.edges[e_idx];
            let l = e.left as usize;
            let r = e.right as usize;
            let dp_dn = (forcing.p_anom_pa[r] - forcing.p_anom_pa[l]) * e.inv_center_dist_m;
            let tau_l = forcing.tau_east_pa[l] * e.nx_left + forcing.tau_north_pa[l] * e.ny_left;
            let tau_r = forcing.tau_east_pa[r] * e.nx_right + forcing.tau_north_pa[r] * e.ny_right;
            self.scratch_atm_dp_acc[e_idx] = -inv_rho * dp_dn;
            self.scratch_atm_tau_acc[e_idx] = inv_rho * 0.5 * (tau_l + tau_r);
        }
    }

    fn finalize_hour(&mut self, geom: &FiniteVolumeSphereGeom) {
        reconstruct_active_vectors(
            geom,
            &self.active_cells,
            &self.u_edge,
            &mut self.u_east,
            &mut self.v_north,
        );

        let inv_sub = 1.0 / (self.n_substeps_per_hour as f32);
        if geom.all_deg_le_6 {
            for &c_u32 in &self.active_cells {
                let i = c_u32 as usize;
                let ux = self.u_east[i];
                let vy = self.v_north[i];
                self.speed_mps[i] = (ux * ux + vy * vy).sqrt();
                let h = self.h_total_m[i];
                self.eta_m[i] = if h > H_DRY_M {
                    self.bed_m[i] + h
                } else {
                    0.0
                };
                let st = &geom.cell_div6[i];
                let inv_deg = self.cell_inv_deg[i];
                // Note for pentagons, slot [5] repeats slot [0], so sum only real degree via CSR or mask:
                let start = geom.cell_edge_offsets[i];
                let end = geom.cell_edge_offsets[i + 1];
                let deg = end - start;
                let mut diss_sum = 0.0_f32;
                for k in 0..deg {
                    diss_sum += self.scratch_edge_diss[st.edge_idx[k] as usize];
                }
                self.dissipation_wm2[i] = diss_sum * inv_deg * inv_sub;
            }
        } else {
            for &c_u32 in &self.active_cells {
                let i = c_u32 as usize;
                let ux = self.u_east[i];
                let vy = self.v_north[i];
                self.speed_mps[i] = (ux * ux + vy * vy).sqrt();
                let h = self.h_total_m[i];
                self.eta_m[i] = if h > H_DRY_M {
                    self.bed_m[i] + h
                } else {
                    0.0
                };
                let mut diss_sum = 0.0_f32;
                for inc in geom.incident_edges(i) {
                    diss_sum += self.scratch_edge_diss[inc.edge_idx as usize];
                }
                self.dissipation_wm2[i] = diss_sum * self.cell_inv_deg[i] * inv_sub;
            }
        }

        self.elapsed_hours += 1;
        self.push_history_frame();
        self.recompute_rolling_tidal_range();
    }

    /// Advances the global tidal ocean by `1 hour` (`n_substeps_per_hour` forward–backward Sielecki
    /// substeps), optionally forced by atmospheric pressure and wind stress (`forcing`).
    pub fn step_hour(
        &mut self,
        geom: &FiniteVolumeSphereGeom,
        forcing: Option<AtmosphericTideForcing<'_>>,
    ) {
        self.ensure_active_domain(geom);
        let base_seconds = (self.elapsed_hours as f64) * 3600.0;
        let dt = self.dt_sub_s;
        let g = self.gravity_mps2;
        let r_rad = 1.7e-5_f32;
        let kappa_wave = 1.6e4_f32;

        let has_atm = if let Some(f_atm) = forcing {
            self.precompute_hourly_atm_edge_forcing(geom, f_atm);
            true
        } else {
            false
        };

        for &e_u32 in &self.active_edges {
            self.scratch_edge_diss[e_u32 as usize] = 0.0;
        }
        if self.has_astro_forcing() {
            self.astro_dirty = true;
        }

        for sub in 0..self.n_substeps_per_hour {
            if sub % 4 == 0 {
                let block = (self.n_substeps_per_hour - sub).min(4) as f64;
                let t_sec = base_seconds + ((sub as f64) + 0.5 * block) * (dt as f64);
                self.evaluate_equilibrium_tide(geom, t_sec);
            }
            reconstruct_active_vectors(
                geom,
                &self.active_cells,
                &self.u_edge,
                &mut self.u_east,
                &mut self.v_north,
            );

            // 1. Momentum update on active C-grid edges (Audusse hydrostatic reconstruction + implicit drag).
            for &e_u32 in &self.active_edges {
                let e_idx = e_u32 as usize;
                let e = &geom.edges[e_idx];
                let l = e.left as usize;
                let r = e.right as usize;
                let hl = self.h_total_m[l];
                let hr = self.h_total_m[r];

                if hl <= H_DRY_M && hr <= H_DRY_M {
                    self.u_edge[e_idx] = 0.0;
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
                    self.u_edge[e_idx] = 0.0;
                    continue;
                }

                let zeta_l_eff = zb_star + hl_star;
                let zeta_r_eff = zb_star + hr_star;
                let h_edge = 0.5 * (hl_star + hr_star);
                let wave_scale = if h_edge > H_WAVE_CAP_M {
                    H_WAVE_CAP_M / h_edge
                } else {
                    1.0
                };

                let eta_anom_l = zetal - zbl.max(0.0);
                let eta_anom_r = zetar - zbr.max(0.0);
                let sal_weight = (h_edge / 200.0).clamp(0.0, 1.0) * SAL_BETA;

                let d_zeta_dn = (zeta_r_eff - zeta_l_eff) * e.inv_center_dist_m;
                let d_sal_dn = (eta_anom_r - eta_anom_l) * e.inv_center_dist_m;
                let d_eq_dn = (self.eta_eq_m[r] - self.eta_eq_m[l]) * e.inv_center_dist_m;

                let vt_l = e.nx_left * self.v_north[l] - e.ny_left * self.u_east[l];
                let vt_r = e.nx_right * self.v_north[r] - e.ny_right * self.u_east[r];
                let cor_acc = 0.5 * (geom.coriolis_f[l] * vt_l + geom.coriolis_f[r] * vt_r);

                let atm_acc = if has_atm {
                    self.scratch_atm_dp_acc[e_idx]
                        + self.scratch_atm_tau_acc[e_idx] / h_edge.max(2.5)
                } else {
                    0.0
                };

                let accel = cor_acc
                    + wave_scale
                        * (-g * d_zeta_dn + g * sal_weight * d_sal_dn + g * d_eq_dn + atm_acc);

                let u_old = self.u_edge[e_idx];
                let vt_avg = 0.5 * (vt_l + vt_r);
                let speed_mag = (u_old * u_old + vt_avg * vt_avg + U_BG_MPS * U_BG_MPS).sqrt();

                let r_bot = CD_BOTTOM * speed_mag / h_edge.max(1.5);
                let n_bv = 0.0025 * (h_edge / 300.0).clamp(0.0, 1.0);
                let r_wave = kappa_wave * n_bv * self.edge_slope_sq[e_idx] / h_edge.max(25.0);
                let r_tot = r_bot + r_wave + r_rad;

                let u_new = ((u_old + dt * accel) / (1.0 + dt * r_tot)).clamp(-12.0, 12.0);
                self.u_edge[e_idx] = u_new;
                self.scratch_edge_diss[e_idx] +=
                    RHO_SEAWATER * h_edge * (r_bot + r_wave) * u_new * u_new;
            }

            // 2 & 3. Upwind thickness flux with positivity limiter + conservative thickness update.
            advance_active_thickness(
                geom,
                &self.active_cells,
                &self.active_edges,
                dt,
                &self.bed_m,
                &self.u_edge,
                &mut self.h_total_m,
                &mut self.scratch_flux,
                &mut self.scratch_outflow_vol,
                &mut self.scratch_outflow_scale,
            );
        }

        self.finalize_hour(geom);
    }

    /// Steps both `self` (coupled dynamic tides + atmospheric storm surge) and `astro_ref`
    /// (unforced astronomical reference tide) by `1 hour` in fused cache-shared passes, writing
    /// `out_storm_surge_m = self.eta_m - astro_ref.eta_m`.
    ///
    /// When neither lunar nor solar tides are active, `astro_ref` remains at lake-at-rest (`0 m`)
    /// and is skipped completely.
    pub fn step_hour_paired_storm_surge(
        &mut self,
        astro_ref: &mut Self,
        geom: &FiniteVolumeSphereGeom,
        forcing: AtmosphericTideForcing<'_>,
        out_storm_surge_m: &mut [f32],
    ) {
        assert_eq!(out_storm_surge_m.len(), self.num_cells);
        if !self.has_astro_forcing() && !astro_ref.astro_dirty {
            self.step_hour(geom, Some(forcing));
            astro_ref.elapsed_hours = self.elapsed_hours;
            out_storm_surge_m.copy_from_slice(&self.eta_m);
            return;
        }

        self.ensure_active_domain(geom);
        astro_ref.inundation_ceil_m = astro_ref.inundation_ceil_m.max(self.inundation_ceil_m);
        astro_ref.ensure_active_domain(geom);

        let base_seconds = (self.elapsed_hours as f64) * 3600.0;
        let dt = self.dt_sub_s;
        let g = self.gravity_mps2;
        let r_rad = 1.7e-5_f32;
        let kappa_wave = 1.6e4_f32;

        self.precompute_hourly_atm_edge_forcing(geom, forcing);
        for &e_u32 in &self.active_edges {
            self.scratch_edge_diss[e_u32 as usize] = 0.0;
        }
        if self.has_astro_forcing() {
            self.astro_dirty = true;
            astro_ref.astro_dirty = true;
        }

        for sub in 0..self.n_substeps_per_hour {
            // 1. Evaluate astronomical bodies every 4 substeps (~15 min) and fill equilibrium tide.
            if sub % 4 == 0 {
                let block = (self.n_substeps_per_hour - sub).min(4) as f64;
                let t_sec = base_seconds + ((sub as f64) + 0.5 * block) * (dt as f64);
                let (bodies, n_bodies) = self.compute_astro_bodies(geom, t_sec);
                if n_bodies == 0 {
                    if self.eta_eq_nonzero {
                        for &c_u32 in &self.active_cells {
                            let i = c_u32 as usize;
                            self.eta_eq_m[i] = 0.0;
                            astro_ref.eta_eq_m[i] = 0.0;
                        }
                        self.eta_eq_nonzero = false;
                        astro_ref.eta_eq_nonzero = false;
                    }
                } else {
                    self.eta_eq_nonzero = true;
                    astro_ref.eta_eq_nonzero = true;
                    let body_slice = &bodies[..n_bodies];
                    let mut wet_sum_a = 0.0_f64;
                    let mut wet_area_a = 0.0_f64;
                    let mut wet_sum_b = 0.0_f64;
                    let mut wet_area_b = 0.0_f64;

                    for &c_u32 in &self.active_cells {
                        let i = c_u32 as usize;
                        let p = self.unit_centers[i];
                        let mut val = 0.0_f32;
                        for &(rb, amp) in body_slice {
                            let cos_z = p[0] * rb[0] + p[1] * rb[1] + p[2] * rb[2];
                            val += amp * 0.5 * (3.0 * cos_z * cos_z - 1.0);
                        }
                        self.eta_eq_m[i] = val;
                        astro_ref.eta_eq_m[i] = val;
                        let a = geom.areas_m2[i] as f64;
                        if self.h_total_m[i] > H_DRY_M {
                            wet_sum_a += (val as f64) * a;
                            wet_area_a += a;
                        }
                        if astro_ref.h_total_m[i] > H_DRY_M {
                            wet_sum_b += (val as f64) * a;
                            wet_area_b += a;
                        }
                    }
                    let mean_a = if wet_area_a > 1.0 {
                        (wet_sum_a / wet_area_a) as f32
                    } else {
                        0.0
                    };
                    let mean_b = if wet_area_b > 1.0 {
                        (wet_sum_b / wet_area_b) as f32
                    } else {
                        0.0
                    };
                    for &c_u32 in &self.active_cells {
                        let i = c_u32 as usize;
                        self.eta_eq_m[i] -= mean_a;
                        astro_ref.eta_eq_m[i] -= mean_b;
                    }
                }
            }

            // 2. Fused Perot velocity reconstruction for both oceans in a single stencil pass.
            if geom.all_deg_le_6 {
                for &c_u32 in &self.active_cells {
                    let c = c_u32 as usize;
                    let st = &geom.cell_perot6[c];
                    let mut ux_a = 0.0_f32;
                    let mut vy_a = 0.0_f32;
                    let mut ux_b = 0.0_f32;
                    let mut vy_b = 0.0_f32;
                    for k in 0..6 {
                        let e_idx = st.edge_idx[k] as usize;
                        let wx = st.perot_wx[k];
                        let wy = st.perot_wy[k];
                        let ua = self.u_edge[e_idx];
                        let ub = astro_ref.u_edge[e_idx];
                        ux_a += ua * wx;
                        vy_a += ua * wy;
                        ux_b += ub * wx;
                        vy_b += ub * wy;
                    }
                    self.u_east[c] = ux_a;
                    self.v_north[c] = vy_a;
                    astro_ref.u_east[c] = ux_b;
                    astro_ref.v_north[c] = vy_b;
                }
            } else {
                geom.reconstruct_cell_vectors(&self.u_edge, &mut self.u_east, &mut self.v_north);
                geom.reconstruct_cell_vectors(
                    &astro_ref.u_edge,
                    &mut astro_ref.u_east,
                    &mut astro_ref.v_north,
                );
            }

            // 3. Fused Audusse edge momentum update sharing `geom.edges`, `bed_m`, `coriolis_f`, and `edge_slope_sq`.
            for &e_u32 in &self.active_edges {
                let e_idx = e_u32 as usize;
                let e = &geom.edges[e_idx];
                let l = e.left as usize;
                let r = e.right as usize;
                let zbl = self.bed_m[l];
                let zbr = self.bed_m[r];
                let zb_star = zbl.max(zbr);
                let zb_l_pos = zbl.max(0.0);
                let zb_r_pos = zbr.max(0.0);
                let fl = geom.coriolis_f[l];
                let fr = geom.coriolis_f[r];
                let slope_sq = self.edge_slope_sq[e_idx];

                // Coupled tide + storm surge edge update:
                let hl_a = self.h_total_m[l];
                let hr_a = self.h_total_m[r];
                if hl_a <= H_DRY_M && hr_a <= H_DRY_M {
                    self.u_edge[e_idx] = 0.0;
                } else {
                    let zetal = zbl + hl_a;
                    let zetar = zbr + hr_a;
                    let hl_star = (zetal - zb_star).max(0.0);
                    let hr_star = (zetar - zb_star).max(0.0);
                    if hl_star <= H_DRY_M && hr_star <= H_DRY_M {
                        self.u_edge[e_idx] = 0.0;
                    } else {
                        let h_edge = 0.5 * (hl_star + hr_star);
                        let wave_scale = if h_edge > H_WAVE_CAP_M {
                            H_WAVE_CAP_M / h_edge
                        } else {
                            1.0
                        };
                        let sal_weight = (h_edge / 200.0).clamp(0.0, 1.0) * SAL_BETA;
                        let d_zeta_dn = (hr_star - hl_star) * e.inv_center_dist_m;
                        let d_sal_dn =
                            ((zetar - zb_r_pos) - (zetal - zb_l_pos)) * e.inv_center_dist_m;
                        let d_eq_dn = (self.eta_eq_m[r] - self.eta_eq_m[l]) * e.inv_center_dist_m;

                        let vt_l = e.nx_left * self.v_north[l] - e.ny_left * self.u_east[l];
                        let vt_r = e.nx_right * self.v_north[r] - e.ny_right * self.u_east[r];
                        let cor_acc = 0.5 * (fl * vt_l + fr * vt_r);
                        let atm_acc = self.scratch_atm_dp_acc[e_idx]
                            + self.scratch_atm_tau_acc[e_idx] / h_edge.max(2.5);
                        let accel = cor_acc
                            + wave_scale
                                * (-g * d_zeta_dn
                                    + g * sal_weight * d_sal_dn
                                    + g * d_eq_dn
                                    + atm_acc);

                        let u_old = self.u_edge[e_idx];
                        let vt_avg = 0.5 * (vt_l + vt_r);
                        let speed_mag =
                            (u_old * u_old + vt_avg * vt_avg + U_BG_MPS * U_BG_MPS).sqrt();
                        let r_bot = CD_BOTTOM * speed_mag / h_edge.max(1.5);
                        let n_bv = 0.0025 * (h_edge / 300.0).clamp(0.0, 1.0);
                        let r_wave = kappa_wave * n_bv * slope_sq / h_edge.max(25.0);
                        let r_tot = r_bot + r_wave + r_rad;

                        let u_new = ((u_old + dt * accel) / (1.0 + dt * r_tot)).clamp(-12.0, 12.0);
                        self.u_edge[e_idx] = u_new;
                        self.scratch_edge_diss[e_idx] +=
                            RHO_SEAWATER * h_edge * (r_bot + r_wave) * u_new * u_new;
                    }
                }

                // Unforced astronomical reference edge update:
                let hl_b = astro_ref.h_total_m[l];
                let hr_b = astro_ref.h_total_m[r];
                if hl_b <= H_DRY_M && hr_b <= H_DRY_M {
                    astro_ref.u_edge[e_idx] = 0.0;
                } else {
                    let zetal = zbl + hl_b;
                    let zetar = zbr + hr_b;
                    let hl_star = (zetal - zb_star).max(0.0);
                    let hr_star = (zetar - zb_star).max(0.0);
                    if hl_star <= H_DRY_M && hr_star <= H_DRY_M {
                        astro_ref.u_edge[e_idx] = 0.0;
                    } else {
                        let h_edge = 0.5 * (hl_star + hr_star);
                        let wave_scale = if h_edge > H_WAVE_CAP_M {
                            H_WAVE_CAP_M / h_edge
                        } else {
                            1.0
                        };
                        let sal_weight = (h_edge / 200.0).clamp(0.0, 1.0) * SAL_BETA;
                        let d_zeta_dn = (hr_star - hl_star) * e.inv_center_dist_m;
                        let d_sal_dn =
                            ((zetar - zb_r_pos) - (zetal - zb_l_pos)) * e.inv_center_dist_m;
                        let d_eq_dn =
                            (astro_ref.eta_eq_m[r] - astro_ref.eta_eq_m[l]) * e.inv_center_dist_m;

                        let vt_l =
                            e.nx_left * astro_ref.v_north[l] - e.ny_left * astro_ref.u_east[l];
                        let vt_r =
                            e.nx_right * astro_ref.v_north[r] - e.ny_right * astro_ref.u_east[r];
                        let cor_acc = 0.5 * (fl * vt_l + fr * vt_r);
                        let accel = cor_acc
                            + wave_scale
                                * (-g * d_zeta_dn + g * sal_weight * d_sal_dn + g * d_eq_dn);

                        let u_old = astro_ref.u_edge[e_idx];
                        let vt_avg = 0.5 * (vt_l + vt_r);
                        let speed_mag =
                            (u_old * u_old + vt_avg * vt_avg + U_BG_MPS * U_BG_MPS).sqrt();
                        let r_bot = CD_BOTTOM * speed_mag / h_edge.max(1.5);
                        let n_bv = 0.0025 * (h_edge / 300.0).clamp(0.0, 1.0);
                        let r_wave = kappa_wave * n_bv * slope_sq / h_edge.max(25.0);
                        let r_tot = r_bot + r_wave + r_rad;

                        astro_ref.u_edge[e_idx] =
                            ((u_old + dt * accel) / (1.0 + dt * r_tot)).clamp(-12.0, 12.0);
                    }
                }
            }

            // 4. Fused conservative thickness updates for both oceans:
            advance_active_thickness_paired(
                geom,
                &self.active_cells,
                &self.active_edges,
                dt,
                &self.bed_m,
                &self.u_edge,
                &mut self.h_total_m,
                &mut self.scratch_flux,
                &mut self.scratch_outflow_vol,
                &mut self.scratch_outflow_scale,
                &astro_ref.u_edge,
                &mut astro_ref.h_total_m,
                &mut astro_ref.scratch_flux,
                &mut astro_ref.scratch_outflow_vol,
                &mut astro_ref.scratch_outflow_scale,
            );
        }

        self.finalize_hour(geom);
        astro_ref.elapsed_hours += 1;

        for i in 0..self.num_cells {
            let h_ref = astro_ref.h_total_m[i];
            let eta_ref = if h_ref > H_DRY_M {
                astro_ref.bed_m[i] + h_ref
            } else {
                0.0
            };
            astro_ref.eta_m[i] = eta_ref;
            out_storm_surge_m[i] = self.eta_m[i] - eta_ref;
        }
    }

    fn push_history_frame(&mut self) {
        let mut frame = if self.eta_history_mm.len() >= HISTORY_HOURS {
            self.eta_history_mm
                .pop_front()
                .unwrap_or_else(|| vec![0_i16; self.num_cells])
        } else {
            vec![0_i16; self.num_cells]
        };
        if frame.len() != self.num_cells {
            frame.resize(self.num_cells, 0);
        }
        for i in 0..self.num_cells {
            frame[i] = (self.eta_m[i] * 1000.0).round().clamp(-32_000.0, 32_000.0) as i16;
        }
        self.eta_history_mm.push_back(frame);
    }

    fn recompute_rolling_tidal_range(&mut self) {
        let len = self.eta_history_mm.len();
        if len < 2 {
            self.tidal_range_m.fill(0.0);
            return;
        }
        let skip = len.saturating_sub(RANGE_WINDOW_HOURS);
        for &c_u32 in &self.active_cells {
            let i = c_u32 as usize;
            self.scratch_range_min[i] = i16::MAX;
            self.scratch_range_max[i] = i16::MIN;
        }
        for frame in self.eta_history_mm.iter().skip(skip) {
            for &c_u32 in &self.active_cells {
                let i = c_u32 as usize;
                let v = frame[i];
                if v < self.scratch_range_min[i] {
                    self.scratch_range_min[i] = v;
                }
                if v > self.scratch_range_max[i] {
                    self.scratch_range_max[i] = v;
                }
            }
        }
        for &c_u32 in &self.active_cells {
            let i = c_u32 as usize;
            if self.h_total_m[i] > H_DRY_M || self.initial_ocean[i] {
                self.tidal_range_m[i] = ((self.scratch_range_max[i] as i32
                    - self.scratch_range_min[i] as i32)
                    .max(0) as f32)
                    * 0.001;
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
        self.active_cells
            .iter()
            .filter(|&&c| self.is_inundated_land(c as usize))
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

    #[test]
    fn test_paired_storm_surge_matches_separate_solvers() {
        let (grid, geom, bed_m) = make_test_ocean(4);
        let n = geom.num_cells;
        let p_anom: Vec<f32> = (0..n).map(|i| -1800.0 * ((i as f32 * 0.17).sin())).collect();
        let tau_x: Vec<f32> = (0..n).map(|i| 0.85 * ((i as f32 * 0.11).cos())).collect();
        let tau_y: Vec<f32> = (0..n).map(|i| -0.45 * ((i as f32 * 0.23).sin())).collect();
        let forcing = AtmosphericTideForcing {
            p_anom_pa: &p_anom,
            tau_east_pa: &tau_x,
            tau_north_pa: &tau_y,
        };

        let mut sep_coupled = LaplaceTidalSolver::new(
            &grid, &geom, bed_m.clone(), 24.0, 23.44, 365.25, 0.0, 1.0, 1, 1.0, 60.27,
        );
        let mut sep_astro = sep_coupled.clone();

        let mut fused_coupled = sep_coupled.clone();
        let mut fused_astro = sep_coupled.clone();
        let mut surge = vec![0.0_f32; n];

        for _ in 0..8 {
            sep_coupled.step_hour(&geom, Some(forcing));
            sep_astro.step_hour(&geom, None);
            fused_coupled.step_hour_paired_storm_surge(&mut fused_astro, &geom, forcing, &mut surge);
        }

        for i in 0..n {
            let expected_surge = sep_coupled.eta_m[i] - sep_astro.eta_m[i];
            assert!(
                (surge[i] - expected_surge).abs() < 1e-5,
                "paired storm surge mismatch at cell {i}: got {}, expected {expected_surge}",
                surge[i]
            );
            assert!((fused_coupled.dissipation_wm2[i] - sep_coupled.dissipation_wm2[i]).abs() < 1e-5);
        }
    }
}
