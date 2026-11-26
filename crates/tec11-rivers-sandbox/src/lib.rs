use tec11_climate::{ClimateModel, EARTH_GLACIATION, EARTH_SIZE6_ELEVATIONS_M};
use tec11_geology::rock::{Rock, RockKind, RockLayer};
use tec11_geology::tile::{tiles_from_snapshot, TileGeology};
use tec11_math_grid::{normal, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;
use tec11_rivers::trace_coarse_rivers;

/// Standalone rivers sandbox (`riverssimulation.py`, `riversdisplay.py`, `riverspresenter.py`).
pub struct RiversSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    tiles: Vec<TileGeology>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    hmin: f32,
    pmin: f32,
    rivers: Vec<Vec<usize>>,
    tile_river_progress: Vec<f32>,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
}

impl RiversSandbox {
    pub fn new_earth(hmin: f32, pmin: f32) -> Self {
        let grid = SphericalGrid::with_size(6);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let mut tiles = Vec::with_capacity(n);

        for &elev_m in EARTH_SIZE6_ELEVATIONS_M.iter() {
            // Scale Earth's highest mountains into the [0..10 km] tectonic range so `hmin = 5.0`
            // highlights major terrestrial mountain-fed rivers out of the box.
            let elev_km = (elev_m as f32) / 550.0;
            if elev_km > 0.0 {
                let mut t = TileGeology {
                    bottom: -1.0,
                    mountainosity: (elev_km * 2.0).clamp(0.0, 20.0),
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    soil_depth_m: 2.0,
                    soil_fertility: 1.0,
                    layers: vec![RockLayer {
                        rock: Rock {
                            kind: RockKind::Sedimentary,
                            name: "sandstone",
                            felsity: 0.55,
                            toughness: 0.5,
                            clasticity: Some(1.0),
                            calcity: None,
                            bogginess: None,
                            quartzity: None,
                            sulfidity: None,
                            salinity: None,
                        },
                        thickness: elev_km + 1.0,
                    }],
                };
                t.limit();
                t.reset_surficial_soil_from_bedrock();
                tiles.push(t);
            } else {
                let bathy_km = ((elev_m as f32) / 1000.0).clamp(-7.0, -0.2);
                tiles.push(TileGeology {
                    bottom: bathy_km - 5.0,
                    mountainosity: 0.0,
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    soil_depth_m: 0.0,
                    soil_fertility: 1.0,
                    layers: vec![RockLayer {
                        rock: Rock::extrusive(0.5),
                        thickness: 5.0,
                    }],
                });
            }
        }

        let mut climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);
        climate_model.set_glaciation_only(EARTH_GLACIATION);
        let mut s = Self {
            grid,
            adjacency,
            tiles,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_precip_norm: vec![0.0; n],
            hmin,
            pmin,
            rivers: Vec::new(),
            tile_river_progress: vec![0.0; n],
            custom_geoticks: None,
            custom_land_percent: None,
        };
        s.recompute_climate_and_rivers();
        s
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(self.hmin, self.pmin);
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some((grid, tiles, geoticks)) = tiles_from_snapshot(&snap) else {
            return false;
        };
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let land_cnt = tiles.iter().filter(|t| t.elevation() > 0.0).count();
        let land_pct = ((land_cnt as f64) * 100.0 / (n.max(1) as f64)).round() as u32;
        self.grid = grid.clone();
        self.adjacency = adjacency;
        self.tiles = tiles;
        self.climate_model = ClimateModel::new_for_tectonics(grid, 3, 1.0, 23.0);
        self.tile_koeppen = vec![0; n];
        self.tile_life = vec![0.0; n];
        self.tile_precip_norm = vec![0.0; n];
        self.tile_river_progress = vec![0.0; n];
        self.custom_geoticks = Some(geoticks);
        self.custom_land_percent = Some(land_pct);
        self.recompute_climate_and_rivers();
        true
    }

    pub fn set_params(&mut self, hmin: f32, pmin: f32) {
        self.hmin = hmin;
        self.pmin = pmin;
        self.update_rivers();
    }

    fn recompute_climate_and_rivers(&mut self) {
        let n = self.tiles.len();
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);
        self.tile_koeppen.resize(n, 0);
        self.tile_life.resize(n, 0.0);
        self.tile_precip_norm.resize(n, 0.0);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }
        self.update_rivers();
    }

    fn update_rivers(&mut self) {
        let elevations: Vec<f32> = self.tiles.iter().map(|t| t.elevation()).collect();
        self.rivers = trace_coarse_rivers(
            &elevations,
            &self.tile_precip_norm,
            self.climate_model.tile_glacier_cover(),
            &self.adjacency,
            self.hmin,
            self.pmin,
        );

        // `riversdisplay.py:33-37`:
        // `rts[t] = (1 + r.index(t)) / float(len(r))` (0.0 if not in any river).
        self.tile_river_progress.fill(0.0);
        for r in &self.rivers {
            let len = r.len().max(1) as f32;
            for (idx, &t) in r.iter().enumerate() {
                self.tile_river_progress[t] = ((idx + 1) as f32) / len;
            }
        }
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn river_count(&self) -> usize {
        self.rivers.len()
    }

    pub fn rivers(&self) -> &[Vec<usize>] {
        &self.rivers
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

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.tiles.iter().map(|t| t.elevation()).collect()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.tiles.iter().map(|t| t.top_felsity()).collect()
    }

    pub fn tile_koeppen_ids(&self) -> &[u8] {
        &self.tile_koeppen
    }

    pub fn tile_life(&self) -> &[f32] {
        &self.tile_life
    }

    pub fn tile_river_progress(&self) -> &[f32] {
        &self.tile_river_progress
    }

    /// Build 3D line segment endpoints `[x0, y0, z0, x1, y1, z1, ...]` in Three.js mesh coordinates
    /// (`[-v[1], v[2], v[0]]` slightly above the sphere radius `1.004`) along every traced river.
    pub fn river_line_positions(&self) -> Vec<f32> {
        let mut out = Vec::new();
        let r_scale = 1.004f64;
        for r in &self.rivers {
            for w in r.windows(2) {
                let p0 = normal(self.grid.faces[w[0]].0);
                let p1 = normal(self.grid.faces[w[1]].0);
                // Convert grid coordinates `[x, y, z]` to Three.js mesh coordinates `[-y, z, x]`
                out.push((-p0[1] * r_scale) as f32);
                out.push((p0[2] * r_scale) as f32);
                out.push((p0[0] * r_scale) as f32);
                out.push((-p1[1] * r_scale) as f32);
                out.push((p1[2] * r_scale) as f32);
                out.push((p1[0] * r_scale) as f32);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rivers_sandbox_earth() {
        let mut sim = RiversSandbox::new_earth(3.0, 0.5);
        assert_eq!(sim.tile_count(), 7292);
        let initial_rivers = sim.river_count();
        assert!(initial_rivers > 0, "Expected rivers on Earth with hmin=3.0, pmin=0.5");

        // Lowering hmin and pmin should produce at least as many rivers
        sim.set_params(1.5, 0.2);
        assert!(sim.river_count() >= initial_rivers);
    }
}
