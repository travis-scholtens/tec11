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
struct TangentBasis {
    east: [f64; 3],
    north: [f64; 3],
}

#[derive(Clone, Copy)]
struct NeighborGeom {
    idx: usize,
    dx_east: f64,
    dy_north: f64,
    inv_d2: f64,
}

#[derive(Clone, Copy)]
struct LandGradient {
    east: f64,
    north: f64,
}

struct SeasonalWindForcing {
    tau_e: Vec<f64>,
    tau_n: Vec<f64>,
    ekman_e: Vec<f64>,
    ekman_n: Vec<f64>,
    wind_curl: Vec<f64>,
    ice_cover: Vec<f64>,
}

#[derive(Clone)]
struct SeasonalOceanFields {
    psi: Vec<f64>,
    tau_e: Vec<f64>,
    tau_n: Vec<f64>,
    u_east: Vec<f64>,
    u_north: Vec<f64>,
    raw_speed: Vec<f64>,
    upwelling: Vec<f64>,
    ice_cover: Vec<f64>,
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

    // 1. Precompute local orthonormal tangent basis (east, north) at each tile.
    let bases: Vec<TangentBasis> = grid
        .faces
        .iter()
        .map(|&([x, y, z], _)| {
            let xy = (x * x + y * y).sqrt();
            if xy > 1e-9 {
                // In our globe coordinate convention (lat = asin(z), lon = atan2(-y, x)):
                // East (+lon) is (y, -x, 0) / xy, and North (+lat) is (-x*z, -y*z, xy*xy) / xy.
                TangentBasis {
                    east: [y / xy, -x / xy, 0.0],
                    north: [-x * z / xy, -y * z / xy, xy],
                }
            } else {
                TangentBasis {
                    east: [0.0, -1.0, 0.0],
                    north: [1.0, 0.0, 0.0],
                }
            }
        })
        .collect();

    // 2. Precompute local neighbor offsets (dx_east, dy_north) in radians on the unit sphere.
    let neighbor_geoms: Vec<Vec<NeighborGeom>> = (0..n)
        .map(|v| {
            let ([vx, vy, vz], _) = grid.faces[v];
            let TangentBasis { east, north } = bases[v];
            adjacency[v]
                .iter()
                .map(|&u| {
                    let ([ux, uy, uz], _) = grid.faces[u];
                    let rx = ux - vx;
                    let ry = uy - vy;
                    let rz = uz - vz;
                    let dx = rx * east[0] + ry * east[1] + rz * east[2];
                    let dy = rx * north[0] + ry * north[1] + rz * north[2];
                    let d2 = (dx * dx + dy * dy).max(1e-7);
                    NeighborGeom {
                        idx: u,
                        dx_east: dx,
                        dy_north: dy,
                        inv_d2: 1.0 / d2,
                    }
                })
                .collect()
        })
        .collect();

    // 3. Precompute smoothed regional continentality and its spatial gradient
    //    (east, north) pointing toward nearby landmasses (~25-35 deg scale).
    let init_land: Vec<f64> = elevations
        .iter()
        .map(|&h| if h > 0.0 { 1.0 } else { 0.0 })
        .collect();
    let smooth_land = (0..12).fold(init_land, |prev, _| {
        (0..n)
            .map(|v| {
                let sum = prev[v] + adjacency[v].iter().map(|&u| prev[u]).sum::<f64>();
                sum / ((adjacency[v].len() + 1) as f64)
            })
            .collect()
    });

    let land_grads: Vec<LandGradient> = (0..n)
        .map(|v| {
            let ngs = &neighbor_geoms[v];
            let k = ngs.len().max(1) as f64;
            let (ge, gn) = ngs.iter().fold((0.0, 0.0), |(acc_e, acc_n), ng| {
                let dl = smooth_land[ng.idx] - smooth_land[v];
                (
                    acc_e + dl * ng.dx_east * ng.inv_d2,
                    acc_n + dl * ng.dy_north * ng.inv_d2,
                )
            });
            LandGradient {
                east: ((2.0 / k) * ge * 0.35).clamp(-1.0, 1.0),
                north: ((2.0 / k) * gn * 0.35).clamp(-1.0, 1.0),
            }
        })
        .collect();

