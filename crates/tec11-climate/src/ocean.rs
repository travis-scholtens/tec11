use std::f64::consts::PI;
use tec11_math_grid::SphericalGrid;

use crate::model::wind_direction_deg;

/// Computed surface ocean circulation and Sea Surface Temperature (SST) anomaly fields
/// for a single season (or annual mean).
#[derive(Clone, Debug)]
pub struct OceanCirculation {
    /// Barotropic Stommel streamfunction $\psi$ at each tile (`0.0` on land).
    pub streamfunction: Vec<f32>,
    /// Normalized 3D velocity vector `(ux, uy, uz)` in grid coordinates on the unit sphere
    /// for each tile (`(0, 0, 0)` on land). Magnitude is normalized to `[0.0, 1.0]`.
    pub velocity_xyz: Vec<[f32; 3]>,
    /// Current speed normalized to `[0.0, 1.0]` (`0.0` on land).
    pub speed: Vec<f32>,
    /// Sea Surface Temperature anomaly $\Delta T_{\text{SST}}$ in `[-1.0, 1.0]`
    /// (`> 0` = warm poleward current, `< 0` = cold equatorward current / coastal upwelling).
    pub sst_anomaly: Vec<f32>,
    /// Marine thermal & moisture influence propagated onto land tiles in `[-1.0, 1.0]`.
    pub coastal_sst_influence: Vec<f32>,
}

#[derive(Clone, Copy)]
struct NeighborGeom {
    idx: usize,
    dx_east: f64,
    dy_north: f64,
    inv_d2: f64,
}

