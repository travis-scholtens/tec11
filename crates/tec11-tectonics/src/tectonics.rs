use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use tec11_climate::{ClimateModel, MEAN_TEMPRANGE_C};
use tec11_math_grid::{
    cross, dot, length, normal, rotate, GreatCircleArc, Pcg64Mcg, PointTree, SphericalGrid, Vec3,
};
use tec11_proto::PlanetSnapshotExt;

use crate::movement::{center_tile, farthest_tile, move_group, split_group};
use crate::pangea::PangeaWorld;
use tec11_geology::rock::{
    deposit_sedimentary, enrich_orogenic_veins_in_place, metamorphic_contact,
    metamorphic_regional_in_place, DepositClimate, ErodedMaterialBatch, Rock, RockKind, RockLayer,
};
use tec11_geology::tile::{TileGeology, TileSourceColumn};

/// A moving continental / lithospheric group (`Group` in `tile.py:17-20`).
pub use tec11_geology::tile::LithosphericGroup;

/// An active mid-ocean spreading ridge segment on the unit sphere (`Section 5.4` of the design doc).
#[derive(Clone, Debug)]
pub struct SpreadingRidgeArc {
    pub start: Vec3,
    pub end: Vec3,
    pub velocity: Vec3,
    pub age_ticks: u32,
}

/// Deep-mantle thermal plume generating ocean-island basalt (`OIB`) swells and advecting seamount/guyot chains.
#[derive(Clone, Debug)]
pub struct MantleHotspot {
    pub pos: Vec3,
    pub drift: Vec3,
    pub flux_km_per_tick: f32,
}

/// Intra-oceanic convergent subduction zone pairing a deep ocean trench (`-overriding_normal` side)
/// with a buoyant calc-alkaline island-arc ridge (`+overriding_normal` side).
#[derive(Clone, Debug)]
pub struct OceanicSubductionArc {
    pub start: Vec3,
    pub end: Vec3,
    pub overriding_normal: Vec3,
    pub convergence_speed: f32,
}

/// Classify a submarine `TileGeology` column (`elevation <= 0.0`) into its geological ocean-floor landform.
pub fn classify_submarine_tile(tile: &TileGeology) -> &'static str {
    let elev = tile.elevation();
    let thick = tile.thickness();
    let mount = tile.mountainosity;
    let top_fels = tile.top_felsity();
    let has_reef_cap = tile
        .layers
        .last()
        .is_some_and(|l| l.rock.kind == RockKind::Sedimentary && l.rock.calcity.unwrap_or(0.0) >= 0.70);
    if elev < -5.55 {
        "Oceanic Subduction Trench"
    } else if thick >= 7.2 && top_fels >= 0.54 {
        "Submarine Island-Arc Ridge"
    } else if thick >= 6.4 && has_reef_cap && elev >= -1.8 {
        "Submerged Coral Atoll / Guyot"
    } else if thick >= 6.4 && mount >= 1.2 {
        "Active Volcanic Seamount"
    } else if thick >= 6.0 {
        "Submarine Seamount / Guyot"
    } else if elev >= -1.6 {
        "Continental Shelf"
    } else if elev >= -3.6 {
        "Mid-Ocean Spreading Ridge"
    } else {
        "Abyssal Marine Basin"
    }
}

#[derive(Clone, Debug)]
struct TileMovement {
    sources: Vec<TileSourceColumn>,
    speed: f64,
}

/// Complete full-crust tectonic simulation (`PlanetSimulation` in `planetsimulation.py`
/// + Section 5.4 of the `tec11` design doc).
pub struct TectonicsSimulation {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    tree: PointTree,
    radius_km: f64,
    dp: f64,
    build_rate: f64,
    erode_rate: f64,
    splitnum: f64,
    atmosphere_ticks_remaining: u32,
    life_ticks_remaining: u32,
    total_target_ticks: u32,
    geoticks: u32,
    target_day_hours: f64,
    base_tilt_deg: f64,
    moon_count: u32,
    moon_mass: f64,
    target_moon_dist_radii: f64,
    tiles: Vec<TileGeology>,
    groups: Vec<LithosphericGroup>,
    ridges: Vec<SpreadingRidgeArc>,
    hotspots: Vec<MantleHotspot>,
    oceanic_arcs: Vec<OceanicSubductionArc>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_temp_norm: Vec<f32>,
    tile_temp_c: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    tile_impact_scars: Vec<f32>,
    meteor_freq: f64,
    impact_count: u32,
    impact_winter_ticks: u32,
    sterilization_delays: u32,
    last_impact_tile: Option<usize>,
    last_impact_diameter_km: f32,
    saved_environment: Option<tec11_proto::PlanetaryEnvironment>,
    rng: Pcg64Mcg,
}

impl TectonicsSimulation {
    /// Create a new tectonic simulation initialized with a random Pangaea supercontinent
    /// and surrounding ocean floor (`TectonicsPresenter` defaults: `grid_size = 6`, `land_pct = 29.0`,
    /// `dt = 5 Myr`, `atmdt = 125 Myr`, `lifedt = 125 Myr`, and 1 Earth-like Moon).
    pub fn new(grid_size: u32, land_percent: f64, seed: u64) -> Self {
        Self::new_with_params(grid_size, 24.0, 23.0, land_percent, 125, 125, seed)
    }

    pub fn new_with_params(
        grid_size: u32,
        day_hours: f64,
        tilt_deg: f64,
        land_percent: f64,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        seed: u64,
    ) -> Self {
        Self::new_with_full_params(
            grid_size,
            day_hours,
            tilt_deg,
            land_percent,
            atm_dt_myr,
            life_dt_myr,
            125,
            1,
            1.0,
            60.0,
            seed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_full_params(
        grid_size: u32,
        day_hours: f64,
        tilt_deg: f64,
        land_percent: f64,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        people_dt_myr: u32,
        moon_count: u32,
        moon_mass: f64,
        moon_dist_radii: f64,
        seed: u64,
    ) -> Self {
        let mut rng = Pcg64Mcg::from_seed_u64(seed);
        let pangea = PangeaWorld::generate(grid_size, land_percent, &mut rng);
        let radius_km = pangea.radius_km;
        let grid = pangea.grid;
        let adjacency = grid.adjacency();
        let n = grid.faces.len();

        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);

        let target_day_hours = day_hours.max(1.0);
        let base_tilt_deg = tilt_deg.clamp(0.0, 85.0);
        let moon_mass = if moon_count == 0 {
            0.0
        } else {
            moon_mass.max(0.0)
        };
        let target_moon_dist_radii = moon_dist_radii.clamp(15.0, 160.0);

        let atmosphere_ticks_remaining = (atm_dt_myr / 5).max(1);
        let life_ticks_remaining = (life_dt_myr / 5).max(1);
        let people_ticks = (people_dt_myr / 5).max(1);
        let total_target_ticks =
            (atmosphere_ticks_remaining + life_ticks_remaining + people_ticks).max(1);

        let spin = 24.0 / target_day_hours;
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        let dt = 5.0;
        let dp = (100.0 / radius_km) * dt;
        let build_rate = dt / 5.0;
        let erode_rate = dt;

        // Scale split threshold proportionally to planet surface area so a continent covering
        // ~9.7% of the globe (~50M km^2 on Earth, r = 6400 km) has a 50/50 chance of rifting
        // on any planet size (`planetsimulation.py:86`).
        let earth_area_km2 = 4.0 * PI * 6400.0 * 6400.0;
        let splitnum = (25.0e6 / earth_area_km2) * (n as f64);

        let groups = vec![LithosphericGroup {
            tiles: pangea.continent_tiles,
            velocity: [0.0, 0.0, 0.0],
        }];

        let ridges = pangea
            .ridge_arcs
            .into_iter()
            .map(|arc| SpreadingRidgeArc {
                start: arc.start,
                end: arc.end,
                velocity: [0.0, 0.0, 0.0],
                age_ticks: 8,
            })
            .collect();

        let hotspots = Self::generate_initial_hotspots(
            &grid,
            &pangea.tiles,
            dp,
            seed ^ 0x484F_5453_504F_5431,
        );

        let climate_model = ClimateModel::new_for_tectonics_with_adjacency(
            grid.clone(),
            adjacency.clone(),
            cells,
            spin,
            base_tilt_deg,
        );

        let mut sim = Self {
            grid,
            adjacency,
            tree,
            radius_km,
            dp,
            build_rate,
            erode_rate,
            splitnum,
            atmosphere_ticks_remaining,
            life_ticks_remaining,
            total_target_ticks,
            geoticks: 0,
            target_day_hours,
            base_tilt_deg,
            moon_count,
            moon_mass,
            target_moon_dist_radii,
            tiles: pangea.tiles,
            groups,
            ridges,
            hotspots,
            oceanic_arcs: Vec::new(),
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            tile_impact_scars: vec![0.0; n],
            meteor_freq: 1.0,
            impact_count: 0,
            impact_winter_ticks: 0,
            sterilization_delays: 0,
            last_impact_tile: None,
            last_impact_diameter_km: 0.0,
            saved_environment: None,
            rng,
        };
        sim.climate_model
            .set_spin_and_tilt_for_tectonics(sim.current_spin(), sim.current_tilt_deg());
        sim
    }

    #[inline]
    fn quantize_vec3_f32(v: Vec3) -> Vec3 {
        [
            v[0] as f32 as f64,
            v[1] as f32 as f64,
            v[2] as f32 as f64,
        ]
    }

    fn generate_initial_hotspots(
        grid: &SphericalGrid,
        tiles: &[TileGeology],
        dp: f64,
        seed: u64,
    ) -> Vec<MantleHotspot> {
        let n = grid.faces.len();
        if n == 0 {
            return Vec::new();
        }
        let target_count = ((n as f64 / 1200.0).round() as usize).clamp(2, 8);
        let mut rng = Pcg64Mcg::from_seed_u64(seed);
        let mut out = Vec::with_capacity(target_count);
        let ocean_indices: Vec<usize> = (0..n).filter(|&i| tiles[i].elevation() <= 0.0).collect();

        for idx in 0..target_count {
            let prefer_ocean = idx * 3 < target_count * 2 && !ocean_indices.is_empty();
            let mut chosen = grid.faces[0].0;
            for attempt in 0..48 {
                let t = if prefer_ocean && attempt < 36 {
                    ocean_indices[(rng.next_u64() as usize) % ocean_indices.len()]
                } else {
                    (rng.next_u64() as usize) % n
                };
                let cand = grid.faces[t].0;
                let min_cos = if attempt < 28 { 0.68 } else { 0.88 };
                if out.iter().all(|h: &MantleHotspot| dot(h.pos, cand) < min_cos) {
                    chosen = cand;
                    break;
                }
                chosen = cand;
            }
            let z = rng.uniform(-1.0, 1.0);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let r = (1.0 - z * z).sqrt();
            let rand_dir = [r * phi.cos(), r * phi.sin(), z];
            let tang = normal(cross(chosen, rand_dir));
            let wander = 0.055 * dp;
            let drift = Self::quantize_vec3_f32([
                tang[0] * wander,
                tang[1] * wander,
                tang[2] * wander,
            ]);
            let flux = rng.uniform(4.4, 6.8) as f32;
            out.push(MantleHotspot {
                pos: Self::quantize_vec3_f32(chosen),
                drift,
                flux_km_per_tick: flux,
            });
        }
        out
    }

    pub fn hotspots(&self) -> &[MantleHotspot] {
        &self.hotspots
    }

    pub fn oceanic_arcs(&self) -> &[OceanicSubductionArc] {
        &self.oceanic_arcs
    }

    /// Water-loaded Airy isostatic buoyancy lift (in km) produced by crustal rock layers above
    /// the standard `5.0 km` MORB oceanic crust baseline (`(ρ_m - ρ_c) / (ρ_m - ρ_w) ≈ 0.20..0.27`).
    pub fn ocean_excess_buoyancy_km(tile: &TileGeology) -> f32 {
        let mut baseline_rem = 5.0f32;
        let mut buoyancy = 0.0f32;
        for layer in &tile.layers {
            let t = layer.thickness;
            if t <= 0.0 {
                continue;
            }
            if baseline_rem >= t {
                baseline_rem -= t;
                continue;
            }
            let excess_t = t - baseline_rem;
            baseline_rem = 0.0;
            let f = layer.rock.felsity.clamp(0.25, 0.90);
            let c_iso = if layer.rock.kind == RockKind::Sedimentary {
                0.24
            } else {
                0.13 + 0.18 * f
            };
            buoyancy += excess_t * c_iso;
        }
        buoyancy
    }

    /// Extract crustal `RockLayer`s above the bottom `5.0 km` MORB baseline, scaled by `scale`.
    fn extract_ocean_excess_layers(tile: &TileGeology, scale: f32) -> Vec<RockLayer> {
        if scale <= 1e-4 {
            return Vec::new();
        }
        let mut baseline_rem = 5.0f32;
        let mut out = Vec::new();
        for layer in &tile.layers {
            let t = layer.thickness;
            if t <= 0.0 {
                continue;
            }
            if baseline_rem >= t {
                baseline_rem -= t;
                continue;
            }
            let excess_t = (t - baseline_rem) * scale;
            baseline_rem = 0.0;
            if excess_t > 0.005 {
                out.push(RockLayer {
                    rock: layer.rock.clone(),
                    thickness: excess_t,
                });
            }
        }
        out
    }

    /// Compute kinematic oceanic plate drift velocities `v_ocean[v]` for all ocean tiles from
    /// active spreading ridges, attached passive continental margins, and background mantle return flow.
    fn compute_ocean_velocities(&self, in_new_continent: &[bool], tile_vel: &[Vec3]) -> Vec<Vec3> {
        let n = self.tiles.len();
        let mut v_ocean = vec![[0.0, 0.0, 0.0]; n];
        let ridge_arcs: Vec<(GreatCircleArc, Vec3, f64)> = self
            .ridges
            .iter()
            .map(|r| {
                let maturity = ((r.age_ticks as f64) / 6.0).clamp(0.35, 1.0);
                (GreatCircleArc::new(r.start, r.end), r.velocity, maturity)
            })
            .collect();

        let target_speed = 0.74 * self.dp;
        let fallback_pole = normal([0.21, -0.34, 0.916]);

        for v in 0..n {
            if in_new_continent[v] {
                v_ocean[v] = tile_vel[v];
                continue;
            }
            let p_v = self.grid.faces[v].0;
            let mut sum_dir = [0.0, 0.0, 0.0];
            let mut sum_w = 0.0f64;

            for &(ref arc, r_vel, maturity) in &ridge_arcs {
                let d_km = arc.distance_to_point(p_v) * self.radius_km;
                if d_km > 5200.0 {
                    continue;
                }
                let w = maturity * (-d_km / 2200.0).exp();
                let side = if dot(p_v, arc.circle.plane) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let push_raw = [
                    arc.circle.plane[0] * side + r_vel[0] * 0.35,
                    arc.circle.plane[1] * side + r_vel[1] * 0.35,
                    arc.circle.plane[2] * side + r_vel[2] * 0.35,
                ];
                let proj = dot(push_raw, p_v);
                let tang = normal([
                    push_raw[0] - proj * p_v[0],
                    push_raw[1] - proj * p_v[1],
                    push_raw[2] - proj * p_v[2],
                ]);
                sum_dir[0] += tang[0] * w;
                sum_dir[1] += tang[1] * w;
                sum_dir[2] += tang[2] * w;
                sum_w += w;
            }

            // Blend with attached passive-margin continent velocity when directly off a trailing coast:
            for &nb in &self.adjacency[v] {
                if in_new_continent[nb] {
                    let vn = tile_vel[nb];
                    let vn_len = length(vn);
                    if vn_len > 1e-9 {
                        let p_nb = self.grid.faces[nb].0;
                        let d_hat = normal([p_v[0] - p_nb[0], p_v[1] - p_nb[1], p_v[2] - p_nb[2]]);
                        let ct = dot(vn, d_hat) / vn_len;
                        if ct <= 0.15 {
                            let proj = dot(vn, p_v);
                            let tang = normal([
                                vn[0] - proj * p_v[0],
                                vn[1] - proj * p_v[1],
                                vn[2] - proj * p_v[2],
                            ]);
                            sum_dir[0] += tang[0] * 0.65;
                            sum_dir[1] += tang[1] * 0.65;
                            sum_dir[2] += tang[2] * 0.65;
                            sum_w += 0.65;
                        }
                    }
                }
            }

            let dir = if sum_w > 0.04 && length(sum_dir) > 1e-8 {
                let proj = dot(sum_dir, p_v);
                normal([
                    sum_dir[0] - proj * p_v[0],
                    sum_dir[1] - proj * p_v[1],
                    sum_dir[2] - proj * p_v[2],
                ])
            } else {
                let bg = cross(fallback_pole, p_v);
                if length(bg) > 1e-8 {
                    normal(bg)
                } else {
                    normal(cross([1.0, 0.0, 0.0], p_v))
                }
            };
            v_ocean[v] = [
                dir[0] * target_speed,
                dir[1] * target_speed,
                dir[2] * target_speed,
            ];
        }
        v_ocean
    }

    /// Roll back active intra-oceanic subduction zones, retire zones overridden by continents or
    /// whose oceanic convergence has reversed, and initiate new zones at open-ocean convergence fronts.
    fn update_oceanic_subduction_arcs(&mut self, in_new_continent: &[bool], v_ocean: &[Vec3]) {
        let n = self.tiles.len();
        if n < 100 {
            return;
        }
        // 1. Slow trench rollback toward the subducting plate (`-overriding_normal`):
        let rollback = 0.15 * self.dp;
        for arc in &mut self.oceanic_arcs {
            let mid = normal([
                arc.start[0] + arc.end[0],
                arc.start[1] + arc.end[1],
                arc.start[2] + arc.end[2],
            ]);
            let rb_vel = [
                -arc.overriding_normal[0] * rollback,
                -arc.overriding_normal[1] * rollback,
                -arc.overriding_normal[2] * rollback,
            ];
            let axis = normal(cross(mid, rb_vel));
            if length(axis) > 1e-9 && rollback > 1e-9 {
                arc.start = Self::quantize_vec3_f32(rotate(arc.start, axis, rollback));
                arc.end = Self::quantize_vec3_f32(rotate(arc.end, axis, rollback));
                let new_mid = normal([
                    arc.start[0] + arc.end[0],
                    arc.start[1] + arc.end[1],
                    arc.start[2] + arc.end[2],
                ]);
                let raw_n = rotate(arc.overriding_normal, axis, rollback);
                let proj = dot(raw_n, new_mid);
                arc.overriding_normal = Self::quantize_vec3_f32(normal([
                    raw_n[0] - proj * new_mid[0],
                    raw_n[1] - proj * new_mid[1],
                    raw_n[2] - proj * new_mid[2],
                ]));
            }
        }

        // 2. Retire arcs overridden by continents or where convergence across the trench ceased:
        self.oceanic_arcs.retain(|arc| {
            let mid = normal([
                arc.start[0] + arc.end[0],
                arc.start[1] + arc.end[1],
                arc.start[2] + arc.end[2],
            ]);
            let s_in = in_new_continent[self.tree.nearest(arc.start)];
            let m_in = in_new_continent[self.tree.nearest(mid)];
            let e_in = in_new_continent[self.tree.nearest(arc.end)];
            if (s_in as u8 + m_in as u8 + e_in as u8) >= 2 {
                return false;
            }
            let step_ang = (240.0 / self.radius_km).clamp(0.04, 0.25);
            let sub_pt = normal([
                mid[0] - step_ang * arc.overriding_normal[0],
                mid[1] - step_ang * arc.overriding_normal[1],
                mid[2] - step_ang * arc.overriding_normal[2],
            ]);
            let ovr_pt = normal([
                mid[0] + step_ang * arc.overriding_normal[0],
                mid[1] + step_ang * arc.overriding_normal[1],
                mid[2] + step_ang * arc.overriding_normal[2],
            ]);
            let t_sub = self.tree.nearest(sub_pt);
            let t_ovr = self.tree.nearest(ovr_pt);
            if in_new_continent[t_sub] || in_new_continent[t_ovr] {
                return false;
            }
            let rel_v = [
                v_ocean[t_sub][0] - v_ocean[t_ovr][0],
                v_ocean[t_sub][1] - v_ocean[t_ovr][1],
                v_ocean[t_sub][2] - v_ocean[t_ovr][2],
            ];
            let conv = dot(rel_v, arc.overriding_normal);
            conv > -0.08 * self.dp
        });

        // 3. Initiate a new intra-oceanic subduction arc where open-ocean convergence or shear peaks:
        let max_arcs = ((n as f64 / 2000.0).round() as usize).clamp(1, 5);
        if self.oceanic_arcs.len() >= max_arcs {
            return;
        }
        let ridge_arcs: Vec<GreatCircleArc> = self
            .ridges
            .iter()
            .map(|r| GreatCircleArc::new(r.start, r.end))
            .collect();
        let existing_arcs: Vec<GreatCircleArc> = self
            .oceanic_arcs
            .iter()
            .map(|a| GreatCircleArc::new(a.start, a.end))
            .collect();

        let near_land: Vec<bool> = (0..n)
            .map(|u| {
                in_new_continent[u]
                    || self.adjacency[u].iter().any(|&nb| {
                        in_new_continent[nb]
                            || self.adjacency[nb].iter().any(|&nb2| in_new_continent[nb2])
                    })
            })
            .collect();

        let min_ridge_km = (580.0_f64).min(0.28 * self.radius_km);
        let min_arc_km = (720.0_f64).min(0.34 * self.radius_km);
        let mut best_cand: Option<(usize, Vec3, f64)> = None;

        for u in 0..n {
            if near_land[u] {
                continue;
            }
            let pu = self.grid.faces[u].0;
            if ridge_arcs
                .iter()
                .any(|ra| ra.distance_to_point(pu) * self.radius_km < min_ridge_km)
            {
                continue;
            }
            if existing_arcs
                .iter()
                .any(|ea| ea.distance_to_point(pu) * self.radius_km < min_arc_km)
            {
                continue;
            }
            for &v in &self.adjacency[u] {
                if v <= u || near_land[v] {
                    continue;
                }
                let pv = self.grid.faces[v].0;
                let d_uv = normal([pv[0] - pu[0], pv[1] - pu[1], pv[2] - pu[2]]);
                let rel_v = [
                    v_ocean[u][0] - v_ocean[v][0],
                    v_ocean[u][1] - v_ocean[v][1],
                    v_ocean[u][2] - v_ocean[v][2],
                ];
                let conv = dot(rel_v, d_uv);
                let shear_vec = cross(rel_v, d_uv);
                let shear = length(shear_vec);
                let score = conv + 0.35 * shear;
                if score > 0.22 * self.dp
                    && best_cand.as_ref().map_or(true, |&(_, _, bs)| score > bs)
                {
                    let proj = dot(d_uv, pu);
                    let ovr_n = normal([
                        d_uv[0] - proj * pu[0],
                        d_uv[1] - proj * pu[1],
                        d_uv[2] - proj * pu[2],
                    ]);
                    best_cand = Some((u, ovr_n, score));
                }
            }
        }

        if let Some((center_u, ovr_n, score)) = best_cand {
            let p_mid = self.grid.faces[center_u].0;
            if length(ovr_n) > 1e-8 {
                let half_len_rad = (680.0 / self.radius_km).clamp(0.11, 0.30);
                let start = Self::quantize_vec3_f32(rotate(p_mid, ovr_n, -half_len_rad));
                let end = Self::quantize_vec3_f32(rotate(p_mid, ovr_n, half_len_rad));
                let conv_norm = ((score / self.dp.max(1e-6)) * 0.85).clamp(0.45, 1.30) as f32;
                self.oceanic_arcs.push(OceanicSubductionArc {
                    start,
                    end,
                    overriding_normal: Self::quantize_vec3_f32(ovr_n),
                    convergence_speed: conv_norm,
                });
            }
        }
    }

    /// Tidal amplitude multiplier for `moon_count` moons in a 2:1 Laplace mean-motion
    /// orbital period resonance chain (`P_k = 2^(k-1) P_1 => a_k^3 = 4^(k-1) a_1^3`).
    /// Because tidal force scales as `M_k / a_k^3`, each successive harmonic moon out
    /// contributes `1 / 4^(k-1)` of the innermost moon's tide (`1 + 1/4 + 1/16 + ...`).
    pub fn harmonic_tidal_multiplier(moon_count: u32) -> f64 {
        let mut sum = 0.0;
        for k in 0..moon_count.min(8) {
            sum += 4.0f64.powi(-(k as i32));
        }
        sum
    }

    /// Orbital angular momentum multiplier for `moon_count` moons in a 2:1 Laplace
    /// resonance chain (`L_k ~ M_k sqrt(a_k) = M_1 sqrt(a_1) * 2^((k-1)/3)`).
    pub fn harmonic_angular_momentum_multiplier(moon_count: u32) -> f64 {
        let mut sum = 0.0;
        for k in 0..moon_count.min(8) {
            sum += 2.0f64.powf((k as f64) / 3.0);
        }
        sum
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

    /// Initial Hadean (`t = 0`) orbital distance of the innermost moon in planetary radii `R_p`,
    /// backward-calibrated via the `a^(13/2)` tidal torque recession law so that the moon
    /// reaches `target_moon_dist_radii` when Prehistory/History begins (`t = total_target_ticks`).
    pub fn initial_moon_dist_radii(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return 0.0;
        }
        let h_tide = Self::harmonic_tidal_multiplier(self.moon_count);
        let factor = (1.0 + 450.0 * self.moon_mass * h_tide).powf(2.0 / 13.0);
        (self.target_moon_dist_radii / factor).clamp(14.0, self.target_moon_dist_radii)
    }

    /// Current orbital distance of the innermost moon in planetary radii `R_p` at `geoticks`.
    pub fn current_moon_dist_radii(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            return 0.0;
        }
        let a0 = self.initial_moon_dist_radii();
        let a_tgt = self.target_moon_dist_radii;
        let tau = ((self.geoticks as f64) / (self.total_target_ticks.max(1) as f64)).clamp(0.0, 1.25);
        let a0_p = a0.powf(6.5);
        let at_p = a_tgt.powf(6.5);
        (a0_p + (at_p - a0_p) * tau).max(1.0).powf(1.0 / 6.5)
    }

