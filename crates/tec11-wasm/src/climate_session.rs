use tec11_climate::ClimateModel;
use tec11_math_grid::SphericalGrid;
use tec11_proto::{GeologicalTileExt, PlanetSnapshotExt};
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids, rotate_axes};

#[wasm_bindgen]
pub struct WasmClimateSession {
    model: ClimateModel,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
}

#[wasm_bindgen]
impl WasmClimateSession {
    #[wasm_bindgen(constructor)]
    pub fn new(cells: u32, glaciation: f64) -> Self {
        Self {
            model: ClimateModel::new_earth(cells, glaciation),
            custom_geoticks: None,
            custom_land_percent: None,
        }
    }

    /// Reset to the default Earth (`ETOPO2`) planet while preserving `cells` and `glaciation`.
    pub fn load_earth(&mut self) {
        self.model = ClimateModel::new_earth(self.model.cells(), self.model.glaciation());
        self.custom_geoticks = None;
        self.custom_land_percent = None;
    }

    /// Load a `.tec11` binary Protocol Buffer `PlanetSnapshot` into the climate model.
    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let grid_size = snap.grid_size;
        if !(2..=7).contains(&grid_size) {
            return false;
        }
        let Some(geo_tiles) = snap.geological_tiles() else {
            return false;
        };
        let grid = SphericalGrid::with_size(grid_size);
        if geo_tiles.len() != grid.faces.len() {
            return false;
        }
        let elevations: Vec<f32> = geo_tiles
            .into_iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        let land_tiles = elevations.iter().filter(|&&e| e > 0.0).count();
        let land_pct =
            ((100.0 * (land_tiles as f64) / (elevations.len().max(1) as f64)) + 0.5) as u32;