    // Equator-to-pole thermal gradient factor: stronger in glacial conditions, weaker in greenhouse.
    let wind_intensity = 1.0 + 0.25 * ice_index;
    // Latitude compression of Hadley/Ferrel cells as polar ice caps expand/contract:
    let cell_lat_compression = 1.0 + 0.16 * ice_index;

    // Helper closure to compute seasonal wind stress, Ekman drift, wind-stress curl, and sea-ice cover.
    let compute_wind_forcing = |season_val: f64| -> SeasonalWindForcing {
        let subsolar_lat_deg = -tilt_deg * season_val;
        let season_phase = if tilt_deg.abs() > 1e-6 {
            -season_val
        } else {
            0.0
        };

        let mut tau_e = Vec::with_capacity(n);
        let mut tau_n = Vec::with_capacity(n);
        let mut ekman_e = Vec::with_capacity(n);
        let mut ekman_n = Vec::with_capacity(n);
        let mut ice_cover = Vec::with_capacity(n);

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
            ice_cover.push(ice_frac);

            // ITCZ and atmospheric cell boundaries shift toward the summer hemisphere,
            // compressed toward the equator during glacial maxima.
            let LandGradient {
                east: lge,
                north: lgn,
            } = land_grads[v];
            let monsoon_pull = (season_phase * lgn).max(0.0);
            let itcz_shift_deg =
                subsolar_lat_deg * (0.42 + 0.48 * monsoon_pull) * (1.0 - 0.15 * ice_index);
            let eff_lat = ((lat - itcz_shift_deg) * cell_lat_compression).clamp(-89.5, 89.5);

            let dir_rad = wind_direction_deg(eff_lat, cells, spin) * PI / 180.0;
            let base_we = dir_rad.sin() * wind_intensity;
            let base_wn = dir_rad.cos() * wind_intensity;

            // Direct continental monsoon pressure-gradient wind:
            let hemi_summer = season_phase * ((lat + 0.4 * subsolar_lat_deg) / 20.0).tanh();
            let tropical_weight = (-((lat / 42.0).powi(2))).exp();
            let monsoon_strength = 0.65 * hemi_summer * tropical_weight;
            let lat_rad = lat * PI / 180.0;
            let cor_angle = -30.0 * (PI / 180.0) * lat_rad.sin() * spin.signum();
            let cos_c = cor_angle.cos();
            let sin_c = cor_angle.sin();
            let mon_e = lge * cos_c - lgn * sin_c;
            let mon_n = lge * sin_c + lgn * cos_c;

            // Sea-ice mechanical lid shields the surface ocean from wind stress.
            let open_water_factor = 1.0 - 0.85 * ice_frac;
            let we = (base_we + monsoon_strength * mon_e) * open_water_factor;
            let wn = (base_wn + monsoon_strength * mon_n) * open_water_factor;

            tau_e.push(we);
            tau_n.push(wn);

            // Surface Ekman deflection: ~35 deg to the right in NH (clockwise), left in SH.
            let ek_angle = -35.0 * (PI / 180.0) * lat_rad.sin() * spin.signum();
            let cos_a = ek_angle.cos();
            let sin_a = ek_angle.sin();
            ekman_e.push(we * cos_a - wn * sin_a);
            ekman_n.push(we * sin_a + wn * cos_a);
        }

