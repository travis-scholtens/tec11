use std::f64::consts::PI;
use tec11_math_grid::SphericalGrid;

use crate::earth_data::EARTH_SIZE6_ELEVATIONS_M;
use crate::koeppen::{classify_tile, TileClimateClassification};
use crate::ocean::{
    solve_seasonal_ocean_circulation_with_geom, OceanCirculation, OceanGridGeometry,
};

/// Standard 8 seasonal steps used in `Tec-Nine` (`climatesimulation.py:41`).
pub const DEFAULT_SEASONS: [f64; 8] = [-1.0, -0.5, 0.0, 0.5, 1.0, 0.5, 0.0, -0.5];

/// Global temperature interval `[min_c, max_c]` in degrees Celsius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemperatureRange {
    pub min_c: f64,
    pub max_c: f64,
}

impl TemperatureRange {
    pub const fn new(min_c: f64, max_c: f64) -> Self {
        Self { min_c, max_c }
    }

    #[inline]
    pub fn span(self) -> f64 {
        self.max_c - self.min_c
    }
}

/// Weighted downwind edge in the atmospheric advection graph (`climatemethod.py`).
#[derive(Clone, Copy, Debug)]
pub struct DownwindLink {
    pub target_tile: usize,
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindDestCacheKey {
    cells: u32,
    spin_bits: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OceanCacheKey {
    cells: u32,
    spin_bits: u64,
    tilt_bits: u64,
    glaciation_bits: u64,
}

/// Default spin rate and axial tilt (`climatesimulation.py:11-12`).
pub const DEFAULT_SPIN: f64 = 1.0;
pub const DEFAULT_TILT_DEG: f64 = 23.0;
pub const MEAN_TEMPRANGE_C: TemperatureRange = TemperatureRange::new(-25.0, 50.0);
pub const BREEZE_DISTANCE: u32 = 10;

/// Compute initial compass bearing (in degrees `[0, 360)`) from `(lat1_deg, lon1_deg)`
/// to `(lat2_deg, lon2_deg)`, matching `bearing()` in `climatemethod.py`.
pub fn bearing_deg(lat1_deg: f64, lon1_deg: f64, lat2_deg: f64, lon2_deg: f64) -> f64 {
    let lat1 = lat1_deg * PI / 180.0;
    let lon1 = lon1_deg * PI / 180.0;
    let lat2 = lat2_deg * PI / 180.0;
    let lon2 = lon2_deg * PI / 180.0;
    let dlon = lon2 - lon1;

    let theta = (dlon.sin() * lat2.cos())
        .atan2(lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos());
    (theta * 180.0 / PI).rem_euclid(360.0)
}

/// Compute seasonal solar insolation at `lat_deg` for `season` in `[-1.0, 1.0]`
/// and axial `tilt_deg`, matching `Climate.insolation` in `climatemethod.py`.
pub fn insolation(lat_deg: f64, season: f64, tilt_deg: f64) -> f64 {
    let theta = (lat_deg + tilt_deg * season) * PI / 180.0;
    let ins = theta.cos().max(0.0);
    0.5 + (ins - 0.5) * (tilt_deg * PI / 180.0).cos()
}

/// Compute prevailing wind direction (in degrees `[0, 360)`) at `lat_deg` for
/// `cells` atmospheric circulation cells per hemisphere and planetary `spin`,
/// matching `Climate.winddirection` in `climatemethod.py`.
pub fn wind_direction_deg(lat_deg: f64, cells: u32, spin: f64) -> f64 {
    let c = cells.max(1) as f64;
    let band = (lat_deg.abs() / (90.0 / c)) as i32 & 1;
    let n = if lat_deg > 0.0 { band } else { 1 - band };
    let base_deg = 180.0 - 180.0 * (n as f64);
    let ce = 2.0 * spin * (PI * (180.0 - lat_deg) / 180.0).sin();
    (base_deg + ce.atan2(1.0) * 180.0 / PI).rem_euclid(360.0)
}

/// Discrete odd circulation cell count per hemisphere (`1, 3, 5, 7`) for a continuous
/// planetary rotation rate `spin = 24.0 / day_hours`, with regime transitions centered
/// at the midpoints between the canonical `8h (7 cells)`, `12h (5 cells)`, `24h (3 cells)`,
/// and `48h (1 cell)` states (`9.6h`, `16h`, and `32h`) so `24h` (`spin = 1.0`) sits
/// squarely in the middle of the 3-cell regime rather than on a step-function cliff.
pub fn nominal_cells_for_spin(spin: f64) -> u32 {
    if spin >= 2.5 {
        7
    } else if spin >= 1.5 {
        5
    } else if spin >= 0.75 {
        3
    } else {
        1
    }
}

/// Compute global temperature limits `(min_c, max_c)` shifted by `glaciation` in `[0.0, 1.0]`,
/// matching `temprange()` in `climatemethod.py`.
pub fn temp_range_for_glaciation(
    mean_range: TemperatureRange,
    glaciation: f64,
) -> TemperatureRange {
    let dtemp = 5.0 * (glaciation / 0.5);
    TemperatureRange::new(mean_range.min_c + dtemp, mean_range.max_c + dtemp)
}

/// Per-season scalar and vector fields across all grid tiles.
#[derive(Clone, Debug)]
pub struct SeasonalSnapshot {
    pub season_value: f64,
    pub insolation: Vec<f32>,
    pub temperature: Vec<f32>,
    pub precipitation: Vec<f32>,
    pub ocean: OceanCirculation,
}

/// Complete climate model over a `SphericalGrid`, supporting canned Earth elevation data
/// or any custom tile elevation array, coupled with seasonal Stommel–Ekman ocean circulation.
pub struct ClimateModel {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    ocean_geom: OceanGridGeometry,
    lats_deg: Vec<f64>,
    lons_deg: Vec<f64>,
    /// Scaled tile elevations matching `Tec-Nine` (`<= 0.0` for ocean, `(0.0..=10.0]` for land).
    elevations: Vec<f32>,
    cells: u32,
    glaciation: f64,
    spin: f64,
    tilt_deg: f64,
    /// Cached wind/seabreeze parameters.
    cached_dest_params: Option<WindDestCacheKey>,
    /// Cached ocean circulation parameters.
    cached_ocean_params: Option<OceanCacheKey>,
    dest_map: Vec<Vec<DownwindLink>>,
    /// Per-tile sea-breeze moisture factor `((d - breeze) / d)^2`.
    seabased: Vec<f64>,
    /// Coupled seasonal wind-driven Stommel–Ekman ocean circulation and SST anomaly.
    ocean_seasons: Vec<OceanCirculation>,
    /// Computed 8-season snapshots.
    seasons: Vec<SeasonalSnapshot>,
    /// Computed annual Köppen and summary classification per tile.
    classifications: Vec<TileClimateClassification>,
}

impl ClimateModel {
    /// Construct a `ClimateModel` on a size-6 `SphericalGrid` (`7,292` tiles) initialized
    /// with canned ETOPO2 Earth elevation data, matching `ClimateSimulation.earth()` in `Tec-Nine`.
    pub fn new_earth(cells: u32, glaciation: f64) -> Self {
        let grid = SphericalGrid::with_size(6);
        assert_eq!(
            grid.faces.len(),
            EARTH_SIZE6_ELEVATIONS_M.len(),
            "Size-6 SphericalGrid face count must match EARTH_SIZE6_ELEVATIONS_M"
        );

        // Convert ETOPO meters to Tec-Nine elevation units (meters / 900.0, clamped to [0.0, 10.0]).
        let elevations: Vec<f32> = EARTH_SIZE6_ELEVATIONS_M
            .iter()
            .map(|&m| {
                if m > 0 {
                    ((m as f32) / 900.0).min(10.0)
                } else {
                    0.0
                }
            })
            .collect();

        Self::with_elevations(grid, elevations, cells, glaciation)
    }

    /// Construct a `ClimateModel` for an arbitrary `SphericalGrid` and scaled `elevations`.
    pub fn with_elevations(
        grid: SphericalGrid,
        elevations: Vec<f32>,
        cells: u32,
        glaciation: f64,
    ) -> Self {
        let n = grid.faces.len();
        assert_eq!(n, elevations.len());
        let adjacency = grid.adjacency();

        let (lats_deg, lons_deg): (Vec<f64>, Vec<f64>) = grid
            .faces
            .iter()
            .map(|&([x, y, z], _)| {
                let lat = 180.0 / PI * z.atan2((x * x + y * y).sqrt());
                // In our right-handed globe frame (where +Z = x is Prime Meridian and +Y = z is North),
                // East (+X) is -y, so geographic longitude is atan2(-y, x).
                let lon = 180.0 / PI * (-y).atan2(x);
                (lat, lon)
            })
            .unzip();

        let ocean_geom = OceanGridGeometry::new(&grid, &adjacency, &lats_deg);

        let mut model = Self {
            grid,
            adjacency,
            ocean_geom,
            lats_deg,
            lons_deg,
            elevations,
            cells: cells.max(1),
            glaciation: glaciation.clamp(0.0, 1.0),
            spin: DEFAULT_SPIN,
            tilt_deg: DEFAULT_TILT_DEG,
            cached_dest_params: None,
            cached_ocean_params: None,
            dest_map: vec![Vec::new(); n],
            seabased: vec![0.0; n],
            ocean_seasons: Vec::new(),
            seasons: Vec::new(),
            classifications: Vec::new(),
        };
        model.recompute();
        model
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.grid.faces.len()
    }

    pub fn season_count(&self) -> usize {
        self.seasons.len()
    }

    pub fn cells(&self) -> u32 {
        self.cells
    }

    pub fn glaciation(&self) -> f64 {
        self.glaciation
    }

    pub fn spin(&self) -> f64 {
        self.spin
    }

    pub fn tilt_deg(&self) -> f64 {
        self.tilt_deg
    }

    pub fn elevations(&self) -> &[f32] {
        &self.elevations
    }

    pub fn lats_deg(&self) -> &[f64] {
        &self.lats_deg
    }

    pub fn lons_deg(&self) -> &[f64] {
        &self.lons_deg
    }

    /// Return the equinox/baseline ocean circulation (`season 2`, `season_value = 0.0`).
    pub fn ocean(&self) -> &OceanCirculation {
        let idx = if self.ocean_seasons.len() > 2 { 2 } else { 0 };
        &self.ocean_seasons[idx]
    }

    /// Return the ocean circulation for a specific `season_idx` (`0..season_count()`).
    pub fn ocean_for_season(&self, season_idx: usize) -> Option<&OceanCirculation> {
        self.ocean_seasons.get(season_idx)
    }

    /// Downwind destination graph `dest_map[v]` = `[DownwindLink { target_tile, weight }]`.
    pub fn dest_map(&self) -> &[Vec<DownwindLink>] {
        &self.dest_map
    }

    /// Prevailing 3D unit tangent wind vector `[wx, wy, wz]` in grid coordinates at `tile_idx`.
    pub fn wind_vector_xyz(&self, tile_idx: usize) -> [f32; 3] {
        if tile_idx >= self.grid.faces.len() {
            return [0.0, 0.0, 0.0];
        }
        let ([x, y, z], _) = self.grid.faces[tile_idx];
        let xy = (x * x + y * y).sqrt();
        let (east, north) = if xy > 1e-9 {
            (
                [y / xy, -x / xy, 0.0],
                [-x * z / xy, -y * z / xy, xy],
            )
        } else {
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0])
        };
        let dir_rad =
            wind_direction_deg(self.lats_deg[tile_idx], self.cells, self.spin) * PI / 180.0;
        let we = dir_rad.sin();
        let wn = dir_rad.cos();
        [
            (we * east[0] + wn * north[0]) as f32,
            (we * east[1] + wn * north[1]) as f32,
            (we * east[2] + wn * north[2]) as f32,
        ]
    }