/// Solve the coupled wind-driven Stommel–Ekman ocean circulation and SST anomaly field
/// across all seasons in `seasons` on an arbitrary planet grid, accounting for `glaciation`.
///
/// Generalizes seasonal & glacial ocean circulation to any planet geometry via:
/// 1. **Seasonal ITCZ & Wind-Belt Shift:** Solar declination $\delta(s) = -\text{tilt\_deg} \cdot s$
///    shifts the atmospheric circulation cells latitudinally, amplified where landmasses lie
///    poleward of an ocean basin.
/// 2. **Continental Monsoon Wind Reversals:** Precomputes the spherical gradient of continental
///    land fraction so summer heating over continents draws cross-equatorial/onshore monsoon winds
///    and winter cooling drives offshore winds, deflected by the planet's Coriolis parameter.
/// 3. **Glacial / Greenhouse Forcing (`glaciation` in `[0.0, 1.0]`):**
///    - **Polar Sea-Ice Lid:** In cold glacial states (`glaciation < 0.5`), the sea-ice margin
///      expands equatorward (down to $\sim 48^\circ$ at `glaciation = 0.0`), damping surface wind
///      stress and deflecting warm western boundary extensions (like the North Atlantic Drift and
///      Kuroshio) eastward at lower latitudes. In warm greenhouse states (`glaciation > 0.5`),
///      sea ice retreats and warm currents penetrate deep into polar basins.
///    - **Thermal Gradient & Wind Compression:** Glacial polar expansion compresses mid-latitude
///      and tropical wind belts equatorward and intensifies trade-wind stress and cold coastal
///      upwelling; greenhouse warming expands wind belts poleward.
/// 4. **Two-Timescale Barotropic + Ekman Response & Periodic SST Cycle:** Mid-latitude Stommel
///    gyres retain multi-season momentum inertia while tropical/monsoonal currents, surface Ekman
///    drift, and coastal upwelling adjust rapidly to each season's wind stress.
pub fn solve_seasonal_ocean_circulation(
    grid: &SphericalGrid,
    adjacency: &[Vec<usize>],
    lats_deg: &[f64],
    elevations: &[f32],
    cells: u32,
    spin: f64,
    tilt_deg: f64,
    glaciation: f64,
    seasons: &[f64],
) -> Vec<OceanCirculation> {
    let n = grid.faces.len();
    let num_seasons = seasons.len().max(1);
    // In Tec-Nine (`climatemethod.py:51`), `glaciation = 0.0` is coldest (maximum glaciation / ice age),
    // `0.5` is present-day baseline, and `1.0` is warmest (greenhouse).
    // Define `ice_index` in `[-1.0, +1.0]` where `+1.0` = glacial maximum, `0.0` = baseline, `-1.0` = greenhouse.
    let ice_index = ((0.5 - glaciation.clamp(0.0, 1.0)) / 0.5).clamp(-1.0, 1.0);

    // 1. Precompute local orthonormal tangent basis (e_east, e_north) at each tile.
    let mut e_east = Vec::with_capacity(n);
    let mut e_north = Vec::with_capacity(n);
    for &([x, y, z], _) in &grid.faces {
        let xy = (x * x + y * y).sqrt();
        if xy > 1e-9 {
            // In our globe coordinate convention (lat = asin(z), lon = atan2(-y, x)):
            // East (+lon) is (y, -x, 0) / xy, and North (+lat) is (-x*z, -y*z, xy*xy) / xy.
            e_east.push([y / xy, -x / xy, 0.0]);
            e_north.push([-x * z / xy, -y * z / xy, xy]);
        } else {
            e_east.push([0.0, -1.0, 0.0]);
            e_north.push([1.0, 0.0, 0.0]);
        }
    }

    // 2. Precompute local neighbor offsets (dx_east, dy_north) in radians on the unit sphere.
    let mut neighbor_geoms: Vec<Vec<NeighborGeom>> = Vec::with_capacity(n);
    for v in 0..n {
        let ([vx, vy, vz], _) = grid.faces[v];
        let east = e_east[v];
        let north = e_north[v];
        let mut ngs = Vec::with_capacity(adjacency[v].len());
        for &u in &adjacency[v] {
            let ([ux, uy, uz], _) = grid.faces[u];
            let rx = ux - vx;
            let ry = uy - vy;
            let rz = uz - vz;
            let dx = rx * east[0] + ry * east[1] + rz * east[2];
            let dy = rx * north[0] + ry * north[1] + rz * north[2];
            let d2 = (dx * dx + dy * dy).max(1e-7);
            ngs.push(NeighborGeom {
                idx: u,
                dx_east: dx,
                dy_north: dy,
                inv_d2: 1.0 / d2,
            });
        }
        neighbor_geoms.push(ngs);
    }

    // 3. Precompute smoothed regional continentality and its spatial gradient
    //    (land_grad_e, land_grad_n) pointing toward nearby landmasses (~25-35 deg scale).
    let mut smooth_land: Vec<f64> = elevations
        .iter()
        .map(|&h| if h > 0.0 { 1.0 } else { 0.0 })
        .collect();
    for _iter in 0..12 {
        let prev = smooth_land.clone();
        for v in 0..n {
            let mut sum = prev[v];
            for &u in &adjacency[v] {
                sum += prev[u];
            }
            smooth_land[v] = sum / ((adjacency[v].len() + 1) as f64);
        }
    }

    let mut land_grad_e = vec![0.0f64; n];
    let mut land_grad_n = vec![0.0f64; n];
    for v in 0..n {
        let ngs = &neighbor_geoms[v];
        let k = ngs.len().max(1) as f64;
        let mut ge = 0.0;
        let mut gn = 0.0;
        for ng in ngs {
            let dl = smooth_land[ng.idx] - smooth_land[v];
            ge += dl * ng.dx_east * ng.inv_d2;
            gn += dl * ng.dy_north * ng.inv_d2;
        }
        land_grad_e[v] = ((2.0 / k) * ge * 0.35).clamp(-1.0, 1.0);
        land_grad_n[v] = ((2.0 / k) * gn * 0.35).clamp(-1.0, 1.0);
    }

    // Equator-to-pole thermal gradient factor: stronger in glacial conditions, weaker in greenhouse.
    let wind_intensity = 1.0 + 0.25 * ice_index;
    // Latitude compression of Hadley/Ferrel cells as polar ice caps expand/contract:
    let cell_lat_compression = 1.0 + 0.16 * ice_index;

    // Helper closure to compute seasonal wind stress (tau_e, tau_n), Ekman drift (ek_e, ek_n),
    // wind-stress curl C_tau, and sea-ice cover fraction `ice_cover` in `[0.0, 1.0]`.
    let compute_wind_forcing = |season_val: f64| -> (
        Vec<f64>,
        Vec<f64>,
        Vec<f64>,
        Vec<f64>,
        Vec<f64>,
        Vec<f64>,
    ) {
        let subsolar_lat_deg = -tilt_deg * season_val;
        let season_phase = if tilt_deg.abs() > 1e-6 {
            -season_val
        } else {
            0.0
        };

        let mut tau_e = vec![0.0f64; n];
        let mut tau_n = vec![0.0f64; n];
        let mut ekman_e = vec![0.0f64; n];
        let mut ekman_n = vec![0.0f64; n];
        let mut ice_cover = vec![0.0f64; n];

        for v in 0..n {
            let lat = lats_deg[v];
            let lat_abs = lat.abs();
            let hemi_sign = if lat >= 0.0 { 1.0 } else { -1.0 };
            let local_summer = season_phase * hemi_sign;

            // Sea-ice edge latitude: ~68 deg at baseline (glaciation = 0.5),
            // advancing to ~46 deg at glacial maximum (glaciation = 0.0) and retreating
            // to ~90 deg in greenhouse conditions (glaciation = 1.0), plus +/-5 deg seasonal swing.
            let ice_edge_lat = 68.0 - 22.0 * ice_index + 5.0 * local_summer;
            let ice_frac = ((lat_abs - (ice_edge_lat - 5.0)) / 10.0).clamp(0.0, 1.0);
            ice_cover[v] = ice_frac;

            // ITCZ and atmospheric cell boundaries shift toward the summer hemisphere,
            // compressed toward the equator during glacial maxima.
            let monsoon_pull = (season_phase * land_grad_n[v]).max(0.0);
            let itcz_shift_deg =
                subsolar_lat_deg * (0.42 + 0.48 * monsoon_pull) * (1.0 - 0.15 * ice_index);
            let eff_lat = ((lat - itcz_shift_deg) * cell_lat_compression).clamp(-89.5, 89.5);

            let (c_lo, c_hi, alpha) =
                if (spin - 1.0).abs() > 1e-6 && cells == crate::model::nominal_cells_for_spin(spin)
                {
                    crate::model::continuous_cell_blend(spin)
                } else {
                    (cells, cells, 0.0)
                };
            let dir_lo = wind_direction_deg(eff_lat, c_lo, spin) * PI / 180.0;
            let (mut we, mut wn) = if c_lo == c_hi || alpha <= 1e-6 {
                (dir_lo.sin() * wind_intensity, dir_lo.cos() * wind_intensity)
            } else {
                let dir_hi = wind_direction_deg(eff_lat, c_hi, spin) * PI / 180.0;
                let we_b = (1.0 - alpha) * dir_lo.sin() + alpha * dir_hi.sin();
                let wn_b = (1.0 - alpha) * dir_lo.cos() + alpha * dir_hi.cos();
                let norm = (we_b * we_b + wn_b * wn_b).sqrt().max(0.35);
                (
                    (we_b / norm) * wind_intensity,
                    (wn_b / norm) * wind_intensity,
                )
            };

            // Direct continental monsoon pressure-gradient wind:
            let hemi_summer = season_phase * ((lat + 0.4 * subsolar_lat_deg) / 20.0).tanh();
            let tropical_weight = (-((lat / 42.0).powi(2))).exp();
            let monsoon_strength = 0.65 * hemi_summer * tropical_weight;
            let lat_rad = lat * PI / 180.0;
            let cor_angle = -30.0 * (PI / 180.0) * lat_rad.sin() * spin.signum();
            let cos_c = cor_angle.cos();
            let sin_c = cor_angle.sin();
            let lge = land_grad_e[v];
            let lgn = land_grad_n[v];
            let mon_e = lge * cos_c - lgn * sin_c;
            let mon_n = lge * sin_c + lgn * cos_c;

            we += monsoon_strength * mon_e;
            wn += monsoon_strength * mon_n;

            // Sea-ice mechanical lid shields the surface ocean from wind stress.
            let open_water_factor = 1.0 - 0.85 * ice_frac;
            we *= open_water_factor;
            wn *= open_water_factor;

            tau_e[v] = we;
            tau_n[v] = wn;

            // Surface Ekman deflection: ~35 deg to the right in NH (clockwise), left in SH.
            let ek_angle = -35.0 * (PI / 180.0) * lat_rad.sin() * spin.signum();
            let cos_a = ek_angle.cos();
            let sin_a = ek_angle.sin();
            ekman_e[v] = we * cos_a - wn * sin_a;
            ekman_n[v] = we * sin_a + wn * cos_a;
        }

        let mut wind_curl = vec![0.0f64; n];
        for v in 0..n {
            if elevations[v] > 0.0 {
                continue;
            }
            let ngs = &neighbor_geoms[v];
            let k = ngs.len().max(1) as f64;
            let mut dtaun_dx = 0.0;
            let mut dtaue_dy = 0.0;
            for ng in ngs {
                let u = ng.idx;
                dtaun_dx += (tau_n[u] - tau_n[v]) * ng.dx_east * ng.inv_d2;
                dtaue_dy += (tau_e[u] - tau_e[v]) * ng.dy_north * ng.inv_d2;
            }
            wind_curl[v] = (2.0 / k) * (dtaun_dx - dtaue_dy);
        }

        (tau_e, tau_n, ekman_e, ekman_n, wind_curl, ice_cover)
    };

    // Helper closure to run SOR iterations for the Stommel barotropic streamfunction PDE:
    //   r_b * Laplacian(psi) + beta * d(psi)/dx = C_tau, with psi = 0 on land and heavy damping under thick sea ice.
    let solve_stommel_sor =
        |wind_curl: &[f64], ice_cover: &[f64], init_psi: &[f64], iters: usize| -> Vec<f64> {
            let r_b_base = 0.08;
            let beta_0 = 2.2 * spin;
            let omega_sor = 1.45;
            let mut psi = init_psi.to_vec();

            for _iter in 0..iters {
                for v in 0..n {
                    if elevations[v] > 0.0 || ice_cover[v] > 0.92 {
                        psi[v] = 0.0;
                        continue;
                    }
                    let lat_rad = lats_deg[v] * PI / 180.0;
                    let beta = beta_0 * lat_rad.cos();
                    // Under partial sea ice, ice-ocean drag increases r_b, deflecting subpolar gyres
                    // equatorward along the sea-ice edge.
                    let r_b = r_b_base * (1.0 + 2.5 * ice_cover[v]);
                    let ngs = &neighbor_geoms[v];
                    let k = ngs.len().max(1) as f64;

                    let mut weight_sum = 0.0;
                    let mut neighbor_sum = 0.0;
                    for ng in ngs {
                        let u = ng.idx;
                        let psi_u = if elevations[u] > 0.0 || ice_cover[u] > 0.92 {
                            0.0
                        } else {
                            psi[u]
                        };
                        let upwind_dx = if beta >= 0.0 {
                            ng.dx_east.max(0.0)
                        } else {
                            (-ng.dx_east).max(0.0)
                        };
                        let w = (4.0 * r_b + 4.0 * beta.abs() * upwind_dx) * ng.inv_d2 / k;
                        weight_sum += w;
                        neighbor_sum += w * psi_u;
                    }

                    if weight_sum > 1e-12 {
                        let target = (neighbor_sum - wind_curl[v]) / weight_sum;
                        psi[v] =
                            ((1.0 - omega_sor) * psi[v] + omega_sor * target) * (1.0 - 0.35 * ice_cover[v]);
                    }
                }
            }
            psi
        };

    // 4. Solve the annual-mean Stommel streamfunction first (equinox baseline).
    let (_, _, _, _, annual_curl, annual_ice) = compute_wind_forcing(0.0);
    let zero_psi = vec![0.0f64; n];
    let annual_psi = solve_stommel_sor(&annual_curl, &annual_ice, &zero_psi, 60);

    // 5. For each season, compute wind forcing, blended Stommel streamfunction,
    //    surface velocity (geostrophic + seasonal Ekman + monsoonal coastal current),
    //    and seasonal coastal upwelling.
    let mut seasonal_psi: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_tau_e: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_tau_n: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_u_east: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_u_north: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_raw_speed: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_upwelling: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);
    let mut seasonal_ice_cover: Vec<Vec<f64>> = Vec::with_capacity(num_seasons);

    for s_idx in 0..num_seasons {
        let season_val = seasons.get(s_idx).copied().unwrap_or(0.0);
        let (tau_e, tau_n, ekman_e, ekman_n, wind_curl, ice_cover) =
            compute_wind_forcing(season_val);
        let eq_psi = solve_stommel_sor(&wind_curl, &ice_cover, &annual_psi, 30);

        let mut psi = vec![0.0f64; n];
        for v in 0..n {
            if elevations[v] <= 0.0 {
                let lat = lats_deg[v];
                let w_seas = 0.28 + 0.52 * (-((lat / 22.0).powi(2))).exp();
                psi[v] = (1.0 - w_seas) * annual_psi[v] + w_seas * eq_psi[v];
            }
        }

        let mut u_east = vec![0.0f64; n];
        let mut u_north = vec![0.0f64; n];
        let mut raw_speed = vec![0.0f64; n];
        let mut upwelling = vec![0.0f64; n];

        for v in 0..n {
            if elevations[v] > 0.0 {
                continue;
            }
            let ngs = &neighbor_geoms[v];
            let k = ngs.len().max(1) as f64;
            let mut dpsi_dx = 0.0;
            let mut dpsi_dy = 0.0;
            let mut has_land_neighbor = false;
            let mut max_offshore_ekman = 0.0f64;

            for ng in ngs {
                let u = ng.idx;
                let psi_u = if elevations[u] > 0.0 { 0.0 } else { psi[u] };
                dpsi_dx += (psi_u - psi[v]) * ng.dx_east * ng.inv_d2;
                dpsi_dy += (psi_u - psi[v]) * ng.dy_north * ng.inv_d2;

                if elevations[u] > 0.0 {
                    has_land_neighbor = true;
                    let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                        .sqrt()
                        .max(1e-6);
                    let dir_to_land_e = ng.dx_east / d_len;
                    let dir_to_land_n = ng.dy_north / d_len;
                    let offshore = -(ekman_e[v] * dir_to_land_e + ekman_n[v] * dir_to_land_n);
                    if offshore > max_offshore_ekman {
                        max_offshore_ekman = offshore;
                    }
                }
            }
            dpsi_dx *= 2.0 / k;
            dpsi_dy *= 2.0 / k;

            let lat_abs = lats_deg[v].abs();
            let trop_wind_weight = 0.25 * (-((lat_abs / 18.0).powi(2))).exp();
            let ice_damp = 1.0 - 0.80 * ice_cover[v];
            let mut ve = (-dpsi_dy + 0.35 * ekman_e[v] + trop_wind_weight * tau_e[v]) * ice_damp;
            let mut vn = (dpsi_dx + 0.35 * ekman_n[v] + trop_wind_weight * tau_n[v]) * ice_damp;

            if has_land_neighbor {
                for ng in ngs {
                    if elevations[ng.idx] > 0.0 {
                        let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                            .sqrt()
                            .max(1e-6);
                        let nx = ng.dx_east / d_len;
                        let ny = ng.dy_north / d_len;
                        let dot = ve * nx + vn * ny;
                        if dot > 0.0 {
                            ve -= dot * nx;
                            vn -= dot * ny;
                        }
                    }
                }
                let lat_weight = if lat_abs < 50.0 {
                    (lat_abs * PI / 50.0).sin().max(0.2)
                } else {
                    0.1
                };
                upwelling[v] = max_offshore_ekman * lat_weight * (1.0 + 0.20 * ice_index);
            }

            u_east[v] = ve;
            u_north[v] = vn;
            raw_speed[v] = (ve * ve + vn * vn).sqrt();
        }

        seasonal_psi.push(psi);
        seasonal_tau_e.push(tau_e);
        seasonal_tau_n.push(tau_n);
        seasonal_u_east.push(u_east);
        seasonal_u_north.push(u_north);
        seasonal_raw_speed.push(raw_speed);
        seasonal_upwelling.push(upwelling);
        seasonal_ice_cover.push(ice_cover);
    }

    // 6. Normalize speeds globally across all seasons.
    let mut all_ocean_speeds: Vec<f64> = Vec::new();
    for s_idx in 0..num_seasons {
        for &spd in &seasonal_raw_speed[s_idx] {
            if spd > 0.0 {
                all_ocean_speeds.push(spd);
            }
        }
    }
    all_ocean_speeds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let scale_speed = if all_ocean_speeds.is_empty() {
        1.0
    } else {
        let p98 = ((all_ocean_speeds.len() - 1) * 98) / 100;
        all_ocean_speeds[p98].max(1e-6)
    };

    // 7. Solve periodic seasonal Sea Surface Temperature T_SST with thermal inertia
    //    and glacial/greenhouse polar cooling/warming.
    let mut t_sst: Vec<f64> = (0..n)
        .map(|v| (lats_deg[v] * PI / 180.0).cos())
        .collect();

    let s0 = if num_seasons > 2 { 2 } else { 0 };
    for _iter in 0..25 {
        let prev = t_sst.clone();
        for v in 0..n {
            if elevations[v] > 0.0 {
                continue;
            }
            let lat_rad = lats_deg[v] * PI / 180.0;
            let eq_temp = lat_rad.cos();
            let poleward_vel = (seasonal_u_north[s0][v] / scale_speed).clamp(-1.2, 1.2)
                * lat_rad.sin().signum();
            let meridional_grad = (2.0 * lat_rad.abs()).sin().max(0.25);
            let relax_rate = 0.09;
            // High-latitude glacial cooling vs greenhouse polar amplification:
            let polar_glacial_forcing =
                -0.08 * ice_index * lat_rad.sin().powi(2) - 0.10 * seasonal_ice_cover[s0][v];
            let thermal_forcing = 0.14 * (1.0 - 0.20 * ice_index) * poleward_vel.max(0.0) * meridional_grad
                - 0.13 * seasonal_upwelling[s0][v]
                + polar_glacial_forcing;
            let mut w_sum = relax_rate;
            let mut val_sum = relax_rate * (eq_temp + thermal_forcing);

            for ng in &neighbor_geoms[v] {
                let u = ng.idx;
                if elevations[u] > 0.0 {
                    continue;
                }
                let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                    .sqrt()
                    .max(1e-6);
                let vel_u_into_v = -(seasonal_u_east[s0][u] * ng.dx_east
                    + seasonal_u_north[s0][u] * ng.dy_north)
                    / d_len;
                let vel_v_from_u = -(seasonal_u_east[s0][v] * ng.dx_east
                    + seasonal_u_north[s0][v] * ng.dy_north)
                    / d_len;
                let inflow = (0.5 * (vel_u_into_v + vel_v_from_u) / scale_speed).max(0.0);
                let w = 0.03 + 1.15 * inflow;
                w_sum += w;
                val_sum += w * prev[u];
            }
            t_sst[v] = val_sum / w_sum;
        }
    }

    let mut seasonal_raw_diff: Vec<Vec<f64>> = vec![vec![0.0; n]; num_seasons];
    let cycles = if num_seasons > 1 { 2 } else { 1 };
    let sweeps_per_season = if num_seasons > 1 { 12 } else { 20 };

    for _cycle in 0..cycles {
        for s_idx in 0..num_seasons {
            let season_val = seasons.get(s_idx).copied().unwrap_or(0.0);
            let subsolar_lat_deg = -tilt_deg * season_val;
            let u_east = &seasonal_u_east[s_idx];
            let u_north = &seasonal_u_north[s_idx];
            let upwelling = &seasonal_upwelling[s_idx];
            let ice_cover = &seasonal_ice_cover[s_idx];

            for _sweep in 0..sweeps_per_season {
                let prev = t_sst.clone();
                for v in 0..n {
                    if elevations[v] > 0.0 {
                        continue;
                    }
                    let lat_deg = lats_deg[v];
                    let lat_rad = lat_deg * PI / 180.0;
                    let shifted_lat_rad = (lat_deg - 0.20 * subsolar_lat_deg) * PI / 180.0;
                    let t_rad_season = shifted_lat_rad.cos();

                    let poleward_vel =
                        (u_north[v] / scale_speed).clamp(-1.2, 1.2) * lat_rad.sin().signum();
                    let meridional_grad = (2.0 * lat_rad.abs()).sin().max(0.25);
                    let relax_rate = 0.09;
                    let polar_glacial_forcing =
                        -0.08 * ice_index * lat_rad.sin().powi(2) - 0.10 * ice_cover[v];
                    let thermal_forcing = 0.15
                        * (1.0 - 0.20 * ice_index)
                        * poleward_vel.max(0.0)
                        * meridional_grad
                        - 0.14 * upwelling[v]
                        + polar_glacial_forcing;

                    let inertia_weight = 0.35;
                    let mut w_sum = inertia_weight + relax_rate;
                    let mut val_sum =
                        inertia_weight * prev[v] + relax_rate * (t_rad_season + thermal_forcing);

                    for ng in &neighbor_geoms[v] {
                        let u = ng.idx;
                        if elevations[u] > 0.0 {
                            continue;
                        }
                        let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                            .sqrt()
                            .max(1e-6);
                        let vel_u_into_v =
                            -(u_east[u] * ng.dx_east + u_north[u] * ng.dy_north) / d_len;
                        let vel_v_from_u =
                            -(u_east[v] * ng.dx_east + u_north[v] * ng.dy_north) / d_len;
                        let inflow = (0.5 * (vel_u_into_v + vel_v_from_u) / scale_speed).max(0.0);
                        let w = 0.03 + 1.15 * inflow;
                        w_sum += w;
                        val_sum += w * prev[u];
                    }
                    t_sst[v] = val_sum / w_sum;
                }
            }

            for v in 0..n {
                if elevations[v] <= 0.0 {
                    let lat_deg = lats_deg[v];
                    let baseline = ((lat_deg - 0.14 * subsolar_lat_deg) * PI / 180.0).cos();
                    seasonal_raw_diff[s_idx][v] = t_sst[v] - baseline;
                }
            }
        }
    }

    // Normalize positive and negative SST anomalies using a fixed physical reference scale
    // combined with the 95th percentile so glacial cooling / greenhouse warming shifts the
    // warm/cold balance visibly rather than being normalized away.
    let mut pos_diffs: Vec<f64> = Vec::new();
    let mut neg_diffs: Vec<f64> = Vec::new();
    for s_idx in 0..num_seasons {
        for &d in &seasonal_raw_diff[s_idx] {
            if d > 0.0 {
                pos_diffs.push(d);
            } else if d < 0.0 {
                neg_diffs.push(-d);
            }
        }
    }
    pos_diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    neg_diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let p95_pos = if pos_diffs.is_empty() {
        0.045
    } else {
        pos_diffs[((pos_diffs.len() - 1) * 95) / 100].max(0.025)
    };
    let p95_neg = if neg_diffs.is_empty() {
        0.045
    } else {
        neg_diffs[((neg_diffs.len() - 1) * 95) / 100].max(0.025)
    };
    // Blend adaptive percentile with fixed physical reference (`0.045`) so changing glaciation
    // expands/shrinks warm and cold current zones directly.
    let pos_scale = 0.5 * p95_pos + 0.5 * 0.045;
    let neg_scale = 0.5 * p95_neg + 0.5 * 0.045;

    // 8. Build final OceanCirculation struct for each season.
    let mut results = Vec::with_capacity(num_seasons);
    for s_idx in 0..num_seasons {
        let u_east = &seasonal_u_east[s_idx];
        let u_north = &seasonal_u_north[s_idx];
        let raw_speed = &seasonal_raw_speed[s_idx];
        let tau_e = &seasonal_tau_e[s_idx];
        let tau_n = &seasonal_tau_n[s_idx];

        let mut speed = vec![0.0f32; n];
        let mut velocity_xyz = vec![[0.0f32; 3]; n];
        for v in 0..n {
            if elevations[v] > 0.0 {
                continue;
            }
            let norm_s = (raw_speed[v] / scale_speed).min(1.0);
            speed[v] = norm_s as f32;
            if raw_speed[v] > 1e-9 {
                let ue = (u_east[v] / raw_speed[v]) * norm_s;
                let un = (u_north[v] / raw_speed[v]) * norm_s;
                let east = e_east[v];
                let north = e_north[v];
                velocity_xyz[v] = [
                    (ue * east[0] + un * north[0]) as f32,
                    (ue * east[1] + un * north[1]) as f32,
                    (ue * east[2] + un * north[2]) as f32,
                ];
            }
        }

        let mut sst = vec![0.0f64; n];
        for v in 0..n {
            if elevations[v] <= 0.0 {
                let d = seasonal_raw_diff[s_idx][v];
                sst[v] = if d >= 0.0 {
                    (d / pos_scale).clamp(0.0, 1.0)
                } else {
                    (d / neg_scale).clamp(-1.0, 0.0)
                };
            }
        }

        // Diffuse marine SST anomaly onto coastal/maritime land tiles (weighted by seasonal onshore wind).
        let mut coastal_sst = sst.clone();
        for _iter in 0..8 {
            let prev = coastal_sst.clone();
            for v in 0..n {
                if elevations[v] <= 0.0 {
                    continue;
                }
                let mut w_sum = 0.15;
                let mut val_sum = 0.0;
                for ng in &neighbor_geoms[v] {
                    let u = ng.idx;
                    let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                        .sqrt()
                        .max(1e-6);
                    let wind_inflow =
                        (-(tau_e[u] * ng.dx_east + tau_n[u] * ng.dy_north) / d_len).max(0.0);
                    let w = 0.1 + 0.9 * wind_inflow;
                    w_sum += w;
                    val_sum += w * prev[u];
                }
                let elev_decay = (1.0 - (elevations[v] as f64) * 0.25).clamp(0.1, 0.88);
                coastal_sst[v] = ((val_sum / w_sum) * elev_decay).clamp(-1.0, 1.0);
            }
        }

        results.push(OceanCirculation {
            streamfunction: seasonal_psi[s_idx].iter().map(|&p| p as f32).collect(),
            velocity_xyz,
            speed,
            sst_anomaly: sst.into_iter().map(|t| t as f32).collect(),
            coastal_sst_influence: coastal_sst.into_iter().map(|t| t as f32).collect(),
        });
    }

    results
}

/// Convenience wrapper for solving a single annual-mean ocean circulation snapshot at baseline glaciation (`0.5`).
pub fn solve_ocean_circulation(
    grid: &SphericalGrid,
    adjacency: &[Vec<usize>],
    lats_deg: &[f64],
    elevations: &[f32],
    cells: u32,
    spin: f64,
) -> OceanCirculation {
    solve_seasonal_ocean_circulation(
        grid,
        adjacency,
        lats_deg,
        elevations,
        cells,
        spin,
        0.0,
        0.5,
        &[0.0],
    )
    .into_iter()
    .next()
    .unwrap()
}