        let wind_curl: Vec<f64> = (0..n)
            .map(|v| {
                if elevations[v] > 0.0 {
                    return 0.0;
                }
                let ngs = &neighbor_geoms[v];
                let k = ngs.len().max(1) as f64;
                let (dtaun_dx, dtaue_dy) =
                    ngs.iter().fold((0.0, 0.0), |(acc_nx, acc_ey), ng| {
                        let u = ng.idx;
                        (
                            acc_nx + (tau_n[u] - tau_n[v]) * ng.dx_east * ng.inv_d2,
                            acc_ey + (tau_e[u] - tau_e[v]) * ng.dy_north * ng.inv_d2,
                        )
                    });
                (2.0 / k) * (dtaun_dx - dtaue_dy)
            })
            .collect();

        SeasonalWindForcing {
            tau_e,
            tau_n,
            ekman_e,
            ekman_n,
            wind_curl,
            ice_cover,
        }
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
                    let r_b = r_b_base * (1.0 + 2.5 * ice_cover[v]);
                    let ngs = &neighbor_geoms[v];
                    let k = ngs.len().max(1) as f64;

                    let (weight_sum, neighbor_sum) =
                        ngs.iter().fold((0.0, 0.0), |(w_acc, n_acc), ng| {
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
                            (w_acc + w, n_acc + w * psi_u)
                        });