    pub fn classifications(&self) -> &[TileClimateClassification] {
        &self.classifications
    }

    pub fn koeppen_ids(&self) -> Vec<u8> {
        self.classifications
            .iter()
            .map(|c| c.koeppen as u8)
            .collect()
    }

    pub fn season_snapshot(&self, season_idx: usize) -> Option<&SeasonalSnapshot> {
        self.seasons.get(season_idx)
    }

    /// Update `cells` and `glaciation` parameters and recompute only what changed.
    pub fn set_params(&mut self, cells: u32, glaciation: f64) {
        let cells = cells.max(1);
        let glaciation = glaciation.clamp(0.0, 1.0);
        if self.cells == cells && (self.glaciation - glaciation).abs() < 1e-9 {
            return;
        }
        self.cells = cells;
        self.glaciation = glaciation;
        self.recompute();
    }

    /// Set `glaciation` without triggering full 8-season ocean recomputation (used before `update_for_tectonics`).
    pub fn set_glaciation_only(&mut self, glaciation: f64) {
        self.glaciation = glaciation.clamp(0.0, 1.0);
    }

    /// Update `spin` (relative to 24h Earth = 1.0) and `tilt_deg` during tectonic evolution,
    /// updating the discrete odd circulation cell count (`1, 3, 5, 7`) and wind `dest_map`
    /// at `0.05` spin resolution.
    pub fn set_spin_and_tilt_for_tectonics(&mut self, spin: f64, tilt_deg: f64) {
        let cells = nominal_cells_for_spin(spin);
        let quant_spin = (spin * 20.0).round() / 20.0;
        self.spin = spin;
        self.tilt_deg = tilt_deg.clamp(0.0, 90.0);
        self.cells = cells;
        let dest_key = WindDestCacheKey {
            cells,
            spin_bits: quant_spin.to_bits(),
        };
        if self.cached_dest_params != Some(dest_key) {
            self.compute_wind_dest_map();
            self.cached_dest_params = Some(dest_key);
        }
    }