        self.model = ClimateModel::with_elevations(
            grid,
            elevations,
            self.model.cells(),
            self.model.glaciation(),
        );
        self.custom_geoticks = Some(snap.tectonics_ticks());
        self.custom_land_percent = Some(land_pct);
        true
    }

    pub fn is_custom_planet(&self) -> bool {
        self.custom_geoticks.is_some()
    }

    pub fn loaded_geoticks(&self) -> u32 {
        self.custom_geoticks.unwrap_or(0)
    }

    pub fn loaded_land_percent(&self) -> u32 {
        self.custom_land_percent.unwrap_or(29)
    }

    /// Update atmospheric circulation `cells` (`1, 3, 5, 7, 9`) and `glaciation` (`[0.0, 1.0]`).
    pub fn set_params(&mut self, cells: u32, glaciation: f64) {
        self.model.set_params(cells, glaciation);
    }

    pub fn grid_size(&self) -> u32 {
        self.model.grid().size
    }

    pub fn tile_count(&self) -> u32 {
        self.model.tile_count() as u32
    }

    pub fn season_count(&self) -> u32 {
        self.model.season_count() as u32
    }

    pub fn cells(&self) -> u32 {
        self.model.cells()
    }

    pub fn glaciation(&self) -> f64 {
        self.model.glaciation()
    }

    /// Per-tile scaled elevation (`<= 0.0` for ocean, `(0.0..=10.0]` for land).
    pub fn tile_elevations(&self) -> Vec<f32> {
        self.model.elevations().to_vec()
    }

    /// Per-tile Köppen classification code IDs (`0` = Ocean, `1..=13` = `Af..ET`).
    pub fn tile_koeppen_ids(&self) -> Vec<f32> {
        self.model
            .koeppen_ids()
            .into_iter()
            .map(|id| id as f32)
            .collect()
    }

    /// Per-tile solar insolation for season `season_idx` (`0..season_count()`).
    pub fn tile_insolation(&self, season_idx: usize) -> Vec<f32> {
        self.model
            .season_snapshot(season_idx)
            .map(|s| s.insolation.clone())
            .unwrap_or_default()
    }

    /// Per-tile precipitation for season `season_idx` (`0..season_count()`).
    pub fn tile_precipitation(&self, season_idx: usize) -> Vec<f32> {
        self.model
            .season_snapshot(season_idx)
            .map(|s| s.precipitation.clone())
            .unwrap_or_default()
    }

    /// Per-tile temperature for season `season_idx` (`0..season_count()`).
    pub fn tile_temperature(&self, season_idx: usize) -> Vec<f32> {
        self.model
            .season_snapshot(season_idx)
            .map(|s| s.temperature.clone())
            .unwrap_or_default()
    }

    /// Per-tile Sea Surface Temperature (SST) anomaly in `[-1.0, 1.0]` for season `season_idx`
    /// (`> 0` warm current, `< 0` cold current/upwelling).
    pub fn tile_ocean_sst_anomaly(&self, season_idx: usize) -> Vec<f32> {
        self.model
            .ocean_for_season(season_idx)
            .unwrap_or_else(|| self.model.ocean())
            .sst_anomaly
            .clone()
    }

    /// Per-tile normalized ocean current speed in `[0.0, 1.0]` for season `season_idx` (`0.0` on land).
    pub fn tile_ocean_speed(&self, season_idx: usize) -> Vec<f32> {
        self.model
            .ocean_for_season(season_idx)
            .unwrap_or_else(|| self.model.ocean())
            .speed
            .clone()
    }

    /// 3D line-segment positions (`[x0, y0, z0, x1, y1, z1, ...]`) for ocean current flow arrows
    /// (shaft + 2 arrowhead barbs per active ocean tile) for season `season_idx`.
    pub fn ocean_arrow_positions(&self, season_idx: usize) -> Vec<f32> {
        let grid = self.model.grid();
        let ocean = self
            .model
            .ocean_for_season(season_idx)
            .unwrap_or_else(|| self.model.ocean());
        let elevations = self.model.elevations();
        let n = grid.faces.len();
        let mut out = Vec::with_capacity(n * 18);

        for v in 0..n {
            if elevations[v] > 0.0 {
                continue;
            }
            let spd = ocean.speed[v];
            if spd < 0.05 {
                continue;
            }
            let vel = ocean.velocity_xyz[v];
            let vmag = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
            if vmag < 1e-6 {
                continue;
            }
            let dx = (vel[0] / vmag) as f64;
            let dy = (vel[1] / vmag) as f64;
            let dz = (vel[2] / vmag) as f64;

            let ([nx, ny, nz], _) = grid.faces[v];
            // Perpendicular tangent vector p = n x d on the sphere surface.
            let px = ny * dz - nz * dy;
            let py = nz * dx - nx * dz;
            let pz = nx * dy - ny * dx;

            let r = 1.003;
            let cx = nx * r;
            let cy = ny * r;
            let cz = nz * r;

            let half_len = 0.007 + 0.013 * (spd as f64).sqrt();
            let tail = [
                cx - half_len * dx,
                cy - half_len * dy,
                cz - half_len * dz,
            ];
            let tip = [
                cx + half_len * dx,
                cy + half_len * dy,
                cz + half_len * dz,
            ];
            let barb_back = 0.65 * half_len;
            let barb_wing = 0.42 * half_len;
            let left = [
                tip[0] - barb_back * dx + barb_wing * px,
                tip[1] - barb_back * dy + barb_wing * py,
                tip[2] - barb_back * dz + barb_wing * pz,
            ];
            let right = [
                tip[0] - barb_back * dx - barb_wing * px,
                tip[1] - barb_back * dy - barb_wing * py,
                tip[2] - barb_back * dz - barb_wing * pz,
            ];

            for (a, b) in [(tail, tip), (tip, left), (tip, right)] {
                let ra = rotate_axes(a);
                let rb = rotate_axes(b);
                out.extend_from_slice(&[ra[0], ra[1], ra[2], rb[0], rb[1], rb[2]]);
            }
        }

        out
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        build_mesh_positions(self.model.grid())
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        build_mesh_normals(self.model.grid())
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        build_vertex_tile_ids(self.model.grid())
    }
}