                    if weight_sum > 1e-12 {
                        let target = (neighbor_sum - wind_curl[v]) / weight_sum;
                        psi[v] = ((1.0 - omega_sor) * psi[v] + omega_sor * target)
                            * (1.0 - 0.35 * ice_cover[v]);
                    }
                }
            }
            psi
        };

    // 4. Solve the annual-mean Stommel streamfunction first (equinox baseline).
    let annual_forcing = compute_wind_forcing(0.0);
    let zero_psi = vec![0.0f64; n];
    let annual_psi = solve_stommel_sor(
        &annual_forcing.wind_curl,
        &annual_forcing.ice_cover,
        &zero_psi,
        36,
    );

    // 5. For each season, compute wind forcing, blended Stommel streamfunction,
    //    surface velocity (geostrophic + seasonal Ekman + monsoonal coastal current),
    //    and seasonal coastal upwelling.
    let mut seasonal_fields: Vec<SeasonalOceanFields> = Vec::with_capacity(num_seasons);

    for s_idx in 0..num_seasons {
        let season_val = seasons.get(s_idx).copied().unwrap_or(0.0);
        // If an earlier half-season had the exact same solar declination parameter `season_val`
        // (e.g. `DEFAULT_SEASONS = [-1.0, -0.5, 0.0, 0.5, 1.0, 0.5, 0.0, -0.5]`), reuse its
        // wind forcing and barotropic streamfunction velocity while letting the 8-half-season
        // periodic SST solver in step 7 integrate thermal inertia across all 8 half-seasons.
        if let Some(prev_idx) = (0..s_idx)
            .find(|&p| (seasons.get(p).copied().unwrap_or(0.0) - season_val).abs() < 1e-9)
        {
            seasonal_fields.push(seasonal_fields[prev_idx].clone());
            continue;
        }
        let SeasonalWindForcing {
            tau_e,
            tau_n,
            ekman_e,
            ekman_n,
            wind_curl,
            ice_cover,
        } = compute_wind_forcing(season_val);
        let eq_psi = if season_val.abs() < 1e-9 {
            annual_psi.clone()
        } else {
            solve_stommel_sor(&wind_curl, &ice_cover, &annual_psi, 18)
        };

        let psi: Vec<f64> = (0..n)
            .map(|v| {
                if elevations[v] <= 0.0 {
                    let lat = lats_deg[v];
                    let w_seas = 0.28 + 0.52 * (-((lat / 22.0).powi(2))).exp();
                    (1.0 - w_seas) * annual_psi[v] + w_seas * eq_psi[v]
                } else {
                    0.0
                }
            })
            .collect();

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
            let (raw_dx, raw_dy, has_land_neighbor, max_offshore_ekman) = ngs.iter().fold(
                (0.0, 0.0, false, 0.0f64),
                |(acc_dx, acc_dy, has_land, max_off), ng| {
                    let u = ng.idx;
                    let psi_u = if elevations[u] > 0.0 { 0.0 } else { psi[u] };
                    let next_dx = acc_dx + (psi_u - psi[v]) * ng.dx_east * ng.inv_d2;
                    let next_dy = acc_dy + (psi_u - psi[v]) * ng.dy_north * ng.inv_d2;
                    if elevations[u] > 0.0 {
                        let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                            .sqrt()
                            .max(1e-6);
                        let dir_to_land_e = ng.dx_east / d_len;
                        let dir_to_land_n = ng.dy_north / d_len;
                        let offshore = -(ekman_e[v] * dir_to_land_e + ekman_n[v] * dir_to_land_n);
                        (next_dx, next_dy, true, max_off.max(offshore))
                    } else {
                        (next_dx, next_dy, has_land, max_off)
                    }
                },
            );
            let dpsi_dx = raw_dx * (2.0 / k);
            let dpsi_dy = raw_dy * (2.0 / k);

            let lat_abs = lats_deg[v].abs();
            let trop_wind_weight = 0.25 * (-((lat_abs / 18.0).powi(2))).exp();
            let ice_damp = 1.0 - 0.80 * ice_cover[v];
            let init_ve = (-dpsi_dy + 0.35 * ekman_e[v] + trop_wind_weight * tau_e[v]) * ice_damp;
            let init_vn = (dpsi_dx + 0.35 * ekman_n[v] + trop_wind_weight * tau_n[v]) * ice_damp;

            let (ve, vn) = if has_land_neighbor {
                let (proj_ve, proj_vn) =
                    ngs.iter()
                        .fold((init_ve, init_vn), |(cur_e, cur_n), ng| {
                            if elevations[ng.idx] > 0.0 {
                                let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                                    .sqrt()
                                    .max(1e-6);
                                let nx = ng.dx_east / d_len;
                                let ny = ng.dy_north / d_len;
                                let dot = cur_e * nx + cur_n * ny;
                                if dot > 0.0 {
                                    (cur_e - dot * nx, cur_n - dot * ny)
                                } else {
                                    (cur_e, cur_n)
                                }
                            } else {
                                (cur_e, cur_n)
                            }
                        });
                let lat_weight = if lat_abs < 50.0 {
                    (lat_abs * PI / 50.0).sin().max(0.2)
                } else {
                    0.1
                };
                upwelling[v] = max_offshore_ekman * lat_weight * (1.0 + 0.20 * ice_index);
                (proj_ve, proj_vn)
            } else {
                (init_ve, init_vn)
            };

            u_east[v] = ve;
            u_north[v] = vn;
            raw_speed[v] = (ve * ve + vn * vn).sqrt();
        }

        seasonal_fields.push(SeasonalOceanFields {
            psi,
            tau_e,
            tau_n,
            u_east,
            u_north,
            raw_speed,
            upwelling,
            ice_cover,
        });
    }

    // 6. Normalize speeds globally across all seasons.
    let mut all_ocean_speeds: Vec<f64> = seasonal_fields
        .iter()
        .flat_map(|sf| sf.raw_speed.iter().copied())
        .filter(|&spd| spd > 0.0)
        .collect();
    all_ocean_speeds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let scale_speed = if all_ocean_speeds.is_empty() {
        1.0
    } else {
        let p98 = ((all_ocean_speeds.len() - 1) * 98) / 100;
        all_ocean_speeds[p98].max(1e-6)
    };

    // 7. Solve periodic seasonal Sea Surface Temperature T_SST with thermal inertia
    //    and glacial/greenhouse polar cooling/warming.
    let mut t_sst: Vec<f64> = (0..n).map(|v| (lats_deg[v] * PI / 180.0).cos()).collect();

    let s0 = if num_seasons > 2 { 2 } else { 0 };
    let sf0 = &seasonal_fields[s0];
    for _iter in 0..14 {
        let prev = t_sst.clone();
        for v in 0..n {
            if elevations[v] > 0.0 {
                continue;
            }
            let lat_rad = lats_deg[v] * PI / 180.0;
            let eq_temp = lat_rad.cos();
            let poleward_vel =
                (sf0.u_north[v] / scale_speed).clamp(-1.2, 1.2) * lat_rad.sin().signum();
            let meridional_grad = (2.0 * lat_rad.abs()).sin().max(0.25);
            let relax_rate = 0.09;
            // High-latitude glacial cooling vs greenhouse polar amplification:
            let polar_glacial_forcing =
                -0.08 * ice_index * lat_rad.sin().powi(2) - 0.10 * sf0.ice_cover[v];
            let thermal_forcing =
                0.14 * (1.0 - 0.20 * ice_index) * poleward_vel.max(0.0) * meridional_grad
                    - 0.13 * sf0.upwelling[v]
                    + polar_glacial_forcing;
            let (w_sum, val_sum) = neighbor_geoms[v].iter().fold(
                (relax_rate, relax_rate * (eq_temp + thermal_forcing)),
                |(w_acc, val_acc), ng| {
                    let u = ng.idx;
                    if elevations[u] > 0.0 {
                        return (w_acc, val_acc);
                    }
                    let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                        .sqrt()
                        .max(1e-6);
                    let vel_u_into_v =
                        -(sf0.u_east[u] * ng.dx_east + sf0.u_north[u] * ng.dy_north) / d_len;
                    let vel_v_from_u =
                        -(sf0.u_east[v] * ng.dx_east + sf0.u_north[v] * ng.dy_north) / d_len;
                    let inflow = (0.5 * (vel_u_into_v + vel_v_from_u) / scale_speed).max(0.0);
                    let w = 0.03 + 1.15 * inflow;
                    (w_acc + w, val_acc + w * prev[u])
                },
            );
            t_sst[v] = val_sum / w_sum;
        }
    }

    let mut seasonal_raw_diff: Vec<Vec<f64>> = vec![vec![0.0; n]; num_seasons];
    let cycles = if num_seasons > 1 { 2 } else { 1 };
    let sweeps_per_season = if num_seasons >= 8 {
        6
    } else if num_seasons > 1 {
        12
    } else {
        20
    };

    for _cycle in 0..cycles {
        for (s_idx, sf) in seasonal_fields.iter().enumerate() {
            let season_val = seasons.get(s_idx).copied().unwrap_or(0.0);
            let subsolar_lat_deg = -tilt_deg * season_val;

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
                        (sf.u_north[v] / scale_speed).clamp(-1.2, 1.2) * lat_rad.sin().signum();
                    let meridional_grad = (2.0 * lat_rad.abs()).sin().max(0.25);
                    let relax_rate = 0.09;
                    let polar_glacial_forcing =
                        -0.08 * ice_index * lat_rad.sin().powi(2) - 0.10 * sf.ice_cover[v];
                    let thermal_forcing = 0.15
                        * (1.0 - 0.20 * ice_index)
                        * poleward_vel.max(0.0)
                        * meridional_grad
                        - 0.14 * sf.upwelling[v]
                        + polar_glacial_forcing;

                    let inertia_weight = 0.35;
                    let init_w = inertia_weight + relax_rate;
                    let init_val =
                        inertia_weight * prev[v] + relax_rate * (t_rad_season + thermal_forcing);

                    let (w_sum, val_sum) =
                        neighbor_geoms[v]
                            .iter()
                            .fold((init_w, init_val), |(w_acc, val_acc), ng| {
                                let u = ng.idx;
                                if elevations[u] > 0.0 {
                                    return (w_acc, val_acc);
                                }
                                let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                                    .sqrt()
                                    .max(1e-6);
                                let vel_u_into_v = -(sf.u_east[u] * ng.dx_east
                                    + sf.u_north[u] * ng.dy_north)
                                    / d_len;
                                let vel_v_from_u = -(sf.u_east[v] * ng.dx_east
                                    + sf.u_north[v] * ng.dy_north)
                                    / d_len;
                                let inflow =
                                    (0.5 * (vel_u_into_v + vel_v_from_u) / scale_speed).max(0.0);
                                let w = 0.03 + 1.15 * inflow;
                                (w_acc + w, val_acc + w * prev[u])
                            });
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
    let mut pos_diffs: Vec<f64> = seasonal_raw_diff
        .iter()
        .flat_map(|row| row.iter().copied())
        .filter(|&d| d > 0.0)
        .collect();
    let mut neg_diffs: Vec<f64> = seasonal_raw_diff
        .iter()
        .flat_map(|row| row.iter().copied())
        .filter(|&d| d < 0.0)
        .map(|d| -d)
        .collect();
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
    seasonal_fields
        .into_iter()
        .enumerate()
        .map(|(s_idx, sf)| {
            let mut speed = vec![0.0f32; n];
            let mut velocity_xyz = vec![[0.0f32; 3]; n];
            for v in 0..n {
                if elevations[v] > 0.0 {
                    continue;
                }
                let norm_s = (sf.raw_speed[v] / scale_speed).min(1.0);
                speed[v] = norm_s as f32;
                if sf.raw_speed[v] > 1e-9 {
                    let ue = (sf.u_east[v] / sf.raw_speed[v]) * norm_s;
                    let un = (sf.u_north[v] / sf.raw_speed[v]) * norm_s;
                    let TangentBasis { east, north } = bases[v];
                    velocity_xyz[v] = [
                        (ue * east[0] + un * north[0]) as f32,
                        (ue * east[1] + un * north[1]) as f32,
                        (ue * east[2] + un * north[2]) as f32,
                    ];
                }
            }

            let sst: Vec<f64> = (0..n)
                .map(|v| {
                    if elevations[v] <= 0.0 {
                        let d = seasonal_raw_diff[s_idx][v];
                        if d >= 0.0 {
                            (d / pos_scale).clamp(0.0, 1.0)
                        } else {
                            (d / neg_scale).clamp(-1.0, 0.0)
                        }
                    } else {
                        0.0
                    }
                })
                .collect();

            // Diffuse marine SST anomaly onto coastal/maritime land tiles (weighted by seasonal onshore wind).
            let coastal_sst = (0..8).fold(sst.clone(), |prev, _| {
                (0..n)
                    .map(|v| {
                        if elevations[v] <= 0.0 {
                            return prev[v];
                        }
                        let (w_sum, val_sum) = neighbor_geoms[v].iter().fold(
                            (0.15, 0.0),
                            |(w_acc, val_acc), ng| {
                                let u = ng.idx;
                                let d_len = (ng.dx_east * ng.dx_east + ng.dy_north * ng.dy_north)
                                    .sqrt()
                                    .max(1e-6);
                                let wind_inflow = (-(sf.tau_e[u] * ng.dx_east
                                    + sf.tau_n[u] * ng.dy_north)
                                    / d_len)
                                    .max(0.0);
                                let w = 0.1 + 0.9 * wind_inflow;
                                (w_acc + w, val_acc + w * prev[u])
                            },
                        );
                        let elev_decay = (1.0 - (elevations[v] as f64) * 0.25).clamp(0.1, 0.88);
                        ((val_sum / w_sum) * elev_decay).clamp(-1.0, 1.0)
                    })
                    .collect()
            });

            OceanCirculation {
                streamfunction: sf.psi.into_iter().map(|p| p as f32).collect(),
                velocity_xyz,
                speed,
                sst_anomaly: sst.into_iter().map(|t| t as f32).collect(),
                coastal_sst_influence: coastal_sst.into_iter().map(|t| t as f32).collect(),
            }
        })
        .collect()
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