    /// Construct a `ClimateModel` attached to a planetary grid for tectonic simulation,
    /// precomputing the wind destination graph `dest_map` for the planet's `spin`.
    pub fn new_for_tectonics(
        grid: SphericalGrid,
        cells: u32,
        spin: f64,
        tilt_deg: f64,
    ) -> Self {
        let n = grid.faces.len();
        let adjacency = grid.adjacency();

        let (lats_deg, lons_deg): (Vec<f64>, Vec<f64>) = grid
            .faces
            .iter()
            .map(|&([x, y, z], _)| {
                let lat = 180.0 / PI * z.atan2((x * x + y * y).sqrt());
                let lon = 180.0 / PI * (-y).atan2(x);
                (lat, lon)
            })
            .unzip();

        let ocean_geom = OceanGridGeometry::new(&grid, &adjacency, &lats_deg);

        let mut model = Self {
            grid,
            adjacency,
            ocean_geom,
            lats_deg,
            lons_deg,
            elevations: vec![0.0; n],
            cells: cells.max(1),
            glaciation: 0.5,
            spin,
            tilt_deg,
            cached_dest_params: None,
            cached_ocean_params: None,
            dest_map: vec![Vec::new(); n],
            seabased: vec![0.0; n],
            ocean_seasons: Vec::new(),
            seasons: Vec::new(),
            classifications: Vec::new(),
        };
        model.compute_wind_dest_map();
        let quant_spin = (spin * 20.0).round() / 20.0;
        model.cached_dest_params = Some(WindDestCacheKey {
            cells: model.cells,
            spin_bits: quant_spin.to_bits(),
        });
        model
    }