    /// Current tidal amplitude relative to modern Earth (`1.0` for 1 Lunar-mass moon at `60 R_p`).
    pub fn current_tidal_strength(&self) -> f64 {
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            0.15 // Solar tide only
        } else {
            let a = self.current_moon_dist_radii().max(12.0);
            let h_tide = Self::harmonic_tidal_multiplier(self.moon_count);
            let lunar_tide = self.moon_mass * (60.0 / a).powi(3) * h_tide;
            0.15 + 0.85 * lunar_tide
        }
    }

    /// Current planetary rotation rate `spin = 24.0 / current_day_hours`, starting faster
    /// in the Hadean and braking via angular momentum transfer to the receding moon(s)
    /// so that `current_day_hours == target_day_hours` when Prehistory/History begins.
    pub fn current_spin(&self) -> f64 {
        let target_spin = 24.0 / self.target_day_hours.max(1.0);
        if self.moon_count == 0 || self.moon_mass <= 0.0 {
            target_spin
        } else {
            let a_cur = self.current_moon_dist_radii();
            let a_tgt = self.target_moon_dist_radii;
            let h_ang = Self::harmonic_angular_momentum_multiplier(self.moon_count);
            let delta_spin = 3.10
                * self.moon_mass
                * h_ang
                * ((a_tgt / 60.0).sqrt() - (a_cur / 60.0).sqrt()).max(-0.2);
            (target_spin + delta_spin).clamp(0.1, 6.0)
        }
    }

    /// Current length of day in hours at `geoticks`.
    pub fn current_day_hours(&self) -> f64 {
        24.0 / self.current_spin()
    }

    /// Current axial tilt (obliquity) in degrees at `geoticks` (Laskar, Joutel & Robutel 1993):
    /// stabilized within `+-1.2 deg` when lunar tidal torque is strong, or wandering
    /// quasi-periodically by up to `+-14 deg` on moonless/tiny-moon worlds.
    pub fn current_tilt_deg(&self) -> f64 {
        let h_tide = Self::harmonic_tidal_multiplier(self.moon_count);
        let s_obliq = if self.moon_count == 0 || self.moon_mass <= 0.0 {
            0.0
        } else {
            self.moon_mass * (60.0 / self.target_moon_dist_radii.max(15.0)).powi(3) * h_tide
        };
        let t = self.geoticks as f64;
        let wave = 0.65 * (0.19 * t).sin() + 0.35 * (0.31 * t + 1.1).cos();
        let amp = if s_obliq >= 0.25 {
            (1.2 / s_obliq.sqrt()).clamp(0.2, 2.4)
        } else {
            14.0 * (1.0 - 3.3 * s_obliq).clamp(0.2, 1.0)
        };
        (self.base_tilt_deg + amp * wave).clamp(0.0, 85.0)
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn adjacency(&self) -> &[Vec<usize>] {
        &self.adjacency
    }

    pub fn radius_km(&self) -> f64 {
        if self.radius_km > 0.0 {
            self.radius_km
        } else {
            PangeaWorld::planet_radius_km(self.grid.size)
        }
    }

    pub fn take_parts_for_prehistory(
        &mut self,
    ) -> (
        SphericalGrid,
        Vec<Vec<usize>>,
        Vec<TileGeology>,
        Vec<LithosphericGroup>,
        ClimateModel,
        f64,
        f64,
        f64,
        u32,
        Pcg64Mcg,
    ) {
        let grid_size = self.grid.size;
        let grid = std::mem::replace(&mut self.grid, SphericalGrid::new_empty(grid_size));
        let adjacency = std::mem::take(&mut self.adjacency);
        let mut tiles = std::mem::take(&mut self.tiles);
        for t in &mut tiles {
            t.normalize_for_snapshot();
        }
        let n = tiles.len();
        let mut groups = std::mem::take(&mut self.groups);
        for g in &mut groups {
            g.tiles.retain(|&id| id < n);
            g.velocity = [
                (g.velocity[0] as f32) as f64,
                (g.velocity[1] as f32) as f64,
                (g.velocity[2] as f32) as f64,
            ];
        }
        let climate_model = std::mem::replace(&mut self.climate_model, ClimateModel::empty());
        let r_f32 = self.radius_km as f32;
        let radius_km = if r_f32 > 0.0 {
            r_f32 as f64
        } else {
            PangeaWorld::planet_radius_km(grid_size)
        };
        let spin_f32 = (24.0 / self.target_day_hours.max(1.0)) as f32;
        let spin = if spin_f32 > 0.0 { spin_f32 as f64 } else { 1.0 };
        let tilt_f32 = self.base_tilt_deg as f32;
        let tilt_deg = if tilt_f32 > 0.0 {
            tilt_f32 as f64
        } else {
            23.0
        };
        let tectonics_ticks = self.geoticks;
        let rng = Pcg64Mcg::from_bytes(self.rng.to_bytes());
        self.ridges = Vec::new();
        self.tile_koeppen = Vec::new();
        self.tile_life = Vec::new();
        self.tile_temp_norm = Vec::new();
        self.tile_temp_c = Vec::new();
        self.tile_precip_norm = Vec::new();
        self.tile_impact_scars = Vec::new();
        (
            grid,
            adjacency,
            tiles,
            groups,
            climate_model,
            radius_km,
            spin,
            tilt_deg,
            tectonics_ticks,
            rng,
        )
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn geoticks(&self) -> u32 {
        self.geoticks
    }

    pub fn continent_count(&self) -> usize {
        self.groups.len()
    }

    pub fn land_percent(&self) -> u32 {
        let land_tiles = self.tiles.iter().filter(|t| t.elevation() > 0.0).count();
        let pct = 100.0 * (land_tiles as f64) / (self.tiles.len().max(1) as f64);
        (pct + 0.5) as u32
    }

    pub fn has_atmosphere(&self) -> bool {
        self.atmosphere_ticks_remaining == 0
    }

    pub fn has_life(&self) -> bool {
        self.life_ticks_remaining == 0
    }

    pub fn tiles(&self) -> &[TileGeology] {
        &self.tiles
    }

    pub fn groups(&self) -> &[LithosphericGroup] {
        &self.groups
    }

    pub fn climate_model(&self) -> &ClimateModel {
        &self.climate_model
    }

    pub fn tile_koeppen_ids(&self) -> &[u8] {
        &self.tile_koeppen
    }

    pub fn tile_life(&self) -> &[f32] {
        &self.tile_life
    }

    pub fn tile_temp_c(&self) -> &[f32] {
        &self.tile_temp_c
    }

    pub fn tile_precip_norm(&self) -> &[f32] {
        &self.tile_precip_norm
    }

    pub fn tile_impact_scars(&self) -> &[f32] {
        &self.tile_impact_scars
    }

    pub fn impact_count(&self) -> u32 {
        self.impact_count
    }

    pub fn meteor_freq(&self) -> f64 {
        self.meteor_freq
    }

    pub fn set_meteor_freq(&mut self, freq: f64) {
        self.meteor_freq = freq.clamp(0.0, 10.0);
    }

    pub fn impact_winter_ticks(&self) -> u32 {
        self.impact_winter_ticks
    }

    pub fn last_impact_tile(&self) -> Option<usize> {
        self.last_impact_tile
    }

    pub fn last_impact_diameter_km(&self) -> f32 {
        self.last_impact_diameter_km
    }

    /// Apply a bolide/asteroid impact centered at `center` with final crater diameter `crater_diameter_km`.
    /// - Excavates upper strata at `center`, shock-metamorphoses the crater floor (`hornfels`),
    ///   and extrudes an impact-melt `basalt` sheet for large basins (`>= 240 km`).
    /// - Deposits an impact-breccia `conglomerate` ejecta blanket (`clasticity = 0.96`) onto
    ///   the surrounding ring of neighbor tiles (`adjacency[center]`), raising the crater rim.
    /// - If `crater_diameter_km >= 320 km` during early pre-biotic epochs (`life_ticks_remaining in 1..=10`),
    ///   ocean-vaporizing sterilization delays `life_ticks_remaining` by up to 2 ticks.
    /// - If `has_life()` and `crater_diameter_km >= 150 km`, triggers a 1-tick impact winter
    ///   (`impact_winter_ticks = 1`) that suppresses biogenic carbonate/peat deposition.
    pub fn apply_bolide_impact(&mut self, center: usize, crater_diameter_km: f64) {
        if self.tiles.is_empty() {
            return;
        }
        let center = center.min(self.tiles.len() - 1);
        let d_km = crater_diameter_km.clamp(50.0, 1200.0);

        self.impact_count = self.impact_count.saturating_add(1);
        self.last_impact_tile = Some(center);
        self.last_impact_diameter_km = d_km as f32;
        self.tile_impact_scars[center] = 1.0;

        // 1. Excavate and shock-metamorphose the crater bullseye (`center`):
        let pre_elev = self.tiles[center].elevation();
        let excavate_km = (0.35 * (d_km / 180.0)).clamp(0.20, 1.25) as f32;
        if pre_elev > 0.0 {
            let cap = (self.tiles[center].thickness() - 1.2).max(0.0).min(excavate_km);
            if cap > 0.05 {
                let _ = self.tiles[center].erode_column(cap, 1.0, cap);
            }
            // Shock-metamorphosed hornfels crater floor (enriched in meteoritic/impact sulfides for large basins):
            let impact_sulfidity = if d_km >= 180.0 {
                Some(((d_km - 150.0) / 300.0).clamp(0.18, 0.45) as f32)
            } else {
                None
            };
            self.tiles[center].deposit(RockLayer {
                rock: Rock {
                    kind: RockKind::Metamorphic,
                    name: "hornfels",
                    felsity: 0.55,
                    toughness: 0.88,
                    clasticity: None,
                    calcity: None,
                    bogginess: None,
                    quartzity: None,
                    sulfidity: impact_sulfidity,
                    salinity: None,
                },
                thickness: 0.18,
            });
            // Large multi-ring basins (>= 240 km, e.g. Sudbury/Vredefort) extrude an impact-melt basalt sheet
            // hosting rich magmatic Ni-Cu-PGM sulfide lenses:
            if d_km >= 240.0 {
                let mut melt_sheet = Rock::extrusive(0.48);
                melt_sheet.sulfidity = Some(0.38);
                self.tiles[center].deposit(RockLayer {
                    rock: melt_sheet,
                    thickness: 0.22,
                });
            }
        // Depress the crater basin floor relative to surrounding crust while keeping inland tiles
            // above 0.08 km so interior continents never develop artificial ocean pinholes:
            let depressed_elev = (pre_elev - 0.55 * excavate_km).clamp(0.08, pre_elev.max(0.08));
            self.tiles[center].bottom = depressed_elev - self.tiles[center].thickness();
            self.tiles[center].mountainosity *= 0.35;
            self.tiles[center].limit();
        } else {
            // Marine impact: shock-metamorphose seafloor basement and deposit tsunami breccia:
            let sea_elev = pre_elev.clamp(-6.5, -0.15);
            self.tiles[center].deposit(RockLayer {
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
                thickness: 0.15,
            });
            self.tiles[center].bottom = sea_elev - self.tiles[center].thickness();
            self.tiles[center].limit();
        }
        self.tiles[center].impact_scar = 1.0;
        self.tile_impact_scars[center] = 1.0;

        // 2. Deposit volume-conserving coarse impact-breccia `conglomerate` ejecta blanket on the surrounding rim:
        let ring1 = self.adjacency[center].clone();
        let rim_thick = (excavate_km / (ring1.len().max(1) as f32)).clamp(0.04, 0.16);
        for &nb in &ring1 {
            let nb_elev = self.tiles[nb].elevation();
            if nb_elev > 0.0 {
                self.tiles[nb].deposit(RockLayer {
                    rock: Rock {
                        kind: RockKind::Sedimentary,
                        name: "conglomerate",
                        felsity: 0.58,
                        toughness: 0.55,
                        clasticity: Some(TileGeology::IMPACT_BRECCIA_CLASTICITY),
                        calcity: None,
                        bogginess: None,
                        quartzity: None,
                        sulfidity: None,
                        salinity: None,
                    },
                    thickness: rim_thick,
                });
                self.tiles[nb].isostasize(-0.85 * rim_thick);
                if self.tiles[nb].elevation() > 5.4 {
                    self.tiles[nb].bottom = 5.4 - self.tiles[nb].thickness();
                }
                self.tiles[nb].mountainosity = (self.tiles[nb].mountainosity + 0.6).min(20.0);
                self.tiles[nb].limit();
            }
            self.tiles[nb].impact_scar = self.tiles[nb].impact_scar.max(0.55);
            self.tile_impact_scars[nb] = self.tiles[nb].impact_signature();
        }

        // Multi-ring Hadean/Archean mega-basins (>= 380 km) leave an outer ejecta/shock ring:
        if d_km >= 380.0 {
            for &nb in &ring1 {
                let ring2 = self.adjacency[nb].clone();
                for &nb2 in &ring2 {
                    if nb2 != center {
                        self.tiles[nb2].impact_scar = self.tiles[nb2].impact_scar.max(0.30);
                        self.tile_impact_scars[nb2] = self.tiles[nb2].impact_signature();
                    }
                }
            }
        }

        // 3. Biosphere & Early Life Sterilization effects:
        if !self.has_life()
            && self.life_ticks_remaining > 0
            && self.life_ticks_remaining <= 10
            && d_km >= 320.0
            && self.sterilization_delays < 2
        {
            // Ocean-boiling Hadean/Archean giant impact delays nascent surface biosphere emergence:
            self.life_ticks_remaining += 2;
            self.sterilization_delays += 1;
        } else if self.has_life() && d_km >= 150.0 {
            // Chicxulub-class Phanerozoic extinction strike triggers a 1-tick impact winter:
            self.impact_winter_ticks = 1;
            self.tile_life[center] = 0.0;
            for &nb in &ring1 {
                self.tile_life[nb] = 0.0;
            }
        }
    }

    /// Advance the tectonic simulation by one 5 Myr timestep (`PlanetSimulation.update`
    /// in `planetsimulation.py:211-381` + full ocean-floor bathymetry from Section 5.4).
    pub fn step(&mut self) {
        let n = self.tiles.len();
        let _span_adv = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::TecPlateAdvectionAndSubduction);

        // 0. Clear previous tick's transient subduction & intrusion markers, and weather impact scars
        //    on the crustal columns themselves (so scars move with continental drift).
        for tile in &mut self.tiles {
            tile.clear_temp();
            if tile.elevation() > 0.0 {
                tile.impact_scar *= 0.96;
                if tile.impact_scar < 0.03 {
                    tile.impact_scar = 0.0;
                }
            } else {
                // Oceanic impacts are rapidly buried by abyssal sediment & recycled by seafloor spreading:
                tile.impact_scar *= 0.45;
                if tile.impact_scar < 0.05 {
                    tile.impact_scar = 0.0;
                }
            }
        }

        // 1. Determine tile movements for each continental group.
        let mut old_continental = vec![false; n];
        for g in &self.groups {
            for &t in &g.tiles {
                old_continental[t] = true;
            }
        }

        let mut new_movements: BTreeMap<usize, Vec<TileMovement>> = BTreeMap::new();
        let mut overlapping: Vec<Vec<usize>> = vec![Vec::new(); n];

        for i in 0..self.groups.len() {
            let speed = length(self.groups[i].velocity);
            let (moved_map, next_v) = move_group(
                &self.grid,
                &self.adjacency,
                &self.tree,
                &self.groups[i].tiles,
                self.groups[i].velocity,
            );

            let new_group_tiles: Vec<usize> = moved_map.keys().copied().collect();
            self.groups[i] = LithosphericGroup {
                tiles: new_group_tiles,
                velocity: next_v,
            };

            for (dest, src_indices) in moved_map {
                let sources: Vec<TileSourceColumn> = src_indices
                    .into_iter()
                    .map(|s| {
                        let st = &self.tiles[s];
                        TileSourceColumn {
                            bottom: st.bottom,
                            mountainosity: st.mountainosity,
                            impact_scar: st.impact_scar,
                            layers: st.layers.clone(),
                        }
                    })
                    .collect();
                new_movements
                    .entry(dest)
                    .or_default()
                    .push(TileMovement { sources, speed });
                overlapping[dest].push(i);
            }
        }

        // Close any 1-to-3 tile inter-plate pinholes completely enclosed by `new_movements`
        // (and any 1-tile `>= deg - 1` pinhole) without inflating `group.tiles` at rifts.
        let mut visited_non_land = vec![false; n];
        for start in 0..n {
            if visited_non_land[start] || new_movements.contains_key(&start) {
                continue;
            }
            let mut comp = Vec::new();
            let mut q = std::collections::VecDeque::new();
            visited_non_land[start] = true;
            q.push_back(start);
            while let Some(u) = q.pop_front() {
                comp.push(u);
                for &nb in &self.adjacency[u] {
                    if !visited_non_land[nb] && !new_movements.contains_key(&nb) {
                        visited_non_land[nb] = true;
                        q.push_back(nb);
                    }
                }
            }
            if comp.len() <= 3 {
                // Fill this tiny 1..=3 tile enclosed pocket from its surrounding `new_movements` neighbors
                for _pass in 0..3 {
                    for &h in &comp {
                        if new_movements.contains_key(&h) {
                            continue;
                        }
                        let nb_in_cont: Vec<usize> = self.adjacency[h]
                            .iter()
                            .copied()
                            .filter(|&nb| new_movements.contains_key(&nb))
                            .collect();
                        if !nb_in_cont.is_empty() {
                            let mut group_counts: BTreeMap<usize, usize> = BTreeMap::new();
                            for &nb in &nb_in_cont {
                                for &g_idx in &overlapping[nb] {
                                    *group_counts.entry(g_idx).or_insert(0) += 1;
                                }
                            }
                            if let Some((&best_g, _)) = group_counts.iter().max_by_key(|&(_, c)| c) {
                                let mut sources = Vec::new();
                                for &nb in &nb_in_cont {
                                    if let Some(ms) = new_movements.get(&nb) {
                                        if let Some(first_m) = ms.first() {
                                            if let Some(first_src) = first_m.sources.first() {
                                                sources.push(first_src.clone());
                                            }
                                        }
                                    }
                                }
                                if !sources.is_empty() {
                                    let speed = length(self.groups[best_g].velocity);
                                    new_movements
                                        .entry(h)
                                        .or_default()
                                        .push(TileMovement { sources, speed });
                                    overlapping[h].push(best_g);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut in_new_continent = vec![false; n];
        let mut tile_vel = vec![[0.0, 0.0, 0.0]; n];
        for &dest in new_movements.keys() {
            in_new_continent[dest] = true;
            let g_indices = &overlapping[dest];
            if !g_indices.is_empty() {
                let mut sum_v = [0.0, 0.0, 0.0];
                for &g_idx in g_indices {
                    let gv = self.groups[g_idx].velocity;
                    sum_v[0] += gv[0];
                    sum_v[1] += gv[1];
                    sum_v[2] += gv[2];
                }
                let inv = 1.0 / (g_indices.len() as f64);
                tile_vel[dest] = [sum_v[0] * inv, sum_v[1] * inv, sum_v[2] * inv];
            }
        }

        // 1b. Kinematic oceanic drift, intra-oceanic subduction arc rollback/initiation, and
        //     advection of thickened oceanic crust (seamounts, guyots, atolls, and submarine island arcs):
        let v_ocean = self.compute_ocean_velocities(&in_new_continent, &tile_vel);
        self.update_oceanic_subduction_arcs(&in_new_continent, &v_ocean);

        let sub_arcs: Vec<(GreatCircleArc, Vec3, f32)> = self
            .oceanic_arcs
            .iter()
            .map(|a| {
                (
                    GreatCircleArc::new(a.start, a.end),
                    a.overriding_normal,
                    a.convergence_speed,
                )
            })
            .collect();

        let mut advected_ocean_excess: Vec<Vec<RockLayer>> = vec![Vec::new(); n];
        let mut advected_ocean_mount = vec![0.0f32; n];
        let mut accreted_onto_continent: Vec<Vec<RockLayer>> = vec![Vec::new(); n];
        let rollback_speed = 0.15 * self.dp;

        for s in 0..n {
            if old_continental[s] {
                continue;
            }
            let src_tile = &self.tiles[s];
            if src_tile.thickness() <= 5.05 && src_tile.mountainosity <= 0.05 {
                continue;
            }
            // Do not carry unweathered oceanic impact-breccia piles endlessly across basins:
            if src_tile.impact_scar == 0.0 && src_tile.stratigraphic_impact_signature() > 0.0 {
                continue;
            }
            let p_s = self.grid.faces[s].0;

            // Check if `s` sits in the trench or overriding volcanic belt of an `OceanicSubductionArc`:
            let mut arc_zone: Option<(usize, f64)> = None;
            for (ai, &(ref gca, ovr_n, _)) in sub_arcs.iter().enumerate() {
                let d_km = gca.distance_to_point(p_s) * self.radius_km;
                if d_km <= 340.0 {
                    let sign = if dot(p_s, gca.circle.plane) >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let signed_km = sign * d_km;
                    let _ = ovr_n;
                    if arc_zone.map_or(true, |(_, prev)| d_km < prev.abs()) {
                        arc_zone = Some((ai, signed_km));
                    }
                }
            }

            if let Some((ai, signed_km)) = arc_zone {
                let ovr_n = sub_arcs[ai].1;
                if (-220.0..35.0).contains(&signed_km) {
                    // Subducting oceanic trench: incoming seamounts & marine sediments are scraped off
                    // the downgoing slab onto the overriding island-arc front (~175 km across the trench):
                    let step_ang = ((175.0 - signed_km).max(90.0) / self.radius_km).clamp(0.03, 0.22);
                    let axis = normal(cross(p_s, ovr_n));
                    let dest = if length(axis) > 1e-9 {
                        self.tree.nearest(rotate(p_s, axis, step_ang))
                    } else {
                        s
                    };
                    let scraped = Self::extract_ocean_excess_layers(src_tile, 0.60);
                    if in_new_continent[dest] {
                        accreted_onto_continent[dest].extend(scraped);
                    } else {
                        advected_ocean_excess[dest].extend(scraped);
                        advected_ocean_mount[dest] += src_tile.mountainosity * 0.45;
                    }
                    continue;
                } else if (35.0..=340.0).contains(&signed_km) {
                    // Overriding island-arc belt: moves coherently with slow trench rollback so
                    // calc-alkaline arc crust piles up until Airy buoyancy lifts it to/above sea level:
                    let rb_vel = [
                        -ovr_n[0] * rollback_speed,
                        -ovr_n[1] * rollback_speed,
                        -ovr_n[2] * rollback_speed,
                    ];
                    let axis = normal(cross(p_s, rb_vel));
                    let dest = if length(axis) > 1e-9 && rollback_speed > 1e-9 {
                        self.tree.nearest(rotate(p_s, axis, rollback_speed))
                    } else {
                        s
                    };
                    let kept = Self::extract_ocean_excess_layers(src_tile, 0.97);
                    if in_new_continent[dest] {
                        accreted_onto_continent[dest].extend(kept);
                    } else {
                        advected_ocean_excess[dest].extend(kept);
                        advected_ocean_mount[dest] =
                            advected_ocean_mount[dest].max(src_tile.mountainosity * 0.88);
                    }
                    continue;
                }
            }

            // Open-ocean plate drift: advect seamount/guyot column along `v_ocean[s]`:
            let vo = v_ocean[s];
            let speed = length(vo);
            let dest = if speed > 1e-9 {
                let axis = normal(cross(p_s, vo));
                if length(axis) > 1e-9 {
                    self.tree.nearest(rotate(p_s, axis, speed))
                } else {
                    s
                }
            } else {
                s
            };
            if in_new_continent[dest] {
                let scraped = Self::extract_ocean_excess_layers(src_tile, 0.55);
                accreted_onto_continent[dest].extend(scraped);
            } else {
                let drifted = Self::extract_ocean_excess_layers(src_tile, 0.94);
                advected_ocean_excess[dest].extend(drifted);
                // Wave planation & submarine flank slumping erode drifted cones into flat-topped guyots:
                let decayed_m = if dest == s {
                    src_tile.mountainosity * 0.84
                } else {
                    src_tile.mountainosity * 0.72
                };
                advected_ocean_mount[dest] = advected_ocean_mount[dest].max(decayed_m);
            }
        }

        // 2. Apply tile movements, directional leading-edge subduction uplift, and accretionary
        //    sediment + seamount/island-arc terrane scraping.
        let mut collisions: BTreeMap<(usize, usize), usize> = BTreeMap::new();

        for (dest, movements) in new_movements {
            let is_leading_edge = !old_continental[dest];
            let avg_speed: f64 =
                movements.iter().map(|m| m.speed).sum::<f64>() / (movements.len() as f64);
            let speed_norm = (avg_speed / self.dp.max(1e-6)).clamp(0.0, 1.6);
            let v_vec = tile_vel[dest];
            let v_len = length(v_vec);
            let p_dest = self.grid.faces[dest].0;
            let mut cos_theta_max = 0.0f64;
            let mut has_ocean_nb = false;
            if v_len > 1e-9 {
                let v_hat = [v_vec[0] / v_len, v_vec[1] / v_len, v_vec[2] / v_len];
                for &nb in &self.adjacency[dest] {
                    if !in_new_continent[nb] {
                        has_ocean_nb = true;
                        let p_nb = self.grid.faces[nb].0;
                        let d_hat = normal([
                            p_nb[0] - p_dest[0],
                            p_nb[1] - p_dest[1],
                            p_nb[2] - p_dest[2],
                        ]);
                        cos_theta_max = cos_theta_max.max(dot(v_hat, d_hat));
                    }
                }
            }
            let raw_build = if has_ocean_nb && is_leading_edge && cos_theta_max > 0.20 {
                (1.60 * self.build_rate * speed_norm * cos_theta_max) as f32
            } else if has_ocean_nb && cos_theta_max > 0.45 {
                (0.45 * self.build_rate * speed_norm * cos_theta_max) as f32
            } else {
                0.0
            };

            // Scrape off marine sedimentary layers AND any overridden oceanic seamount / island-arc
            // terrane layers before replacing `dest` with the advancing continental column:
            let mut scraped_sediments: Vec<RockLayer> = Vec::new();
            let mut subducted_seafloor = self.tiles[dest]
                .layers
                .iter()
                .rev()
                .find(|l| l.rock.kind == RockKind::Igneous)
                .map(|l| l.rock.clone())
                .unwrap_or_else(Rock::seafloor);
            if is_leading_edge {
                let mut slab_carbon = 0.0f32;
                let mut had_marine_sed = false;
                for l in &self.tiles[dest].layers {
                    if l.rock.kind == RockKind::Sedimentary {
                        had_marine_sed = true;
                        let sed_c = l
                            .rock
                            .bogginess
                            .unwrap_or(0.0)
                            .max(0.24 * l.rock.calcity.unwrap_or(0.0));
                        slab_carbon = slab_carbon.max(sed_c);
                        scraped_sediments.push(l.clone());
                    }
                }
                // Also accrete up to 0.85 km of overridden seamount/arc volcanic edifice into the wedge:
                let mut terrane_rem = 0.85f32;
                for mut tl in Self::extract_ocean_excess_layers(&self.tiles[dest], 0.30)
                    .into_iter()
                    .chain(std::mem::take(&mut accreted_onto_continent[dest]))
                {
                    if tl.rock.kind == RockKind::Sedimentary {
                        continue;
                    }
                    if terrane_rem <= 0.02 {
                        break;
                    }
                    let take = tl.thickness.min(terrane_rem);
                    tl.thickness = take;
                    terrane_rem -= take;
                    scraped_sediments.push(tl);
                }
                if had_marine_sed {
                    let existing_c = subducted_seafloor.bogginess.unwrap_or(0.0);
                    subducted_seafloor.bogginess =
                        Some((existing_c.max(slab_carbon) + 0.04).clamp(0.04, 0.55));
                }
            }

            let group_sources: Vec<Vec<TileSourceColumn>> =
                movements.into_iter().map(|m| m.sources).collect();
            self.tiles[dest].merge_sources_owned(group_sources);

            for sed in scraped_sediments {
                let is_igneous_terrane = sed.rock.kind != RockKind::Sedimentary;
                let st = sed.thickness;
                self.tiles[dest].deposit(sed);
                if is_igneous_terrane {
                    // Subduction uplift is already accounted for in `raw_build`; compensate root so
                    // accreted terranes enrich Cordilleran stratigraphy without double-counting elevation:
                    self.tiles[dest].isostasize(-st);
                }
            }

            if raw_build > 0.0 {
                let cur_h = self.tiles[dest].elevation().max(0.0);
                let max_sub_h = if is_leading_edge { 6.8 } else { 6.2 };
                let taper = ((max_sub_h - cur_h) / 5.8).clamp(0.0, 1.0);
                let build_amount = raw_build * taper;
                if build_amount > 0.01 {
                    self.tiles[dest].build(build_amount, subducted_seafloor);
                }
            }

            // Record pairwise collisions between overlapping groups at `dest`.
            let ov = &overlapping[dest];
            for a_idx in 0..ov.len() {
                for b_idx in (a_idx + 1)..ov.len() {
                    let g1 = ov[a_idx].min(ov[b_idx]);
                    let g2 = ov[a_idx].max(ov[b_idx]);
                    *collisions.entry((g1, g2)).or_insert(0) += 1;
                }
            }
        }

        drop(_span_adv);
        let _span_ridge = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::TecRidgeAndOceanBathymetry);

        // 3. Mid-ocean ridges, mantle hotspot plumes, intra-oceanic subduction arcs, and
        //    water-loaded Airy-isostatic ocean bathymetry:
        for ridge in &mut self.ridges {
            let speed = length(ridge.velocity);
            if speed > 1e-9 {
                let mid = normal([
                    ridge.start[0] + ridge.end[0],
                    ridge.start[1] + ridge.end[1],
                    ridge.start[2] + ridge.end[2],
                ]);
                let axis = normal(cross(mid, ridge.velocity));
                if length(axis) > 1e-9 {
                    ridge.start = rotate(ridge.start, axis, speed);
                    ridge.end = rotate(ridge.end, axis, speed);
                }
            }
            ridge.age_ticks = ridge.age_ticks.saturating_add(1);
        }

        self.ridges.retain(|r| {
            if r.age_ticks <= 6 {
                return true;
            }
            let mid = normal([
                r.start[0] + r.end[0],
                r.start[1] + r.end[1],
                r.start[2] + r.end[2],
            ]);
            let s_in = in_new_continent[self.tree.nearest(r.start)];
            let m_in = in_new_continent[self.tree.nearest(mid)];
            let e_in = in_new_continent[self.tree.nearest(r.end)];
            !(s_in && m_in && e_in)
        });

        // 3a. Rebuild each ocean tile's advected column (`5.0 km` MORB seafloor + advected seamount/arc layers):
        for v in 0..n {
            if in_new_continent[v] {
                continue;
            }
            let preserved_scar = if old_continental[v] {
                0.0
            } else {
                self.tiles[v].impact_scar
            };
            let mut base_tile = TileGeology::empty_ocean_with_depth(Rock::seafloor(), -4.5);
            base_tile.impact_scar = preserved_scar;
            let mut excess_sum = 0.0f32;
            for mut layer in std::mem::take(&mut advected_ocean_excess[v]) {
                if excess_sum >= 24.0 {
                    break;
                }
                let take = layer.thickness.min(24.0 - excess_sum);
                if take > 0.005 {
                    layer.thickness = take;
                    base_tile.layers.push(layer);
                    excess_sum += take;
                }
            }
            base_tile.mountainosity = advected_ocean_mount[v].clamp(0.0, 14.0);
            base_tile.compact();
            self.tiles[v] = base_tile;
        }

        // 3b. Mantle Hotspot Plume Volcanism (Ocean-Island Basalt shield edifices + continental flood traps):
        for hotspot in &mut self.hotspots {
            let d_speed = length(hotspot.drift);
            if d_speed > 1e-9 {
                let axis = normal(cross(hotspot.pos, hotspot.drift));
                if length(axis) > 1e-9 {
                    let next_p = Self::quantize_vec3_f32(rotate(hotspot.pos, axis, d_speed));
                    let next_d = rotate(hotspot.drift, axis, d_speed);
                    let proj = dot(next_d, next_p);
                    let tang = normal([
                        next_d[0] - proj * next_p[0],
                        next_d[1] - proj * next_p[1],
                        next_d[2] - proj * next_p[2],
                    ]);
                    hotspot.pos = next_p;
                    hotspot.drift = Self::quantize_vec3_f32([
                        tang[0] * d_speed,
                        tang[1] * d_speed,
                        tang[2] * d_speed,
                    ]);
                }
            }
            let h_tile = self.tree.nearest(hotspot.pos);
            let flux = hotspot.flux_km_per_tick;
            let mut oib = Rock::extrusive(0.44);
            oib.sulfidity = Some(0.10);
            if !in_new_continent[h_tile] {
                let cur_excess = (self.tiles[h_tile].thickness() - 5.0).max(0.0);
                let add_main = (flux * ((22.0 - cur_excess) / 18.0).clamp(0.20, 1.0)).max(0.0);
                if add_main > 0.05 {
                    self.tiles[h_tile].layers.push(RockLayer {
                        rock: oib.clone(),
                        thickness: add_main,
                    });
                    self.tiles[h_tile].mountainosity =
                        (self.tiles[h_tile].mountainosity + 4.4).min(14.0);
                    self.tiles[h_tile].compact();
                }
                let flank_add = 0.22 * flux;
                for &nb in &self.adjacency[h_tile] {
                    if !in_new_continent[nb] && self.tiles[nb].thickness() < 16.0 {
                        self.tiles[nb].layers.push(RockLayer {
                            rock: oib.clone(),
                            thickness: flank_add,
                        });
                        self.tiles[nb].mountainosity =
                            (self.tiles[nb].mountainosity + 0.85).min(10.0);
                        self.tiles[nb].compact();
                    }
                }
            } else if self.tiles[h_tile].thickness() < 32.0
                && self.tiles[h_tile].impact_signature() < 0.05
                && (self.geoticks + (h_tile as u32)) % 8 == 0
            {
                // Occasional Flood Basalt / Large Igneous Province trap eruption through thin continental crust:
                let trap_t = (0.14 * flux).clamp(0.15, 0.75);
                self.tiles[h_tile].deposit(RockLayer {
                    rock: oib,
                    thickness: trap_t,
                });
                self.tiles[h_tile].isostasize(-trap_t);
            }
        }

        // 3c. Intra-Oceanic Subduction Arc Magmatism (calc-alkaline andesite/dacite + porphyry Cu-Au):
        for &(ref gca, _, conv_speed) in &sub_arcs {
            let mut arc_rock = Rock::intrusive(0.58); // "andesite"
            arc_rock.sulfidity = Some(0.22);
            arc_rock.quartzity = Some(0.15);
            for v in 0..n {
                if in_new_continent[v] {
                    continue;
                }
                let p_v = self.grid.faces[v].0;
                let d_km = gca.distance_to_point(p_v) * self.radius_km;
                if !(50.0..=320.0).contains(&d_km) {
                    continue;
                }
                if dot(p_v, gca.circle.plane) <= 0.0 {
                    continue; // Subducting trench side — no arc magmatism
                }
                // Bell-shaped magmatic front peaking ~175 km behind the trench axis:
                let u = ((d_km - 175.0) / 135.0).clamp(-1.0, 1.0);
                let profile = (1.0 - u * u).max(0.0) as f32;
                let cur_excess = (self.tiles[v].thickness() - 5.0).max(0.0);
                let taper = ((24.0 - cur_excess) / 18.0).clamp(0.15, 1.0);
                let add_arc = 2.05 * profile * conv_speed * taper;
                if add_arc > 0.05 {
                    self.tiles[v].layers.push(RockLayer {
                        rock: arc_rock.clone(),
                        thickness: add_arc,
                    });
                    self.tiles[v].mountainosity =
                        (self.tiles[v].mountainosity + 2.2 * profile).min(15.0);
                    self.tiles[v].subduction = self.tiles[v].subduction.max(0.55 * profile);
                    self.tiles[v].compact();
                }
            }
        }

        // 3d. Water-loaded Airy-isostatic ocean bathymetry + emergent island-arc / ridge-plume graduation:
        let active_ridges: Vec<(GreatCircleArc, f64)> = self
            .ridges
            .iter()
            .map(|r| {
                let maturity = ((r.age_ticks as f64) / 6.0).clamp(0.25, 1.0);
                (GreatCircleArc::new(r.start, r.end), maturity)
            })
            .collect();

        let mut cur_land_count = in_new_continent.iter().filter(|&&b| b).count();
        let max_land_count = ((n as f64) * 0.46) as usize;
        let mut newly_emergent: Vec<usize> = Vec::new();

        for v in 0..n {
            if in_new_continent[v] {
                continue;
            }

            let p_v = self.grid.faces[v].0;
            let mut z_ridge = -5.5f64;
            let mut ridge_vent_intensity = 0.0f32;
            for &(ref arc, maturity) in &active_ridges {
                let d_km = arc.distance_to_point(p_v) * self.radius_km;
                let u = (d_km / 2600.0).clamp(0.0, 1.0);
                let z_arc = -5.5 + maturity * 3.0 * (1.0 - u.sqrt());
                if z_arc > z_ridge {
                    z_ridge = z_arc;
                }
                if d_km < 650.0 {
                    let vent = (maturity * (1.0 - d_km / 650.0)) as f32;
                    if vent > ridge_vent_intensity {
                        ridge_vent_intensity = vent;
                    }
                }
            }

            // Dynamic thermal plume swell (~1.35 km uplift over active hotspot heads, subsiding downstream):
            let mut z_plume_swell = 0.0f64;
            for hotspot in &self.hotspots {
                let ang = dot(p_v, hotspot.pos).clamp(-1.0, 1.0).acos();
                let d_km = ang * self.radius_km;
                let swell_r = (380.0_f64).max(0.10 * self.radius_km);
                if d_km < swell_r {
                    let q = 1.0 - d_km / swell_r;
                    z_plume_swell = z_plume_swell.max(1.35 * q * q);
                }
            }

            // Intra-oceanic subduction trench flexural depression on subducting side (`-250..+35 km`):
            let mut z_intra_trench = 0.0f64;
            for &(ref gca, _, conv_speed) in &sub_arcs {
                let d_km = gca.distance_to_point(p_v) * self.radius_km;
                if d_km <= 250.0 {
                    let signed_km = if dot(p_v, gca.circle.plane) >= 0.0 {
                        d_km
                    } else {
                        -d_km
                    };
                    if (-250.0..=35.0).contains(&signed_km) {
                        let u = ((signed_km + 65.0) / 170.0).clamp(-1.0, 1.0);
                        let dep =
                            1.35 * (1.0 - u * u).max(0.0) * (conv_speed as f64).clamp(0.4, 1.2);
                        z_intra_trench = z_intra_trench.max(dep);
                    }
                }
            }

            let mut has_cont_nb = false;
            let mut cos_theta_max = -1.0f64;
            let mut max_sub = 0.0f64;
            for &nb in &self.adjacency[v] {
                if in_new_continent[nb] {
                    has_cont_nb = true;
                    let v_nb = tile_vel[nb];
                    let v_len = length(v_nb);
                    if v_len > 1e-9 {
                        let v_hat = [v_nb[0] / v_len, v_nb[1] / v_len, v_nb[2] / v_len];
                        let p_nb = self.grid.faces[nb].0;
                        let d_hat = normal([
                            p_v[0] - p_nb[0],
                            p_v[1] - p_nb[1],
                            p_v[2] - p_nb[2],
                        ]);
                        let ct = dot(v_hat, d_hat);
                        if ct > cos_theta_max {
                            cos_theta_max = ct;
                        }
                        if ct > 0.25 {
                            max_sub = max_sub.max(self.tiles[nb].subduction as f64);
                        }
                    } else {
                        cos_theta_max = cos_theta_max.max(0.0);
                    }
                }
            }

            let open_base = (z_ridge + z_plume_swell - z_intra_trench).clamp(-6.8, -1.05);
            let buoyancy_km = Self::ocean_excess_buoyancy_km(&self.tiles[v]);

            let raw_elev = if !has_cont_nb {
                (open_base as f32) + buoyancy_km
            } else if cos_theta_max > 0.35 && max_sub > 0.0 {
                // Active convergent subduction trench ahead of an advancing continental margin:
                let trench_deflection = 1.35 * cos_theta_max * max_sub.clamp(0.25, 1.2);
                ((z_ridge - trench_deflection).clamp(-6.8, -4.8) as f32) + 0.35 * buoyancy_km
            } else if cos_theta_max <= 0.15 {
                // Passive / trailing continental shelf:
                -1.15f32 + 0.45 * buoyancy_km
            } else {
                // Oblique coastal margin: smooth transition between passive shelf and open ocean:
                let w = ((cos_theta_max - 0.15) / 0.20).clamp(0.0, 1.0);
                (((-1.15 * (1.0 - w)) + open_base * w) as f32) + 0.65 * buoyancy_km
            };

            // Physical emergence test (pure elevation + accumulated crustal thickness — no age timer):
            // Only thicken-crust mature island arcs or ridge-centered volcanic plateaus (`>= 19.5 km` crust)
            // breach `0.0 km` on the coarse grid, provided doing so doesn't enclose a tiny ocean pinhole.
            let can_emerge = raw_elev > 0.08
                && self.tiles[v].thickness() >= 19.5
                && cur_land_count < max_land_count
                && self.adjacency[v]
                    .iter()
                    .filter(|&&nb| !in_new_continent[nb])
                    .count()
                    >= 3;

            if can_emerge {
                let em_elev = raw_elev.clamp(0.10, 1.35);
                self.tiles[v].bottom = em_elev - self.tiles[v].thickness();
                newly_emergent.push(v);
                cur_land_count += 1;
            } else {
                let sub_elev = raw_elev.clamp(-6.85, -0.08);
                self.tiles[v].bottom = sub_elev - self.tiles[v].thickness();

                // Darwinian coral-atoll reef limestone cap on shallow tropical/subtropical seamounts & arcs:
                if self.has_atmosphere()
                    && self.has_life()
                    && (-0.95..=-0.08).contains(&sub_elev)
                    && self.tiles[v].thickness() >= 7.0
                    && self.tile_temp_c[v] >= 19.0
                {
                    let reef = Rock {
                        kind: RockKind::Sedimentary,
                        name: "limestone",
                        felsity: 0.45,
                        toughness: 0.0,
                        clasticity: Some(1.0),
                        calcity: Some(0.88),
                        bogginess: Some(0.05),
                        quartzity: None,
                        sulfidity: None,
                        salinity: None,
                    };
                    self.tiles[v].layers.push(RockLayer {
                        rock: reef,
                        thickness: 0.05,
                    });
                    self.tiles[v].compact();
                    self.tiles[v].bottom = sub_elev - self.tiles[v].thickness();
                }
            }

            // Black-smoker hydrothermal venting at active mid-ocean spreading ridge axes
            // precipitates Volcanogenic Massive Sulfide (VMS) lenses and siliceous sinter caps:
            if ridge_vent_intensity > 0.01 && !can_emerge {
                if let Some(top_igneous) = self.tiles[v]
                    .layers
                    .iter_mut()
                    .rev()
                    .find(|l| l.rock.kind == RockKind::Igneous)
                {
                    let s = (top_igneous.rock.sulfidity.unwrap_or(0.08)
                        + 0.09 * ridge_vent_intensity)
                        .clamp(0.0, 0.55);
                    let q = (top_igneous.rock.quartzity.unwrap_or(0.0)
                        + 0.04 * ridge_vent_intensity)
                        .clamp(0.0, 0.30);
                    top_igneous.rock.sulfidity = Some(s);
                    top_igneous.rock.quartzity = Some(q);
                }
            }
        }

        // 3e. Graduate newly emergent volcanic plateaus & mature island arcs into `LithosphericGroup`s
        //     (skipping any candidate whose emergence would trap a 1..=3 tile interior ocean pinhole):
        if !newly_emergent.is_empty() {
            for &cand in &newly_emergent {
                in_new_continent[cand] = true;
            }
            // Verify no 1..=3 tile ocean pinhole was created; revert any offending candidate to -0.08 km:
            let mut visited_oc = vec![false; n];
            let mut revert_tiles = Vec::new();
            for start in 0..n {
                if visited_oc[start] || in_new_continent[start] {
                    continue;
                }
                let mut comp = Vec::new();
                let mut q = std::collections::VecDeque::new();
                visited_oc[start] = true;
                q.push_back(start);
                while let Some(u) = q.pop_front() {
                    comp.push(u);
                    for &nb in &self.adjacency[u] {
                        if !visited_oc[nb] && !in_new_continent[nb] {
                            visited_oc[nb] = true;
                            q.push_back(nb);
                        }
                    }
                }
                if comp.len() <= 3 {
                    for &hole in &comp {
                        for &nb in &self.adjacency[hole] {
                            if newly_emergent.contains(&nb) {
                                revert_tiles.push(nb);
                            }
                        }
                    }
                }
            }
            for &rv in &revert_tiles {
                in_new_continent[rv] = false;
                self.tiles[rv].bottom = -0.08 - self.tiles[rv].thickness();
            }
            newly_emergent.retain(|v| in_new_continent[*v]);

            let em_set: BTreeSet<usize> = newly_emergent.iter().copied().collect();
            let mut handled = BTreeSet::new();
            for &start in &newly_emergent {
                if handled.contains(&start) {
                    continue;
                }
                let mut comp = Vec::new();
                let mut q = std::collections::VecDeque::new();
                handled.insert(start);
                q.push_back(start);
                while let Some(u) = q.pop_front() {
                    comp.push(u);
                    for &nb in &self.adjacency[u] {
                        if em_set.contains(&nb) && !handled.contains(&nb) {
                            handled.insert(nb);
                            q.push_back(nb);
                        }
                    }
                }
                // Attach to adjacent existing continent group if touching one; otherwise form an island-arc microplate:
                let mut adj_g: Option<usize> = None;
                for &u in &comp {
                    for &nb in &self.adjacency[u] {
                        if !em_set.contains(&nb) && !overlapping[nb].is_empty() {
                            adj_g = Some(overlapping[nb][0]);
                            break;
                        }
                    }
                    if adj_g.is_some() {
                        break;
                    }
                }
                if let Some(g_idx) = adj_g {
                    for &u in &comp {
                        if !self.groups[g_idx].tiles.contains(&u) {
                            self.groups[g_idx].tiles.push(u);
                        }
                        overlapping[u].push(g_idx);
                    }
                } else {
                    let mut sv = [0.0, 0.0, 0.0];
                    for &u in &comp {
                        sv[0] += v_ocean[u][0];
                        sv[1] += v_ocean[u][1];
                        sv[2] += v_ocean[u][2];
                    }
                    let inv = 1.0 / (comp.len() as f64);
                    let g_vel = Self::quantize_vec3_f32([sv[0] * inv, sv[1] * inv, sv[2] * inv]);
                    let new_g = self.groups.len();
                    for &u in &comp {
                        overlapping[u].push(new_g);
                    }
                    self.groups.push(LithosphericGroup {
                        tiles: comp,
                        velocity: g_vel,
                    });
                }
            }
        }

        drop(_span_ridge);

        // 4. Record each continent's pre-erosion total above-sea height (`planetsimulation.py:273`).
        let pre_erosion_heights: Vec<f32> = self
            .groups
            .iter()
            .map(|g| {
                g.tiles
                    .iter()
                    .map(|&t| self.tiles[t].elevation().max(0.0))
                    .sum()
            })
            .collect();

        // 5. Simulate coupled climate and erosion if the atmosphere has formed (`planetsimulation.py:275-315`).
        if self.has_atmosphere() {
            let has_life = self.has_life() && self.impact_winter_ticks == 0;
            {
                let _span_clim = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::TecClimateAndOceanCirculation);
                self.refresh_climate_fields();
            }
            let _span_erode = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::TecErosionAndDeposition);
            if self.impact_winter_ticks > 0 {
                for l in &mut self.tile_life {
                    *l *= 0.25;
                }
                self.impact_winter_ticks -= 1;
            }

            // Build downhill erosion graph (`erodemethod.py:21-38` with physical local base-level bounds):
            // Each land tile (elevation > 0) erodes only into strictly lower neighbors (`h_u < h_v`),
            // weighted by the elevation drop, and capped at half the relief above its lowest neighbor
            // (and above sea level 0.05 km) so rivers never carve pits below their own outlet.
            let mut outgoing_dests: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
            let mut incoming_sources: Vec<Vec<usize>> = vec![Vec::new(); n];
            let mut max_erosion_cap: Vec<f32> = vec![0.0; n];
            let tidal_factor = self.current_tidal_strength().sqrt().clamp(0.2, 3.5) as f32;

            for v in 0..n {
                let elev_v = self.tiles[v].elevation();
                if elev_v > 0.05 {
                    let base_climate = self.tile_precip_norm[v]
                        * (1.0 - self.tile_temp_norm[v]).max(0.0);
                    let high_relief_weathering = if elev_v > 2.2 {
                        0.08 * ((elev_v - 2.2) / 6.0).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let climate_factor = (base_climate + high_relief_weathering).max(0.0);
                    let has_ocean_nb = self.adjacency[v]
                        .iter()
                        .any(|&u| self.tiles[u].elevation() <= 0.0);
                    if climate_factor > 0.0 || has_ocean_nb {
                        let mut min_nb_elev = elev_v;
                        for &u in &self.adjacency[v] {
                            let raw_nb_elev = self.tiles[u].elevation();
                            let nb_elev = raw_nb_elev.max(0.0);
                            if nb_elev < elev_v {
                                min_nb_elev = min_nb_elev.min(nb_elev);
                                let drop = elev_v - nb_elev;
                                let tidal_scour = if raw_nb_elev <= 0.0 {
                                    0.008 * tidal_factor
                                } else {
                                    0.0
                                };
                                let d = (0.34 * (elev_v + drop) / 10.0) * climate_factor
                                    + tidal_scour;
                                if d > 0.0 {
                                    outgoing_dests[v].push((u, d));
                                    incoming_sources[u].push(v);
                                }
                            }
                        }
                        if min_nb_elev < elev_v {
                            let base_level = min_nb_elev.max(0.05);
                            max_erosion_cap[v] = (0.38 * (elev_v - base_level)).max(0.0);
                        }
                    }
                }
            }

            let mut incoming_batches: Vec<Vec<ErodedMaterialBatch>> = vec![Vec::new(); n];
            let erode_dt = self.erode_rate as f32;

            for v in 0..n {
                if outgoing_dests[v].is_empty() || max_erosion_cap[v] <= 1e-6 {
                    continue;
                }
                let total_degree: f32 = outgoing_dests[v].iter().map(|&(_, d)| d).sum();
                if total_degree <= 1e-7 {
                    continue;
                }
                let avg_degree = total_degree / (outgoing_dests[v].len().max(1) as f32);
                let (total_eroded, stripped_layers) =
                    self.tiles[v].erode_column(avg_degree, erode_dt, max_erosion_cap[v]);
                if total_eroded > 1e-6 && !stripped_layers.is_empty() {
                    // Local Airy isostatic root rebound: removing crustal mass from an eroding
                    // mountain unloads its root locally (`60%` rebound of bottom, `40%` net lowering
                    // of surface elevation), exhuming deep metamorphic/plutonic rocks without
                    // jacking up un-eroded lowland tiles across the rest of the continent.
                    self.tiles[v].isostasize(0.60 * total_eroded);

                    // Distribute eroded material across lower neighbors proportional to slope:
                    // - On land neighbors, 35% is deposited locally in terrestrial basins while 65%
                    //   is exported to the sea as dissolved/suspended fluvial load.
                    // - On coastal ocean neighbors, the full share enters the coastal sea.
                    let dest_len = outgoing_dests[v].len();
                    let mut stripped_opt = Some(stripped_layers);
                    for (idx, &(dest_u, deg_u)) in outgoing_dests[v].iter().enumerate() {
                        let share = total_eroded * (deg_u / total_degree);
                        let retention = if !overlapping[dest_u].is_empty() {
                            0.35
                        } else {
                            1.0
                        };
                        let amount_u = share * retention;
                        let substance = if idx + 1 == dest_len {
                            stripped_opt.take().unwrap()
                        } else {
                            stripped_opt.as_ref().unwrap().clone()
                        };
                        incoming_batches[dest_u].push(ErodedMaterialBatch {
                            amount: amount_u,
                            total: total_eroded,
                            substance,
                        });
                    }
                }
            }

            for t in 0..n {
                if incoming_batches[t].is_empty() {
                    continue;
                }
                let clim_summary = DepositClimate {
                    temperature_c: self.tile_temp_c[t],
                    precipitation_norm: self.tile_precip_norm[t],
                    life: self.tile_life[t],
                };

                if !overlapping[t].is_empty() {
                    let mut dep = deposit_sedimentary(
                        &incoming_batches[t],
                        has_life,
                        false,
                        Some(clim_summary),
                    );
                    // Endorheic (landlocked) basins with no downhill river outlet concentrate
                    // dissolved runoff into evaporite salt pans / lithium brines when warm & dry:
                    if outgoing_dests[t].is_empty()
                        && clim_summary.temperature_c > 5.0
                        && clim_summary.precipitation_norm < 0.45
                    {
                        let endorheic_sal = ((0.45 - clim_summary.precipitation_norm) * 1.2)
                            .clamp(0.10, 0.50);
                        dep.rock.salinity = Some(
                            (dep.rock.salinity.unwrap_or(0.0) + endorheic_sal).clamp(0.0, 1.0),
                        );
                    }
                    let dep_t = dep.thickness;
                    self.tiles[t].deposit(dep);
                    // Local Airy basin subsidence under the weight of newly deposited sediment:
                    self.tiles[t].isostasize(-0.75 * dep_t);
                } else {
                    let total_amount: f32 = incoming_batches[t].iter().map(|b| b.amount).sum();
                    let adj_continent_count = self.adjacency[t]
                        .iter()
                        .filter(|&&nb| !overlapping[nb].is_empty())
                        .count();
                    if total_amount > 1.5 && adj_continent_count >= 3 {
                        // Prograde continent into coastal sea when deposition exceeds 1.5 km (`planetsimulation.py:301-310`)
                        let dep = deposit_sedimentary(
                            &incoming_batches[t],
                            has_life,
                            true,
                            Some(clim_summary),
                        );
                        self.tiles[t].deposit(dep);
                        if self.tiles[t].elevation() <= 0.05 {
                            self.tiles[t].bottom = (total_amount - 1.4).max(0.1) - self.tiles[t].thickness();
                        }

                        let mut group_counts: BTreeMap<usize, usize> = BTreeMap::new();
                        for &src_v in &incoming_sources[t] {
                            for &g_idx in &overlapping[src_v] {
                                *group_counts.entry(g_idx).or_insert(0) += 1;
                            }
                        }
                        if let Some((&best_g, _)) = group_counts.iter().max_by_key(|&(_, c)| c) {
                            if !self.groups[best_g].tiles.contains(&t) {
                                self.groups[best_g].tiles.push(t);
                            }
                            overlapping[t].push(best_g);
                        }
                    } else if adj_continent_count >= 2
                        && total_amount > 0.08
                        && (!has_life || self.life_ticks_remaining > 0)
                    {
                        // Pre-biotic and Great Oxidation Era continental shelves precipitate
                        // Banded Iron Formation (BIF) marine shelf beds (`sulfidity` + `quartzity`),
                        // which later prograde onto cratons or get scraped into accretionary wedges:
                        let mut shelf_dep = deposit_sedimentary(
                            &incoming_batches[t],
                            false,
                            true,
                            Some(clim_summary),
                        );
                        shelf_dep.thickness = (shelf_dep.thickness * 0.25).clamp(0.03, 0.25);
                        let new_top = self.tiles[t].elevation() + shelf_dep.thickness;
                        self.tiles[t].deposit(shelf_dep);
                        if new_top > -0.15 {
                            self.tiles[t].bottom = -0.15 - self.tiles[t].thickness();
                        }
                    }
                }
            }

            if self.life_ticks_remaining > 0 {
                self.life_ticks_remaining -= 1;
            }
        } else {
            self.atmosphere_ticks_remaining -= 1;
        }

        // 6. Craton buoyancy safeguard & post-orogenic denudation (`planetsimulation.py:318-321`):
        //    - Local Airy root rebound and basin subsidence are applied per-tile during erosion above.
        //    - Ancient inactive mountain belts (`!is_active_orogen` and not adjacent to an active
        //      orogenic front) slowly relax/denude toward lowland craton elevations (`2.0 km`).
        for (i, group) in self.groups.iter().enumerate() {
            if group.tiles.is_empty() {
                continue;
            }
            let n_tiles = group.tiles.len() as f32;
            let pre_h = pre_erosion_heights.get(i).copied().unwrap_or(0.0);
            let post_h: f32 = group
                .tiles
                .iter()
                .map(|&t| self.tiles[t].elevation().max(0.0))
                .sum();
            let pre_mean = pre_h / n_tiles;
            let post_mean = post_h / n_tiles;
            let eroded_loss = (pre_mean - post_mean).max(0.0);
            let dh = if post_mean < 0.45 {
                (0.45 - post_mean).min(eroded_loss.max(0.05))
            } else {
                0.0
            };

            for &t in &group.tiles {
                if dh > 0.0 {
                    self.tiles[t].isostasize(dh);
                }
                let is_active_orogen =
                    self.tiles[t].subduction > 0.0 || overlapping[t].len() > 1;
                if !is_active_orogen {
                    let elev = self.tiles[t].elevation();
                    if elev > 1.3 {
                        let relax = 0.048 * (elev - 1.3);
                        self.tiles[t].isostasize(-relax);
                    }
                    self.tiles[t].mountainosity = (self.tiles[t].mountainosity * 0.92).max(0.0);
                }
                // Keep continental tiles emergent above sea level
                if self.tiles[t].elevation() < 0.05 {
                    self.tiles[t].bottom = 0.05 - self.tiles[t].thickness();
                }
            }
        }

        let _span_meta = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::TecMetamorphismAndRifting);
        // 7. Random plutonic intrusions and contact metamorphism on continental arcs (`planetsimulation.py:325-329`).
        for t in 0..n {
            if in_new_continent[t] && self.tiles[t].subduction > 0.0 && self.rng.next_f64() < 0.1 {
                let silica = self.rng.gauss(0.85, 0.15).clamp(0.0, 1.0) as f32;
                self.tiles[t].intrude(Rock::intrusive(silica));
                let transformed =
                    metamorphic_contact(&self.tiles[t].layers, self.tiles[t].intrusion);
                self.tiles[t].transform(transformed);
            }
        }

        // 8. Regional metamorphism (`planetsimulation.py:333-335`) & orogenic quartz-vein enrichment.
        for t in 0..n {
            let sub = self.tiles[t].subduction > 0.0;
            metamorphic_regional_in_place(&mut self.tiles[t].layers, sub);
            let is_collision = overlapping[t].len() > 1;
            if sub || is_collision {
                let orogenic_intensity = if sub && is_collision {
                    1.0
                } else if is_collision {
                    0.85
                } else {
                    self.tiles[t].subduction.clamp(0.35, 1.0)
                };
                enrich_orogenic_veins_in_place(&mut self.tiles[t].layers, orogenic_intensity);
            }
            self.tiles[t].compact();
        }

        // 8b. Deep-mantle kimberlite / lamproite pipe eruptions punching through thick,
        // non-subducting continental interiors, sampling deep eclogite/amphibolite carbon into surface diamonds.
        let kimb_seed = u128::from_le_bytes(self.rng.to_bytes())
            ^ ((self.geoticks as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835));
        let mut kimb_rng = Pcg64Mcg::new(kimb_seed | 1);
        for t in 0..n {
            if overlapping[t].len() == 1
                && self.tiles[t].subduction == 0.0
                && self.tiles[t].elevation() > 0.05
            {
                let thick = self.tiles[t].thickness();
                if thick >= 22.0 {
                    let prob = 0.0065 * (((thick - 20.0) / 30.0).clamp(0.25, 1.5) as f64);
                    if kimb_rng.next_f64() < prob {
                        let _ = self.tiles[t].erupt_kimberlite();
                    }
                }
            }
        }

        // 9. Merge overlapping continent shapes (`planetsimulation.py:342-370`).
        let mut merge_clusters: Vec<BTreeSet<usize>> = Vec::new();
        for (&(g1, g2), &count) in &collisions {
            let min_len = self.groups[g1].tiles.len().min(self.groups[g2].tiles.len());
            if count > min_len / 10 {
                let mut found = None;
                for (c_idx, cluster) in merge_clusters.iter_mut().enumerate() {
                    if cluster.contains(&g1) || cluster.contains(&g2) {
                        cluster.insert(g1);
                        cluster.insert(g2);
                        found = Some(c_idx);
                        break;
                    }
                }
                if found.is_none() {
                    let mut set = BTreeSet::new();
                    set.insert(g1);
                    set.insert(g2);
                    merge_clusters.push(set);
                }
            }
        }

        if !merge_clusters.is_empty() {
            let mut to_remove = BTreeSet::new();
            for cluster in merge_clusters {
                let largest = *cluster
                    .iter()
                    .max_by_key(|&&idx| self.groups[idx].tiles.len())
                    .unwrap();
                let mut combined_tiles: BTreeSet<usize> =
                    self.groups[largest].tiles.iter().copied().collect();
                let largest_len = self.groups[largest].tiles.len() as f64;
                let mut weighted_v = [
                    self.groups[largest].velocity[0] * largest_len,
                    self.groups[largest].velocity[1] * largest_len,
                    self.groups[largest].velocity[2] * largest_len,
                ];

                for &other in &cluster {
                    if other != largest && !to_remove.contains(&other) {
                        let o_len = self.groups[other].tiles.len() as f64;
                        weighted_v[0] += self.groups[other].velocity[0] * o_len;
                        weighted_v[1] += self.groups[other].velocity[1] * o_len;
                        weighted_v[2] += self.groups[other].velocity[2] * o_len;
                        combined_tiles.extend(self.groups[other].tiles.iter().copied());
                        to_remove.insert(other);
                    }
                }

                let total_len = combined_tiles.len().max(1) as f64;
                self.groups[largest].tiles = combined_tiles.into_iter().collect();
                self.groups[largest].velocity = [
                    weighted_v[0] / total_len,
                    weighted_v[1] / total_len,
                    weighted_v[2] / total_len,
                ];
            }

            let mut kept_groups = Vec::with_capacity(self.groups.len());
            for (idx, g) in self.groups.drain(..).enumerate() {
                if !to_remove.contains(&idx) {
                    kept_groups.push(g);
                }
            }
            self.groups = kept_groups;
        }

        // 10. Randomly split large continent shapes (`planetsimulation.py:374-377`).
        let min_splittable_len = (self.splitnum * 0.35).ceil() as usize;
        let mut next_groups = Vec::with_capacity(self.groups.len() + 2);
        for g in self.groups.drain(..) {
            let g_len = g.tiles.len();
            if g_len >= min_splittable_len.max(24)
                && self.rng.uniform(0.0, 1.0) > self.splitnum / (g_len as f64)
            {
                if let Some(((a_tiles, va), (b_tiles, vb))) =
                    split_group(&self.grid, &self.adjacency, &g.tiles, &mut self.rng)
                {
                    // Spawn a new mid-ocean spreading ridge along the rift seam between `a_tiles` and `b_tiles`.
                    let b_set: BTreeSet<usize> = b_tiles.iter().copied().collect();
                    let seam_tiles: Vec<usize> = a_tiles
                        .iter()
                        .copied()
                        .filter(|&u| self.adjacency[u].iter().any(|nb| b_set.contains(nb)))
                        .collect();
                    if seam_tiles.len() >= 2 {
                        let m_tile = center_tile(&self.grid, &seam_tiles);
                        let p_m = self.grid.faces[m_tile].0;
                        let e1_tile = farthest_tile(&self.grid, &seam_tiles, p_m);
                        let p_e1 = self.grid.faces[e1_tile].0;
                        let e2_tile = farthest_tile(&self.grid, &seam_tiles, p_e1);
                        let p_e2 = self.grid.faces[e2_tile].0;
                        if e1_tile != m_tile && m_tile != e2_tile {
                            self.ridges.push(SpreadingRidgeArc {
                                start: p_e1,
                                end: p_m,
                                velocity: g.velocity,
                                age_ticks: 0,
                            });
                            self.ridges.push(SpreadingRidgeArc {
                                start: p_m,
                                end: p_e2,
                                velocity: g.velocity,
                                age_ticks: 0,
                            });
                        } else if e1_tile != e2_tile {
                            self.ridges.push(SpreadingRidgeArc {
                                start: p_e1,
                                end: p_e2,
                                velocity: g.velocity,
                                age_ticks: 0,
                            });
                        }
                        while self.ridges.len() > 18 {
                            self.ridges.remove(0);
                        }
                    }

                    let dp = self.dp;
                    let max_speed = 1.35 * dp;
                    let mut va_full = [
                        g.velocity[0] + va[0] * dp,
                        g.velocity[1] + va[1] * dp,
                        g.velocity[2] + va[2] * dp,
                    ];
                    let sa = length(va_full);
                    if sa > max_speed {
                        let scale = max_speed / sa;
                        va_full = [va_full[0] * scale, va_full[1] * scale, va_full[2] * scale];
                    }
                    let mut vb_full = [
                        g.velocity[0] + vb[0] * dp,
                        g.velocity[1] + vb[1] * dp,
                        g.velocity[2] + vb[2] * dp,
                    ];
                    let sb = length(vb_full);
                    if sb > max_speed {
                        let scale = max_speed / sb;
                        vb_full = [vb_full[0] * scale, vb_full[1] * scale, vb_full[2] * scale];
                    }
                    next_groups.push(LithosphericGroup {
                        tiles: a_tiles,
                        velocity: va_full,
                    });
                    next_groups.push(LithosphericGroup {
                        tiles: b_tiles,
                        velocity: vb_full,
                    });
                    continue;
                }
            }
            next_groups.push(g);
        }
        self.groups = next_groups;

        // 11. Stochastic meteor / bolide impacts (Hadean-Archean Late Heavy Bombardment decay
        //     followed by steady Phanerozoic Chicxulub/Sudbury-scale bolide flux), scaled by `meteor_freq`:
        if self.meteor_freq > 0.0 && n > 0 {
            let base_p = if self.geoticks < 45 {
                0.28 * (-(self.geoticks as f64) / 16.0).exp()
            } else {
                0.055
            };
            let mut rem = base_p * self.meteor_freq;
            let mut impact_rng = Pcg64Mcg::from_seed_u64(
                (self.geoticks as u64)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add((n as u64).wrapping_mul(0x517C_C1B7_2722_0A95)),
            );
            while rem > 0.0 {
                let p_impact = rem.min(1.0);
                if impact_rng.next_f64() < p_impact {
                    let center = (impact_rng.next_u64() as usize) % n;
                    let u = impact_rng.next_f64();
                    let crater_d_km = if self.geoticks < 45 {
                        160.0 + 420.0 * u * u
                    } else {
                        140.0 + 180.0 * u * u
                    };
                    self.apply_bolide_impact(center, crater_d_km);
                }
                rem -= 1.0;
            }
        }

        // Synchronize `tile_impact_scars` with the advected/weathered `TileGeology` columns:
        for v in 0..n {
            self.tile_impact_scars[v] = self.tiles[v].impact_signature();
        }

        self.geoticks += 1;
    }

    /// Find the tile index nearest to a 3D unit vector in grid coordinates.
    pub fn nearest_tile(&self, vec_grid: Vec3) -> usize {
        self.tree.nearest( normal(vec_grid) )
    }

    /// Format a multi-line stratigraphic inspection report for `tile_idx`
    /// (surface to Moho, matching `TectonicsPresenter.selecttile` in `tectonicspresenter.py:45-53`).
    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        let Some(tile) = self.tiles.get(tile_idx) else {
            return String::new();
        };
        let mut lines = Vec::new();
        let elev = tile.elevation();
        let thick = tile.thickness();
        let kind_str = if elev > 0.0 { "Land" } else { "Ocean" };
        let koeppen_str = if self.has_atmosphere() && elev > 0.0 {
            let code = match self.tile_koeppen[tile_idx] {
                1 => "Af",
                2 => "Am",
                3 => "Aw",
                4 => "BS",
                5 => "BW",
                6 => "Cf",
                7 => "Cs",
                8 => "Cw",
                9 => "Df",
                10 => "Ds",
                11 => "Dw",
                12 => "EF",
                13 => "ET",
                _ => "Ocean",
            };
            format!(" | Köppen: {code}")
        } else {
            String::new()
        };

        lines.push(format!(
            "Tile #{tile_idx} ({kind_str}) | Elev: {elev:+.2} km | Crust: {thick:.1} km{koeppen_str}"
        ));
        if elev <= 0.0 {
            lines.push(format!(
                "Submarine Landform: {} | Relief: {:.0}%",
                classify_submarine_tile(tile),
                tile.mountainosity_norm() * 100.0
            ));
        }
        if tile.subduction > 0.0 {
            lines.push(format!(
                "Active Subduction: {:.2} km | Mountainosity: {:.0}%",
                tile.subduction,
                tile.mountainosity_norm() * 100.0
            ));
        }
        let scar = self.tile_impact_scars[tile_idx];
        if scar > 0.05 {
            let zone = if scar > 0.60 {
                "Impact Crater Basin (Shocked Hornfels / Melt)"
            } else if scar > 0.32 {
                "Impact Ejecta Rim (Breccia Conglomerate)"
            } else {
                "Weathered Impact Ring"
            };
            lines.push(format!("Meteor Scar: {zone} (intensity {:.0}%)", scar * 100.0));
        }

        // Layers from surface (top) down to Moho (bottom):
        let mut depth_from_surface = 0.0f32;
        for (idx, layer) in tile.layers.iter().enumerate().rev() {
            let r = &layer.rock;
            let mut attrs = vec![
                format!("type: {}", r.kind.code()),
                format!("felsity: {:.2}", r.felsity),
                format!("tough: {:.2}", r.toughness),
            ];
            if let Some(c) = r.clasticity {
                attrs.push(format!("clast: {c:.1}"));
            }
            if let Some(c) = r.calcity {
                attrs.push(format!("calc: {c:.2}"));
            }
            if let Some(b) = r.bogginess {
                attrs.push(format!("bog: {b:.2}"));
            }
            if let Some(q) = r.quartzity {
                attrs.push(format!("quartz: {q:.2}"));
            }
            if let Some(s) = r.sulfidity {
                attrs.push(format!("sulf: {s:.2}"));
            }
            if let Some(s) = r.salinity {
                attrs.push(format!("salt: {s:.2}"));
            }
            let deposits = tile.derive_layer_deposits_with_elev(idx, depth_from_surface, elev);
            depth_from_surface += layer.thickness;
            let dep_suffix = if deposits.is_empty() {
                String::new()
            } else {
                format!(" → {}", deposits.join(" + "))
            };
            lines.push(format!(
                "{} ({:.2} km) [{}]{dep_suffix}",
                r.name,
                layer.thickness,
                attrs.join(", ")
            ));
            if idx > 0 {
                let intf =
                    tile.derive_interface_occurrences_with_elev(idx - 1, depth_from_surface, elev);
                if !intf.is_empty() {
                    let labels: Vec<&str> = intf.iter().map(|o| o.label.as_str()).collect();
                    lines.push(format!(
                        "  ┄ Interface ({depth_from_surface:.2} km): {}",
                        labels.join(" + ")
                    ));
                }
            }
        }

        lines.join("\n")
    }

    pub fn inspect_tile_stratigraphy_json(&self, tile_idx: usize) -> String {
        let Some(tile) = self.tiles.get(tile_idx) else {
            return "{\"hasColumn\":false}".to_string();
        };
        tile.to_stratigraphy_json(tile_idx, None)
    }

    pub fn saved_environment(&self) -> Option<&tec11_proto::PlanetaryEnvironment> {
        self.saved_environment.as_ref()
    }

    pub fn set_saved_environment(&mut self, env: Option<tec11_proto::PlanetaryEnvironment>) {
        self.saved_environment = env;
    }

    fn refresh_climate_fields(&mut self) {
        self.climate_model
            .set_spin_and_tilt_for_tectonics(self.current_spin(), self.current_tilt_deg());
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        let has_life = self.has_life();
        self.climate_model
            .update_for_tectonics(&land_elevs, has_life);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            let temp_norm = c.mean_temperature as f32;
            self.tile_temp_norm[v] = temp_norm;
            self.tile_temp_c[v] = MEAN_TEMPRANGE_C.to_celsius(temp_norm as f64) as f32;
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }
    }

    /// Convert the current tectonic simulation state into a `tec11_proto::PlanetSnapshot`
    /// (`Section 7` of the design document).
    pub fn to_snapshot(&self) -> tec11_proto::PlanetSnapshot {
        let proto_tiles: Vec<tec11_proto::GeologicalTile> = self
            .tiles
            .iter()
            .map(TileGeology::to_proto_geological_tile)
            .collect();

        let proto_groups: Vec<tec11_proto::LithosphericGroup> = self
            .groups
            .iter()
            .map(|g| tec11_proto::LithosphericGroup {
                tile_ids: g.tiles.iter().map(|&id| id as u32).collect(),
                velocity: Some(tec11_proto::Vec3 {
                    x: g.velocity[0] as f32,
                    y: g.velocity[1] as f32,
                    z: g.velocity[2] as f32,
                }),
            })
            .collect();

        let proto_ridges: Vec<tec11_proto::SpreadingRidgeArc> = self
            .ridges
            .iter()
            .map(|r| tec11_proto::SpreadingRidgeArc {
                start: Some(tec11_proto::Vec3 {
                    x: r.start[0] as f32,
                    y: r.start[1] as f32,
                    z: r.start[2] as f32,
                }),
                end: Some(tec11_proto::Vec3 {
                    x: r.end[0] as f32,
                    y: r.end[1] as f32,
                    z: r.end[2] as f32,
                }),
                velocity: Some(tec11_proto::Vec3 {
                    x: r.velocity[0] as f32,
                    y: r.velocity[1] as f32,
                    z: r.velocity[2] as f32,
                }),
                age_ticks: r.age_ticks,
            })
            .collect();

        let proto_hotspots: Vec<tec11_proto::MantleHotspot> = self
            .hotspots
            .iter()
            .map(|h| tec11_proto::MantleHotspot {
                pos: Some(tec11_proto::Vec3 {
                    x: h.pos[0] as f32,
                    y: h.pos[1] as f32,
                    z: h.pos[2] as f32,
                }),
                drift: Some(tec11_proto::Vec3 {
                    x: h.drift[0] as f32,
                    y: h.drift[1] as f32,
                    z: h.drift[2] as f32,
                }),
                flux_km_per_tick: h.flux_km_per_tick,
            })
            .collect();

        let proto_oceanic_arcs: Vec<tec11_proto::OceanicSubductionArc> = self
            .oceanic_arcs
            .iter()
            .map(|a| tec11_proto::OceanicSubductionArc {
                start: Some(tec11_proto::Vec3 {
                    x: a.start[0] as f32,
                    y: a.start[1] as f32,
                    z: a.start[2] as f32,
                }),
                end: Some(tec11_proto::Vec3 {
                    x: a.end[0] as f32,
                    y: a.end[1] as f32,
                    z: a.end[2] as f32,
                }),
                overriding_normal: Some(tec11_proto::Vec3 {
                    x: a.overriding_normal[0] as f32,
                    y: a.overriding_normal[1] as f32,
                    z: a.overriding_normal[2] as f32,
                }),
                convergence_speed: a.convergence_speed,
            })
            .collect();

        let env_out = self.saved_environment.as_ref().map(|e| {
            let mut env = e.clone();
            env.has_atmosphere = self.has_atmosphere();
            env.has_life = self.has_life();
            env.moon_count = self.moon_count;
            env.moon_mass_lunar = self.moon_mass as f32;
            env.current_moon_dist_radii = self.current_moon_dist_radii() as f32;
            env.target_moon_dist_radii = self.target_moon_dist_radii.round().max(15.0) as u32;
            env
        });

        tec11_proto::PlanetSnapshot {
            rng_state_16b: self.rng.to_bytes().to_vec(),
            grid_size: self.grid.size,
            radius_km: self.radius_km as f32,
            spin: (24.0 / self.target_day_hours.max(1.0)) as f32,
            tilt_deg: self.base_tilt_deg as f32,
            stage: Some(tec11_proto::PlanetStageOneof::Tectonics(
                tec11_proto::TectonicsStage {
                    tectonics_ticks: self.geoticks,
                    atmosphere_ticks_remaining: self.atmosphere_ticks_remaining,
                    life_ticks_remaining: self.life_ticks_remaining,
                    people_ticks_remaining: self.total_target_ticks,
                    tiles: proto_tiles,
                    groups: proto_groups,
                    ridges: proto_ridges,
                    hotspots: proto_hotspots,
                    oceanic_arcs: proto_oceanic_arcs,
                },
            )),
            environment: env_out,
        }
    }

    /// Encode the current tectonic simulation state to `.tec11` binary Protocol Buffer bytes.
    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.to_snapshot().encode_bytes()
    }

    /// Reconstruct a `TectonicsSimulation` from a `tec11_proto::PlanetSnapshot`.
    pub fn from_snapshot(snap: &tec11_proto::PlanetSnapshot, fallback_seed: u64) -> Option<Self> {
        let grid_size = snap.grid_size;
        if !(2..=7).contains(&grid_size) {
            return None;
        }
        let grid = SphericalGrid::with_size(grid_size);
        let n = grid.faces.len();
        let adjacency = grid.adjacency();

        let (
            tiles,
            mut groups,
            ridges,
            hotspots,
            oceanic_arcs,
            geoticks,
            atmosphere_ticks_remaining,
            life_ticks_remaining,
            total_target_ticks,
        ) = match snap.stage.as_ref()? {
            tec11_proto::PlanetStageOneof::Tectonics(tec) => {
                if tec.tiles.len() != n {
                    return None;
                }
                let tiles: Vec<TileGeology> = tec
                    .tiles
                    .iter()
                    .map(TileGeology::from_proto_geological_tile)
                    .collect::<Option<Vec<_>>>()?;
                let groups: Vec<LithosphericGroup> = tec
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                let ridges: Vec<SpreadingRidgeArc> = tec
                    .ridges
                    .iter()
                    .map(|pr| {
                        let s = pr.start.unwrap_or_default();
                        let e = pr.end.unwrap_or_default();
                        let v = pr.velocity.unwrap_or_default();
                        SpreadingRidgeArc {
                            start: [s.x as f64, s.y as f64, s.z as f64],
                            end: [e.x as f64, e.y as f64, e.z as f64],
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                            age_ticks: pr.age_ticks,
                        }
                    })
                    .collect();
                let hotspots: Vec<MantleHotspot> = tec
                    .hotspots
                    .iter()
                    .map(|ph| {
                        let p = ph.pos.unwrap_or_default();
                        let d = ph.drift.unwrap_or_default();
                        MantleHotspot {
                            pos: [p.x as f64, p.y as f64, p.z as f64],
                            drift: [d.x as f64, d.y as f64, d.z as f64],
                            flux_km_per_tick: ph.flux_km_per_tick,
                        }
                    })
                    .collect();
                let oceanic_arcs: Vec<OceanicSubductionArc> = tec
                    .oceanic_arcs
                    .iter()
                    .map(|pa| {
                        let s = pa.start.unwrap_or_default();
                        let e = pa.end.unwrap_or_default();
                        let on = pa.overriding_normal.unwrap_or_default();
                        OceanicSubductionArc {
                            start: [s.x as f64, s.y as f64, s.z as f64],
                            end: [e.x as f64, e.y as f64, e.z as f64],
                            overriding_normal: [on.x as f64, on.y as f64, on.z as f64],
                            convergence_speed: pa.convergence_speed,
                        }
                    })
                    .collect();
                let total_target = if tec.people_ticks_remaining > 0 {
                    tec.people_ticks_remaining
                } else {
                    tec.tectonics_ticks.max(75)
                };
                (
                    tiles,
                    groups,
                    ridges,
                    hotspots,
                    oceanic_arcs,
                    tec.tectonics_ticks,
                    tec.atmosphere_ticks_remaining,
                    tec.life_ticks_remaining,
                    total_target,
                )
            }
            tec11_proto::PlanetStageOneof::Prehistory(pre) => {
                if pre.tiles.len() != n {
                    return None;
                }
                let tiles: Vec<TileGeology> = pre
                    .tiles
                    .iter()
                    .map(|pt| TileGeology::from_proto_geological_tile(pt.geology.as_ref()?))
                    .collect::<Option<Vec<_>>>()?;
                let groups: Vec<LithosphericGroup> = pre
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                (
                    tiles,
                    groups,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    pre.tectonics_ticks,
                    0,
                    0,
                    pre.tectonics_ticks.max(75),
                )
            }
            tec11_proto::PlanetStageOneof::History(hist) => {
                let pre = hist.prehistory_base.as_ref()?;
                if pre.tiles.len() != n {
                    return None;
                }
                let tiles: Vec<TileGeology> = pre
                    .tiles
                    .iter()
                    .map(|pt| TileGeology::from_proto_geological_tile(pt.geology.as_ref()?))
                    .collect::<Option<Vec<_>>>()?;
                let groups: Vec<LithosphericGroup> = pre
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                (
                    tiles,
                    groups,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    pre.tectonics_ticks,
                    0,
                    0,
                    pre.tectonics_ticks.max(75),
                )
            }
        };

        if groups.is_empty() {
            let mut visited = vec![false; n];
            for start in 0..n {
                if visited[start] || tiles[start].elevation() <= 0.0 {
                    continue;
                }
                let mut comp = Vec::new();
                let mut queue = std::collections::VecDeque::new();
                visited[start] = true;
                queue.push_back(start);
                while let Some(u) = queue.pop_front() {
                    comp.push(u);
                    for &nb in &adjacency[u] {
                        if !visited[nb] && tiles[nb].elevation() > 0.0 {
                            visited[nb] = true;
                            queue.push_back(nb);
                        }
                    }
                }
                groups.push(LithosphericGroup {
                    tiles: comp,
                    velocity: [0.0, 0.0, 0.0],
                });
            }
        }

        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let radius_km = if snap.radius_km > 0.0 {
            snap.radius_km as f64
        } else {
            PangeaWorld::planet_radius_km(grid_size)
        };

        let spin = if snap.spin > 0.0 {
            snap.spin as f64
        } else {
            1.0
        };
        let target_day_hours = (24.0 / spin).clamp(1.0, 240.0);
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        let tilt_deg = if snap.tilt_deg > 0.0 {
            snap.tilt_deg as f64
        } else {
            23.0
        };
        let dt = 5.0;
        let dp = (100.0 / radius_km) * dt;
        let build_rate = dt / 5.0;
        let erode_rate = dt;
        let earth_area_km2 = 4.0 * PI * 6400.0 * 6400.0;
        let splitnum = (25.0e6 / earth_area_km2) * (n as f64);

        let rng = if let Ok(bytes_16) = <[u8; 16]>::try_from(snap.rng_state_16b.as_slice()) {
            Pcg64Mcg::from_bytes(bytes_16)
        } else {
            Pcg64Mcg::from_seed_u64(fallback_seed.wrapping_add(geoticks as u64))
        };

        let climate_model = ClimateModel::new_for_tectonics_with_adjacency(
            grid.clone(),
            adjacency.clone(),
            cells,
            spin,
            tilt_deg,
        );

        let (moon_count, moon_mass, target_moon_dist_radii, meteor_freq) =
            if let Some(ref env) = snap.environment {
                (
                    env.moon_count,
                    if env.moon_count == 0 {
                        0.0
                    } else if env.moon_mass_lunar > 0.0 {
                        env.moon_mass_lunar as f64
                    } else {
                        1.0
                    },
                    if env.target_moon_dist_radii >= 15 {
                        env.target_moon_dist_radii as f64
                    } else {
                        60.0
                    },
                    (env.meteor_freq as f64).clamp(0.0, 10.0),
                )
            } else {
                (1, 1.0, 60.0, 1.0)
            };

        let mut sim = Self {
            grid,
            adjacency,
            tree,
            radius_km,
            dp,
            build_rate,
            erode_rate,
            splitnum,
            atmosphere_ticks_remaining,
            life_ticks_remaining,
            total_target_ticks,
            geoticks,
            target_day_hours,
            base_tilt_deg: tilt_deg,
            moon_count,
            moon_mass,
            target_moon_dist_radii,
            tiles,
            groups,
            ridges,
            hotspots,
            oceanic_arcs,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_temp_norm: vec![0.5; n],
            tile_temp_c: vec![15.0; n],
            tile_precip_norm: vec![0.5; n],
            tile_impact_scars: vec![0.0; n],
            meteor_freq,
            impact_count: 0,
            impact_winter_ticks: 0,
            sterilization_delays: 0,
            last_impact_tile: None,
            last_impact_diameter_km: 0.0,
            saved_environment: snap.environment.clone(),
            rng,
        };
        sim.climate_model
            .set_spin_and_tilt_for_tectonics(sim.current_spin(), sim.current_tilt_deg());
        if sim.has_atmosphere() {
            sim.refresh_climate_fields();
        }
        // Reconstruct visible impact scars from shock-metamorphic hornfels / coarse impact breccia in top strata:
        for v in 0..n {
            sim.tile_impact_scars[v] = sim.tiles[v].impact_signature();
        }
        Some(sim)
    }

    /// Decode `.tec11` binary Protocol Buffer bytes into a `TectonicsSimulation`.
    pub fn decode_snapshot(bytes: &[u8], fallback_seed: u64) -> Option<Self> {
        let snap = tec11_proto::PlanetSnapshot::decode_bytes(bytes).ok()?;
        Self::from_snapshot(&snap, fallback_seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tec11_proto::GeologicalTileExt;

    #[test]
    fn test_tectonics_simulation_lifecycle() {
        // Use size 4 (2,100 km radius, 812 tiles) for fast multi-step lifecycle test
        let mut sim = TectonicsSimulation::new(4, 29.0, 42);
        assert_eq!(sim.geoticks(), 0);
        assert_eq!(sim.continent_count(), 1);
        assert!(!sim.has_atmosphere());
        assert!(!sim.has_life());

        // Step 30 ticks (150 Myr): Pangaea should rift/move, atmosphere should activate at tick 25,
        // and metamorphic/sedimentary/igneous layers should form.
        for _ in 0..30 {
            sim.step();
        }

        assert_eq!(sim.geoticks(), 30);
        assert!(sim.has_atmosphere());
        assert!(sim.continent_count() >= 1);
        assert!(
            (15..=40).contains(&sim.land_percent()),
            "Expected land percent to remain near initial ~29%, got {}%",
            sim.land_percent()
        );

        let has_metamorphic = sim.tiles().iter().any(|t| {
            t.layers
                .iter()
                .any(|l| l.rock.kind == RockKind::Metamorphic)
        });
        let has_sedimentary = sim.tiles().iter().any(|t| {
            t.layers
                .iter()
                .any(|l| l.rock.kind == RockKind::Sedimentary)
        });
        assert!(has_metamorphic, "Expected regional/contact metamorphic layers after 30 ticks");
        assert!(has_sedimentary, "Expected eroded sedimentary layers after atmosphere activates");

        let report = sim.inspect_tile(0);
        assert!(report.contains("Tile #0"));
    }

    #[test]
    fn test_no_interior_pinholes() {
        let mut sim = TectonicsSimulation::new(5, 29.0, 42);
        for tick in 1..=35 {
            sim.step();

            // Count non-land components (ocean + any interior holes)
            let n = sim.tile_count();
            let mut visited = vec![false; n];
            let mut ocean_comps = Vec::new();
            for start in 0..n {
                if visited[start] || sim.tiles()[start].elevation() > 0.0 {
                    continue;
                }
                let mut comp = Vec::new();
                let mut q = std::collections::VecDeque::new();
                visited[start] = true;
                q.push_back(start);
                while let Some(u) = q.pop_front() {
                    comp.push(u);
                    for &nb in &sim.adjacency[u] {
                        if !visited[nb] && sim.tiles()[nb].elevation() <= 0.0 {
                            visited[nb] = true;
                            q.push_back(nb);
                        }
                    }
                }
                ocean_comps.push(comp);
            }
            ocean_comps.sort_by_key(|c| std::cmp::Reverse(c.len()));
            let pinholes: Vec<usize> = ocean_comps
                .iter()
                .skip(1)
                .map(|c| c.len())
                .filter(|&len| len <= 3)
                .collect();
            assert!(
                pinholes.is_empty(),
                "Tick {}: unexpected 1-3 tile interior pinholes: {:?}",
                tick,
                pinholes
            );
        }
    }

    #[test]
    fn test_realistic_ocean_bathymetry() {
        let mut sim = TectonicsSimulation::new(5, 29.0, 42);
        for _ in 0..25 {
            sim.step();
        }
        let n = sim.tile_count();
        let mut min_ocean = f32::INFINITY;
        let mut max_ocean = f32::NEG_INFINITY;
        let mut non_coastal_above_abyssal = 0usize;
        let mut open_ocean_seamount_or_arc = 0usize;
        for v in 0..n {
            let elev = sim.tiles()[v].elevation();
            if elev <= 0.0 {
                min_ocean = min_ocean.min(elev);
                max_ocean = max_ocean.max(elev);
                let has_land_nb = sim.adjacency[v]
                    .iter()
                    .any(|&nb| sim.tiles()[nb].elevation() > 0.0);
                if !has_land_nb {
                    // Open-ocean tiles span intra-oceanic trenches [-6.85, -5.5], ridge-to-abyssal
                    // thermal subsidence [-5.5, -2.5], and buoyant seamount / island-arc ridges (-2.5, -0.05].
                    assert!(
                        (-6.9..=-0.05).contains(&elev),
                        "Open-ocean tile {} has out-of-range elevation {} km",
                        v,
                        elev
                    );
                    if elev > -4.5 {
                        non_coastal_above_abyssal += 1;
                    }
                    if sim.tiles()[v].thickness() > 6.0 && elev > -3.8 {
                        open_ocean_seamount_or_arc += 1;
                    }
                }
            }
        }
        assert!(
            min_ocean >= -7.0 && min_ocean < -5.3,
            "Expected abyssal/trench depths in [-7.0, -5.3), got {}",
            min_ocean
        );
        assert!(
            max_ocean <= -0.05 && max_ocean > -1.5,
            "Expected passive continental shelf or shallow seamount in (-1.5, -0.05], got {}",
            max_ocean
        );
        assert!(
            non_coastal_above_abyssal > 20,
            "Expected active mid-ocean ridges and seamounts to elevate open-ocean tiles above abyssal plain, got {}",
            non_coastal_above_abyssal
        );
        assert!(
            open_ocean_seamount_or_arc >= 3,
            "Expected mantle hotspots and intra-oceanic arcs to build thickened submarine seamount/arc tiles, got {}",
            open_ocean_seamount_or_arc
        );
    }

    #[test]
    fn test_orogeny_erosion_balance() {
        let mut sim = TectonicsSimulation::new(4, 45.0, 42);
        for _ in 0..80 {
            sim.step();
        }
        let land_tiles: Vec<usize> = (0..sim.tile_count())
            .filter(|&v| sim.tiles()[v].elevation() > 0.0)
            .collect();
        assert!(!land_tiles.is_empty());
        let mean_land_elev: f32 = land_tiles
            .iter()
            .map(|&v| sim.tiles()[v].elevation())
            .sum::<f32>()
            / (land_tiles.len() as f32);
        let max_land_elev: f32 = land_tiles
            .iter()
            .map(|&v| sim.tiles()[v].elevation())
            .fold(0.0f32, f32::max);
        let alpine_snow_count = land_tiles
            .iter()
            .filter(|&&v| matches!(sim.tile_koeppen_ids()[v], 12 | 13))
            .count();
        let alpine_snow_frac = (alpine_snow_count as f64) / (land_tiles.len() as f64);

        assert!(
            max_land_elev > 4.2,
            "Expected active orogeny to build mountain ranges > 4.2 km after 80 geoticks, got max {:.2} km (mean {:.2} km)",
            max_land_elev,
            mean_land_elev
        );
        assert!(
            (0.45..=2.40).contains(&mean_land_elev),
            "Expected mean continental elevation in [0.45, 2.40] km after 80 geoticks, got {:.2} km (max {:.2} km)",
            mean_land_elev,
            max_land_elev
        );
        assert!(
            alpine_snow_frac < 0.35,
            "Expected < 35% of land to be polar/alpine snow (ET/EF) after 80 geoticks, got {:.1}% (mean elev {:.2} km, max {:.2} km)",
            alpine_snow_frac * 100.0,
            mean_land_elev,
            max_land_elev
        );
    }

    #[test]
    fn test_proto_snapshot_roundtrip_and_climate_load() {
        let mut sim = TectonicsSimulation::new(4, 30.0, 99);
        for _ in 0..30 {
            sim.step();
        }
        let bytes = sim.encode_snapshot();
        // Verify Protobuf header starts with field 1 (`rng_state_16b`, tag 0x0A, len 0x10)
        assert_eq!(bytes[0], 0x0A);
        assert_eq!(bytes[1], 0x10);

        let mut restored =
            TectonicsSimulation::decode_snapshot(&bytes, 99).expect("valid .tec11 proto snapshot");
        assert_eq!(restored.grid().size, sim.grid().size);
        assert_eq!(restored.geoticks(), sim.geoticks());
        assert_eq!(restored.continent_count(), sim.continent_count());
        assert_eq!(restored.land_percent(), sim.land_percent());
        assert_eq!(restored.has_atmosphere(), sim.has_atmosphere());
        assert_eq!(restored.has_life(), sim.has_life());
        for i in 0..sim.tile_count() {
            assert!((restored.tiles()[i].elevation() - sim.tiles()[i].elevation()).abs() < 1e-5);
            assert_eq!(restored.tiles()[i].layers.len(), sim.tiles()[i].layers.len());
        }

        // Verify loading the decoded PlanetSnapshot into ClimateModel produces seasonal climate and ocean currents:
        let snap = tec11_proto::PlanetSnapshot::decode_bytes(&bytes).unwrap();
        let elevs: Vec<f32> = snap
            .geological_tiles()
            .unwrap()
            .into_iter()
            .map(|t| t.elevation())
            .collect();
        let climate = ClimateModel::with_elevations(restored.grid().clone(), elevs, 3, 0.5);
        assert_eq!(climate.tile_count(), restored.tile_count());
        assert_eq!(climate.season_count(), 8);
        assert!(climate.ocean_for_season(0).is_some());
        assert!(climate.ocean_for_season(4).is_some());

        // Verify exact PRNG continuation: stepping both simulations 5 more ticks produces identical state!
        for _ in 0..5 {
            sim.step();
            restored.step();
        }
        assert_eq!(restored.geoticks(), sim.geoticks());
        assert_eq!(restored.continent_count(), sim.continent_count());
        assert_eq!(restored.land_percent(), sim.land_percent());
    }

    #[test]
    fn test_lunar_recession_spin_braking_and_harmonic_orbits() {
        // 1 Earth-like Moon (target: 24h day, 23 deg tilt, 60 Rp moon distance after 75 ticks = 375 Myr):
        let mut earth =
            TectonicsSimulation::new_with_full_params(4, 24.0, 23.0, 29.0, 125, 125, 125, 1, 1.0, 60.0, 42);
        let a0 = earth.current_moon_dist_radii();
        let day0 = earth.current_day_hours();
        let tide0 = earth.current_tidal_strength();
        assert!(
            a0 > 20.0 && a0 < 30.0,
            "Expected Hadean Moon to start near ~23.4 Rp, got {a0:.2} Rp"
        );
        assert!(
            day0 > 9.5 && day0 < 13.5,
            "Expected Hadean Earth day to start near ~11.1 hours, got {day0:.2} h"
        );
        assert!(
            tide0 > 10.0,
            "Expected Hadean tides > 10x modern Earth, got {tide0:.2}x"
        );

        // Advance to target Prehistory/History transition (75 ticks = 375 Myr):
        earth.geoticks = 75;
        assert!(
            (earth.current_moon_dist_radii() - 60.0).abs() < 1e-4,
            "Expected Moon to reach 60.0 Rp at Prehistory start, got {:.3}",
            earth.current_moon_dist_radii()
        );
        assert!(
            (earth.current_day_hours() - 24.0).abs() < 1e-4,
            "Expected Earth day to brake to 24.0 h at Prehistory start, got {:.3}",
            earth.current_day_hours()
        );
        assert!(
            (earth.current_tidal_strength() - 1.0).abs() < 1e-4,
            "Expected tidal strength = 1.0x modern Earth at Prehistory start, got {:.3}",
            earth.current_tidal_strength()
        );

        // Verify 2:1 Laplace resonance chain multipliers (1 + 1/4 + 1/16 = 1.3125):
        assert!((TectonicsSimulation::harmonic_tidal_multiplier(1) - 1.0).abs() < 1e-9);
        assert!((TectonicsSimulation::harmonic_tidal_multiplier(2) - 1.25).abs() < 1e-9);
        assert!((TectonicsSimulation::harmonic_tidal_multiplier(3) - 1.3125).abs() < 1e-9);

        // Verify moonless planet has weak solar-only tides (0.15x) and larger obliquity excursions:
        let mut moonless =
            TectonicsSimulation::new_with_full_params(4, 24.0, 23.0, 29.0, 125, 125, 125, 0, 0.0, 60.0, 42);
        assert!((moonless.current_tidal_strength() - 0.15).abs() < 1e-6);
        assert!((moonless.current_day_hours() - 24.0).abs() < 1e-6);
        moonless.geoticks = 8;
        let tilt_wander = (moonless.current_tilt_deg() - 23.0).abs();
        earth.geoticks = 8;
        let tilt_stable = (earth.current_tilt_deg() - 23.0).abs();
        assert!(
            tilt_wander > 5.0 * tilt_stable,
            "Expected moonless obliquity excursion ({tilt_wander:.2} deg) >> moon-stabilized ({tilt_stable:.2} deg)"
        );
    }

    #[test]
    fn test_bolide_impact_crater_excavation_and_biosphere_effects() {
        let mut sim = TectonicsSimulation::new(4, 30.0, 42);
        // Find a continental tile:
        let land_idx = (0..sim.tile_count())
            .find(|&v| sim.tiles()[v].elevation() > 0.8)
            .unwrap();
        let pre_elev = sim.tiles()[land_idx].elevation();

        // Apply a 200 km Chicxulub-scale bolide strike:
        sim.apply_bolide_impact(land_idx, 200.0);
        assert!(sim.impact_count() >= 1);
        assert!((sim.tile_impact_scars()[land_idx] - 1.0).abs() < 1e-5);
        assert!(
            sim.tiles()[land_idx].elevation() < pre_elev,
            "Expected bolide strike to excavate & depress crater floor"
        );
        assert!(
            sim.tiles()[land_idx].impact_signature() >= 0.65,
            "Expected shocked hornfels stratigraphic signature at crater floor"
        );
        let report = sim.inspect_tile(land_idx);
        assert!(report.contains("Impact Crater Basin"));

        // Verify meteor_freq = 0.0 disables stochastic bolide impacts across 25 ticks,
        // while meteor_freq = 4.0 increases impact count over meteor_freq = 1.0:
        let mut sim_off = TectonicsSimulation::new(4, 29.0, 42);
        sim_off.set_meteor_freq(0.0);
        let mut sim_std = TectonicsSimulation::new(4, 29.0, 42);
        sim_std.set_meteor_freq(1.0);
        let mut sim_heavy = TectonicsSimulation::new(4, 29.0, 42);
        sim_heavy.set_meteor_freq(4.0);
        for _ in 0..25 {
            sim_off.step();
            sim_std.step();
            sim_heavy.step();
        }
        assert_eq!(sim_off.impact_count(), 0);
        assert!(
            sim_off.tile_impact_scars().iter().all(|&s| s == 0.0),
            "Subduction contact-metamorphic hornfels and coastal conglomerates must not produce false meteor scars when meteor_freq = 0"
        );
        assert!(sim_heavy.impact_count() > sim_std.impact_count());

        // Verify that an impact crater on a moving continental plate advects with the continent
        // rather than remaining pinned to static grid coordinates:
        let mut sim_move = TectonicsSimulation::new(4, 29.0, 42);
        sim_move.set_meteor_freq(0.0);
        // Step until Pangaea splits and plates have non-zero velocity:
        for _ in 0..8 {
            sim_move.step();
        }
        let moving_group_idx = sim_move
            .groups
            .iter()
            .position(|g| {
                let speed = (g.velocity[0].powi(2) + g.velocity[1].powi(2) + g.velocity[2].powi(2)).sqrt();
                speed > 1e-4 && g.tiles.len() > 20
            })
            .expect("Expected a moving continental plate");
        // Pick an interior tile of the moving continent:
        let group_set: std::collections::HashSet<usize> = sim_move.groups[moving_group_idx]
            .tiles
            .iter()
            .copied()
            .collect();
        let impact_tile = sim_move.groups[moving_group_idx]
            .tiles
            .iter()
            .copied()
            .find(|&v| {
                sim_move.tiles()[v].elevation() > 0.5
                    && sim_move.adjacency[v].iter().all(|nb| group_set.contains(nb))
            })
            .unwrap_or(sim_move.groups[moving_group_idx].tiles[0]);
        let initial_pos = sim_move.grid().faces[impact_tile].0;
        let vel = sim_move.groups[moving_group_idx].velocity;

        sim_move.apply_bolide_impact(impact_tile, 200.0);
        assert!(sim_move.tile_impact_scars()[impact_tile] > 0.9);

        // Step 12 ticks so the continent drifts across several grid tiles:
        for _ in 0..12 {
            sim_move.step();
        }

        // Any tile that is now ocean must not retain the continental crater scar:
        for v in 0..sim_move.tile_count() {
            if sim_move.tiles()[v].elevation() <= 0.0 {
                assert!(
                    sim_move.tile_impact_scars()[v] < 0.05,
                    "Vacated ocean tile {v} retained continental crater scar {:.3}",
                    sim_move.tile_impact_scars()[v]
                );
            }
        }

        // The peak scar centroid should have displaced in the direction of plate tangent velocity `vel`:
        let peak_tile = (0..sim_move.tile_count())
            .max_by(|&a, &b| {
                sim_move.tile_impact_scars()[a]
                    .partial_cmp(&sim_move.tile_impact_scars()[b])
                    .unwrap()
            })
            .unwrap();
        assert!(
            sim_move.tile_impact_scars()[peak_tile] > 0.25,
            "Expected crater scar to persist on moving continent after 12 ticks"
        );
        let new_pos = sim_move.grid().faces[peak_tile].0;
        let actual_disp = [
            new_pos[0] - initial_pos[0],
            new_pos[1] - initial_pos[1],
            new_pos[2] - initial_pos[2],
        ];
        let dot_dir = actual_disp[0] * vel[0]
            + actual_disp[1] * vel[1]
            + actual_disp[2] * vel[2];
        assert!(
            peak_tile != impact_tile && dot_dir > 0.0,
            "Expected crater peak tile to move from {impact_tile} to {peak_tile} along plate velocity (dot={dot_dir:.5})"
        );
    }

    #[test]
    fn test_compositional_mineral_fields_populate_and_roundtrip() {
        let mut sim = TectonicsSimulation::new_with_params(3, 24.0, 23.5, 35.0, 10, 15, 42);
        for _ in 0..25 {
            sim.step();
        }

        let mut bog_count = 0usize;
        let mut quartz_count = 0usize;
        let mut sulf_count = 0usize;
        let mut salt_count = 0usize;
        let mut black_smoker_count = 0usize;
        let mut bif_count = 0usize;

        for tile in sim.tiles() {
            for layer in &tile.layers {
                let q = layer.rock.quartzity.unwrap_or(0.0);
                let s = layer.rock.sulfidity.unwrap_or(0.0);
                if layer.rock.bogginess.unwrap_or(0.0) > 0.0 {
                    bog_count += 1;
                }
                if q > 0.0 {
                    quartz_count += 1;
                }
                if s > 0.0 {
                    sulf_count += 1;
                }
                if layer.rock.salinity.unwrap_or(0.0) > 0.0 {
                    salt_count += 1;
                }
                if layer.rock.kind == RockKind::Igneous && s > 0.12 && q > 0.01 {
                    black_smoker_count += 1;
                }
                if layer.rock.kind != RockKind::Igneous && s > 0.10 && q > 0.10 {
                    bif_count += 1;
                }
            }
        }

        assert!(bog_count > 0, "Expected bogginess > 0 in humid vegetated layers");
        assert!(quartz_count > 0, "Expected quartzity > 0 in orogenic/contact/placer layers");
        assert!(sulf_count > 0, "Expected sulfidity > 0 in seafloor/contact/orogenic layers");
        assert!(salt_count > 0, "Expected salinity > 0 in warm arid evaporite layers");
        assert!(
            black_smoker_count > 0,
            "Expected black-smoker VMS enrichment on spreading-ridge basalt layers"
        );
        assert!(
            bif_count > 0,
            "Expected Banded Iron Formation (co-enriched sulfidity + quartzity) layers"
        );

        // Verify precious minerals (e.g., workable Gold) are genuinely rare on land compared to Quartz:
        let land_tiles: Vec<&TileGeology> =
            sim.tiles().iter().filter(|t| t.elevation() > 0.0).collect();
        if !land_tiles.is_empty() {
            let mut workable_gold_tiles = 0usize;
            let mut workable_quartz_tiles = 0usize;
            for t in &land_tiles {
                let acc = t.accessible_mineral_abundances(0.5);
                if acc
                    .get(&tec11_geology::tile::MineralKind::Gold)
                    .map(|&(g, a)| g >= tec11_geology::tile::WORKABLE_GRADE && a >= 0.005)
                    .unwrap_or(false)
                {
                    workable_gold_tiles += 1;
                }
                if acc
                    .get(&tec11_geology::tile::MineralKind::Quartz)
                    .map(|&(g, a)| g >= tec11_geology::tile::WORKABLE_GRADE && a >= 0.020)
                    .unwrap_or(false)
                {
                    workable_quartz_tiles += 1;
                }
            }
            let gold_frac = workable_gold_tiles as f32 / land_tiles.len() as f32;
            assert!(
                gold_frac < 0.10,
                "Workable surface gold should be rare (<10% of land tiles), got {:.1}% ({}/{}); quartz={}",
                gold_frac * 100.0,
                workable_gold_tiles,
                land_tiles.len(),
                workable_quartz_tiles
            );
        }

        // Verify .tec11 snapshot roundtrip preserves all four compositional fields:
        let bytes = sim.encode_snapshot();
        let restored = TectonicsSimulation::decode_snapshot(&bytes, 42).expect("roundtrip snapshot");
        for (a, b) in sim.tiles().iter().zip(restored.tiles().iter()) {
            let mut a_norm = a.clone();
            a_norm.normalize_for_snapshot();
            assert_eq!(a_norm.layers.len(), b.layers.len());
            for (la, lb) in a_norm.layers.iter().zip(b.layers.iter()) {
                assert_eq!(la.rock.bogginess, lb.rock.bogginess);
                assert_eq!(la.rock.quartzity, lb.rock.quartzity);
                assert_eq!(la.rock.sulfidity, lb.rock.sulfidity);
                assert_eq!(la.rock.salinity, lb.rock.salinity);
            }
        }
    }

    #[test]
    fn test_mantle_hotspots_seamount_trails_and_oceanic_arcs_isostasy() {
        let mut sim = TectonicsSimulation::new(5, 28.0, 1337);
        assert!(!sim.hotspots().is_empty(), "Expected active mantle hotspots");

        for _ in 0..32 {
            sim.step();
        }

        // 1. Verify advecting submarine seamount/guyot trails (`thickness > 6.2 km` in open ocean):
        let seamount_tiles: Vec<usize> = (0..sim.tile_count())
            .filter(|&v| {
                let t = &sim.tiles()[v];
                t.elevation() <= 0.0
                    && t.thickness() > 6.2
                    && sim.adjacency[v].iter().all(|&nb| sim.tiles()[nb].elevation() <= 0.0)
            })
            .collect();
        assert!(
            seamount_tiles.len() >= 6,
            "Expected advecting submarine seamount trails + arc ridges (>= 6 open-ocean tiles), got {}",
            seamount_tiles.len()
        );

        // 2. Verify intra-oceanic subduction arcs generate intermediate/andesitic (`felsity >= 0.55`) submarine arc crust:
        let has_arc_crust = (0..sim.tile_count()).any(|v| {
            let t = &sim.tiles()[v];
            t.elevation() <= 0.0
                && t.thickness() > 6.5
                && t.layers
                    .iter()
                    .any(|l| l.rock.kind == RockKind::Igneous && l.rock.felsity >= 0.55)
        });
        assert!(
            has_arc_crust,
            "Expected intra-oceanic subduction arcs to build intermediate/andesitic submarine arc crust"
        );

        // 3. Verify water-loaded Airy isostasy: thicker submarine volcanic crust floats higher than 5 km abyssal floor:
        let mean_seamount_elev: f32 = seamount_tiles
            .iter()
            .map(|&v| sim.tiles()[v].elevation())
            .sum::<f32>()
            / (seamount_tiles.len() as f32);
        assert!(
            mean_seamount_elev > -4.2,
            "Expected thickened seamount/arc crust to sit above abyssal plain via Airy buoyancy, got mean {:.2} km",
            mean_seamount_elev
        );
    }
}