    /// Per-tick climate & ocean circulation update across all 8 (half-)seasons (`DEFAULT_SEASONS`:
    /// `[-1.0, -0.5, 0.0, 0.5, 1.0, 0.5, 0.0, -0.5]`, matching `planetsimulation.py:278-286`),
    /// reusing the precomputed wind `dest_map` (`_climatemappings`).
    pub fn update_for_tectonics(&mut self, elevations: &[f32], has_life: bool) {
        assert_eq!(self.elevations.len(), elevations.len());
        self.elevations.copy_from_slice(elevations);
        self.compute_seabreeze();

        self.ocean_seasons = solve_seasonal_ocean_circulation_with_geom(
            &self.ocean_geom,
            &self.lats_deg,
            &self.elevations,
            self.cells,
            self.spin,
            self.tilt_deg,
            self.glaciation,
            &DEFAULT_SEASONS,
        );
        self.cached_ocean_params = Some(OceanCacheKey {
            cells: self.cells,
            spin_bits: self.spin.to_bits(),
            tilt_bits: self.tilt_deg.to_bits(),
            glaciation_bits: self.glaciation.to_bits(),
        });

        self.refresh_seasons_and_classifications(has_life);
    }

    /// Fast per-tick climate update for Pleistocene glaciation & glacio-isostasy steps in
    /// `PrehistorySimulation` where continent positions do not drift (`4,000` yr/tick),
    /// reusing the solved 8-season ocean circulation while updating all 8 half-seasons'
    /// temperature, precipitation, and Köppen classifications for the new `glaciation` and `elevations`.
    pub fn update_for_glaciation_step(&mut self, elevations: &[f32], has_life: bool) {
        if self.ocean_seasons.len() != DEFAULT_SEASONS.len() {
            self.update_for_tectonics(elevations, has_life);
            return;
        }
        assert_eq!(self.elevations.len(), elevations.len());
        self.elevations.copy_from_slice(elevations);
        self.compute_seabreeze();
        self.refresh_seasons_and_classifications(has_life);
    }

    fn refresh_seasons_and_classifications(&mut self, has_life: bool) {
        let n = self.tile_count();
        let glacial_dt = 0.15 * (self.glaciation - 0.5) / 0.5;
        let glacial_dp = 0.2 * (0.5 - self.glaciation) / 0.5;
        let c_f64 = self.cells as f64;
        let templimits = temp_range_for_glaciation(MEAN_TEMPRANGE_C, self.glaciation);

        let snapshots: Vec<SeasonalSnapshot> = DEFAULT_SEASONS
            .iter()
            .enumerate()
            .map(|(s_idx, &season_val)| {
                let ocean = &self.ocean_seasons[s_idx];
                let mut sun_vec = Vec::with_capacity(n);
                let mut temp_vec = Vec::with_capacity(n);
                let mut precip_vec = Vec::with_capacity(n);

                for v in 0..n {
                    let lat = self.lats_deg[v];
                    let h = self.elevations[v] as f64;
                    let sst_inf = ocean.coastal_sst_influence[v] as f64;

                    let ins = insolation(lat, season_val, self.tilt_deg);
                    let base_t = if h > 0.0 {
                        ins * (1.0 - (h / 10.0).min(1.0))
                    } else {
                        ins
                    };
                    let temp = base_t + glacial_dt + 0.06 * sst_inf;
                    let conv =
                        (((lat * 2.0 * c_f64 + self.tilt_deg * season_val) * PI / 180.0).cos()
                            + 1.0)
                            / 2.0;
                    let precip =
                        (self.seabased[v] + conv + glacial_dp + 0.12 * sst_inf).clamp(0.0, 1.0);

                    sun_vec.push(ins as f32);
                    temp_vec.push(temp as f32);
                    precip_vec.push(precip as f32);
                }

                SeasonalSnapshot {
                    season_value: season_val,
                    insolation: sun_vec,
                    temperature: temp_vec,
                    precipitation: precip_vec,
                    ocean: ocean.clone(),
                }
            })
            .collect();

        let mut rts = [0.0f64; DEFAULT_SEASONS.len()];
        let mut rps = [0.0f64; DEFAULT_SEASONS.len()];
        let classifications: Vec<TileClimateClassification> = (0..n)
            .map(|v| {
                for (s_idx, snap) in snapshots.iter().enumerate() {
                    rts[s_idx] = snap.temperature[v] as f64;
                    rps[s_idx] = snap.precipitation[v] as f64;
                }
                classify_tile(
                    self.elevations[v],
                    &rts[..snapshots.len()],
                    &rps[..snapshots.len()],
                    templimits,
                    has_life,
                )
            })
            .collect();

        self.seasons = snapshots;
        self.classifications = classifications;
    }

    fn recompute(&mut self) {
        let dest_key = WindDestCacheKey {
            cells: self.cells,
            spin_bits: self.spin.to_bits(),
        };
        if self.cached_dest_params != Some(dest_key) {
            self.compute_wind_dest_map();
            self.compute_seabreeze();
            self.cached_dest_params = Some(dest_key);
        }

        let ocean_key = OceanCacheKey {
            cells: self.cells,
            spin_bits: self.spin.to_bits(),
            tilt_bits: self.tilt_deg.to_bits(),
            glaciation_bits: self.glaciation.to_bits(),
        };
        if self.cached_ocean_params != Some(ocean_key) {
            self.ocean_seasons = solve_seasonal_ocean_circulation_with_geom(
                &self.ocean_geom,
                &self.lats_deg,
                &self.elevations,
                self.cells,
                self.spin,
                self.tilt_deg,
                self.glaciation,
                &DEFAULT_SEASONS,
            );
            self.cached_ocean_params = Some(ocean_key);
        }

        self.refresh_seasons_and_classifications(true);
    }

    fn compute_wind_dest_map(&mut self) {
        let n = self.tile_count();
        let directions: Vec<f64> = self
            .lats_deg
            .iter()
            .map(|&lat| wind_direction_deg(lat, self.cells, self.spin))
            .collect();

        let mut dests: Vec<Vec<DownwindLink>> = vec![Vec::new(); n];
        let mut seen = vec![false; n];

        for v in 0..n {
            let lat_v = self.lats_deg[v];
            let lon_v = self.lons_deg[v];
            let dir_v = directions[v];

            for &a in &self.adjacency[v] {
                let b = bearing_deg(lat_v, lon_v, self.lats_deg[a], self.lons_deg[a]);
                let w = ((b - dir_v) * PI / 180.0).cos();
                if w > 0.0 {
                    dests[v].push(DownwindLink {
                        target_tile: a,
                        weight: w,
                    });
                    seen[a] = true;
                }
            }
        }

        for v in 0..n {
            if !seen[v] {
                let lat_v = self.lats_deg[v];
                let lon_v = self.lons_deg[v];
                for &a in &self.adjacency[v] {
                    let b = bearing_deg(lat_v, lon_v, self.lats_deg[a], lon_v);
                    let w = -((b - directions[a]) * PI / 180.0).cos();
                    if w > 0.0 {
                        dests[a].push(DownwindLink {
                            target_tile: v,
                            weight: w,
                        });
                    }
                }
            }
        }

        for dws in &mut dests {
            let total: f64 = dws.iter().map(|link| link.weight).sum();
            if total > 0.0 {
                for link in dws.iter_mut() {
                    link.weight /= total;
                }
            }
        }
        self.dest_map = dests;
    }

    fn compute_seabreeze(&mut self) {
        let n = self.tile_count();
        let mut seabreeze: Vec<Option<u32>> = vec![None; n];
        let mut frontier: Vec<usize> = (0..n)
            .filter(|&v| {
                if self.elevations[v] <= 0.0 {
                    seabreeze[v] = Some(0);
                    true
                } else {
                    false
                }
            })
            .collect();

        let mut d = 0;
        while d < BREEZE_DISTANCE && !frontier.is_empty() {
            d += 1;
            let mut next_frontier = Vec::new();
            for &s in &frontier {
                for &link in &self.dest_map[s] {
                    let a = link.target_tile;
                    if seabreeze[a].is_none() {
                        seabreeze[a] = Some(d);
                        next_frontier.push(a);
                    }
                }
            }
            frontier = next_frontier;
        }

        let d_f64 = BREEZE_DISTANCE as f64;
        self.seabased = seabreeze
            .into_iter()
            .map(|opt| opt.map_or(0.0, |b| ((d_f64 - b as f64) / d_f64).powi(2)))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::koeppen::KoeppenCode;

    #[test]
    fn earth_climate_produces_realistic_koeppen_zones() {
        let model = ClimateModel::new_earth(3, 0.5);
        assert_eq!(model.tile_count(), 7292);
        assert_eq!(model.season_count(), 8);

        let mut counts = [0usize; 14];
        for c in model.classifications() {
            counts[c.koeppen as usize] += 1;
        }

        // Ocean tiles should be the majority (~5181 of 7292), and major terrestrial
        // Köppen groups (Tropical Aw, Arid BS/BW, Temperate Cf, Continental Df, Polar ET/EF)
        // must all be populated.
        assert!(counts[KoeppenCode::Ocean as usize] > 5000);
        assert!(counts[KoeppenCode::Aw as usize] > 200);
        assert!(counts[KoeppenCode::BS as usize] > 100);
        assert!(counts[KoeppenCode::BW as usize] > 100);
        assert!(counts[KoeppenCode::Cf as usize] > 200);
        assert!(counts[KoeppenCode::Df as usize] > 200);
        assert!(counts[KoeppenCode::ET as usize] + counts[KoeppenCode::EF as usize] > 50);
    }

    #[test]
    fn ocean_circulation_produces_warm_and_cold_currents() {
        let model = ClimateModel::new_earth(3, 0.5);
        let ocean = model.ocean();
        let warm_count = ocean.sst_anomaly.iter().filter(|&&t| t > 0.15).count();
        let cold_count = ocean.sst_anomaly.iter().filter(|&&t| t < -0.15).count();
        let fast_count = ocean.speed.iter().filter(|&&s| s > 0.25).count();

        assert!(warm_count > 300, "Expected >300 warm current tiles, got {warm_count}");
        assert!(cold_count > 300, "Expected >300 cold current tiles, got {cold_count}");
        assert!(fast_count > 500, "Expected >500 active current tiles, got {fast_count}");
    }

    #[test]
    fn ocean_circulation_varies_across_seasons() {
        let model = ClimateModel::new_earth(3, 0.5);
        // Season 0 is Northern Summer (season_value = -1.0);
        // Season 4 is Northern Winter / Southern Summer (season_value = +1.0).
        let summer_nh = model.ocean_for_season(0).unwrap();
        let winter_nh = model.ocean_for_season(4).unwrap();

        let mut sst_diff_sum = 0.0f32;
        let mut vel_diff_sum = 0.0f32;
        let mut reversed_tropical_tiles = 0usize;

        for v in 0..model.tile_count() {
            if model.elevations()[v] > 0.0 {
                continue;
            }
            sst_diff_sum += (summer_nh.sst_anomaly[v] - winter_nh.sst_anomaly[v]).abs();
            let vs = summer_nh.velocity_xyz[v];
            let vw = winter_nh.velocity_xyz[v];
            let dv = ((vs[0] - vw[0]).powi(2)
                + (vs[1] - vw[1]).powi(2)
                + (vs[2] - vw[2]).powi(2))
            .sqrt();
            vel_diff_sum += dv;

            if model.lats_deg()[v].abs() < 25.0
                && summer_nh.speed[v] > 0.08
                && winter_nh.speed[v] > 0.08
            {
                let dot = vs[0] * vw[0] + vs[1] * vw[1] + vs[2] * vw[2];
                if dot < 0.0 {
                    reversed_tropical_tiles += 1;
                }
            }
        }

        assert!(
            sst_diff_sum > 250.0,
            "Expected significant seasonal SST anomaly shift, got {sst_diff_sum}"
        );
        assert!(
            vel_diff_sum > 200.0,
            "Expected significant seasonal velocity shift, got {vel_diff_sum}"
        );
        assert!(
            reversed_tropical_tiles > 40,
            "Expected tropical/monsoonal current reversals between solstices, got {reversed_tropical_tiles}"
        );
    }

    #[test]
    fn glaciation_modulates_ocean_currents_and_subpolar_sst() {
        let mut model = ClimateModel::new_earth(3, 0.0); // Glacial maximum
        let glacial_ocean = model.ocean().clone();

        model.set_params(3, 1.0); // Greenhouse
        let greenhouse_ocean = model.ocean();

        let mut subpolar_glacial_sst = 0.0f32;
        let mut subpolar_greenhouse_sst = 0.0f32;
        let mut subpolar_glacial_speed = 0.0f32;
        let mut subpolar_greenhouse_speed = 0.0f32;
        let mut subpolar_count = 0usize;

        for v in 0..model.tile_count() {
            if model.elevations()[v] <= 0.0 && model.lats_deg()[v].abs() >= 52.0 {
                subpolar_glacial_sst += glacial_ocean.sst_anomaly[v];
                subpolar_greenhouse_sst += greenhouse_ocean.sst_anomaly[v];
                subpolar_glacial_speed += glacial_ocean.speed[v];
                subpolar_greenhouse_speed += greenhouse_ocean.speed[v];
                subpolar_count += 1;
            }
        }

        assert!(subpolar_count > 500);
        assert!(
            subpolar_greenhouse_sst > subpolar_glacial_sst + 150.0,
            "Expected warmer subpolar SST in greenhouse ({subpolar_greenhouse_sst}) than glacial ({subpolar_glacial_sst})"
        );
        assert!(
            subpolar_greenhouse_speed > subpolar_glacial_speed + 80.0,
            "Expected stronger high-latitude open-water currents in greenhouse ({subpolar_greenhouse_speed}) than under glacial sea ice ({subpolar_glacial_speed})"
        );
    }

    #[test]
    fn changing_glaciation_and_cells_updates_classifications() {
        let mut model = ClimateModel::new_earth(3, 0.5);
        let initial_koeppen = model.koeppen_ids();

        model.set_params(3, 0.0);
        let glacial_koeppen = model.koeppen_ids();
        assert_ne!(initial_koeppen, glacial_koeppen);

        model.set_params(5, 0.5);
        let five_cell_koeppen = model.koeppen_ids();
        assert_ne!(initial_koeppen, five_cell_koeppen);
    }

    #[test]
    fn continuous_rotational_braking_transitions_smoothly_across_day_lengths() {
        // Canonical Tec-Nine points must map to exact odd cell integers:
        assert_eq!(nominal_cells_for_spin(0.5), 1);
        assert_eq!(nominal_cells_for_spin(1.0), 3);
        assert_eq!(nominal_cells_for_spin(2.0), 5);
        assert_eq!(nominal_cells_for_spin(3.0), 7);

        // 24.5h day (spin = 24/24.5 ≈ 0.98) must remain in the 3-cell regime (16h..=32h)
        // rather than collapsing to 1 cell on a step cliff at spin = 1.0:
        let spin_24_5h = 24.0 / 24.5;
        assert_eq!(nominal_cells_for_spin(spin_24_5h), 3);

        // Verify continuous Coriolis & ocean beta updates across 24h -> 25h without a 3->1 cell cliff:
        let grid = SphericalGrid::with_size(4);
        let elevs = vec![0.2f32; grid.faces.len()];
        let mut model = ClimateModel::new_for_tectonics(grid, 3, 1.0, 23.0);
        model.update_for_tectonics(&elevs, true);
        assert_eq!(model.season_count(), 8);
        let precip_24h: Vec<f64> = model
            .classifications()
            .iter()
            .map(|c| c.mean_precipitation)
            .collect();

        model.set_spin_and_tilt_for_tectonics(24.0 / 25.0, 23.0);
        model.update_for_tectonics(&elevs, true);
        let precip_25h: Vec<f64> = model
            .classifications()
            .iter()
            .map(|c| c.mean_precipitation)
            .collect();

        let mean_delta: f64 = precip_24h
            .iter()
            .zip(precip_25h.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / (precip_24h.len() as f64);
        assert!(
            mean_delta < 0.035,
            "Expected smooth transition across 24h -> 25h boundary (no step cliff at spin=1.0), got mean_delta = {:.4}",
            mean_delta
        );
    }
}
