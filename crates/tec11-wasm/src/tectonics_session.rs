use tec11_geology::RockKind;
use tec11_tectonics::TectonicsSimulation;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmTectonicsSession {
    sim: TectonicsSimulation,
    seed: u64,
    biosphere_cache: tec11_biosphere::BiosphereCache,
}

#[wasm_bindgen]
impl WasmTectonicsSession {
    #[wasm_bindgen(constructor)]
    pub fn new(grid_size: u32, land_percent: f64, seed: u64) -> Self {
        let sim = TectonicsSimulation::new(grid_size, land_percent, seed);
        Self {
            sim,
            seed,
            biosphere_cache: tec11_biosphere::BiosphereCache::new(),
        }
    }

    pub fn reset(&mut self, grid_size: u32, land_percent: f64) {
        self.seed = self.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        self.sim = TectonicsSimulation::new(grid_size, land_percent, self.seed);
        self.biosphere_cache.clear();
    }

    pub fn step(&mut self) {
        self.sim.step();
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn geoticks(&self) -> u32 {
        self.sim.geoticks()
    }

    pub fn continent_count(&self) -> u32 {
        self.sim.continent_count() as u32
    }

    pub fn land_percent_actual(&self) -> u32 {
        self.sim.land_percent()
    }

    pub fn has_atmosphere(&self) -> bool {
        self.sim.has_atmosphere()
    }

    pub fn has_life(&self) -> bool {
        self.sim.has_life()
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.elevation()).collect()
    }

    pub fn tile_thicknesses(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.thickness()).collect()
    }

    pub fn tile_mountainosities(&self) -> Vec<f32> {
        self.sim
            .tiles()
            .iter()
            .map(|t| t.mountainosity_norm())
            .collect()
    }

    pub fn tile_subductions(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.subduction).collect()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.top_felsity()).collect()
    }

    /// Top rock kind code per tile:
    /// - 0: Igneous (`'I'`)
    /// - 1: Sedimentary (`'S'`)
    /// - 2: Metamorphic (`'M'`)
    pub fn tile_top_rock_kinds(&self) -> Vec<u8> {
        self.sim
            .tiles()
            .iter()
            .map(|t| match t.top_rock_kind() {
                RockKind::Igneous => 0,
                RockKind::Sedimentary => 1,
                RockKind::Metamorphic => 2,
            })
            .collect()
    }

    pub fn tile_koeppen_ids(&self) -> Vec<u8> {
        self.sim.tile_koeppen_ids().to_vec()
    }

    pub fn tile_life(&self) -> Vec<f32> {
        self.sim.tile_life().to_vec()
    }

    pub fn tile_impact_scars(&self) -> Vec<f32> {
        self.sim
            .tiles()
            .iter()
            .enumerate()
            .map(|(i, t)| self.sim.tile_impact_scars()[i].max(t.impact_signature()))
            .collect()
    }

    pub fn impact_count(&self) -> u32 {
        self.sim.impact_count()
    }

    pub fn nearest_tile(&self, x: f64, y: f64, z: f64) -> usize {
        self.sim.nearest_tile([x, y, z])
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn inspect_tile_stratigraphy_json(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile_stratigraphy_json(tile_idx)
    }

    pub fn inspect_tile_structured_json(&self, tile_idx: usize) -> String {
        let Some(tile) = self.sim.tiles().get(tile_idx) else {
            return "{\"hasTile\":false}".to_string();
        };
        let (home_star, optics, solar_system, constellations, _) =
            tec11_sky_view::resolve_sky_environment(
                self.sim.saved_environment(),
                self.sim.grid().size,
                self.sim.radius_km() as f32,
                self.sim.current_spin() as f32,
                self.sim.current_tilt_deg() as f32,
                self.sim.land_percent(),
                self.sim.moon_count(),
                0x517C_C1B7_2722_0A95 ^ (self.sim.grid().size as u64),
            );
        let elev = tile.elevation();
        let has_atm = self.sim.has_atmosphere();
        let has_life = self.sim.has_life();
        let loc_summary = if !has_atm {
            format!("Pre-atmosphere Crust | Elevation: {elev:+.2} km")
        } else if elev <= 0.0 {
            format!(
                "{} | Bathymetry: {elev:.2} km",
                tec11_tectonics::classify_submarine_tile(tile)
            )
        } else {
            let k_code = self.sim.tile_koeppen_ids().get(tile_idx).copied().unwrap_or(0);
            format!(
                "{} | Elevation: +{elev:.2} km",
                tec11_koeppen::koeppen_climate_name(k_code)
            )
        };
        let surf_infos: Vec<tec11_sky_view::TileSurfaceInfo> = self
            .sim
            .tiles()
            .iter()
            .map(|t| tec11_sky_view::TileSurfaceInfo::from_tuple(t.surface_info_tuple()))
            .collect();
        let Some(mut view) = tec11_sky_view::build_coarse_tile_inspection_view(
            tile_idx,
            tec11_sky_view::CoarseTileSkyParams {
                grid: self.sim.grid(),
                adjacency: self.sim.adjacency(),
                tiles: &surf_infos,
                koeppen_ids: self.sim.tile_koeppen_ids(),
                glacier_cover: self.sim.climate_model().tile_glacier_cover(),
                tile_life: self.sim.tile_life(),
                river_tiles: None,
                radius_km: self.sim.radius_km(),
                day_hours: self.sim.current_day_hours(),
                tilt_deg: self.sim.current_tilt_deg(),
                has_atmosphere: has_atm,
                has_life,
                current_year: 0,
                moon_count: self.sim.moon_count(),
                home_star: &home_star,
                optics: &optics,
                solar_system: &solar_system,
                constellations: &constellations,
                title: format!("Coarse Tile #{tile_idx}"),
                location_summary: loc_summary,
                population_summary: "Pre-human Geological Epoch".to_string(),
                technology: if has_life {
                    "Biosphere".to_string()
                } else if has_atm {
                    "Hydrosphere".to_string()
                } else {
                    "Hadean".to_string()
                },
                stratigraphy_json: self.sim.inspect_tile_stratigraphy_json(tile_idx),
            },
        ) else {
            return "{\"hasTile\":false}".to_string();
        };
        if has_atm && has_life {
            let (flora, fauna) = self
                .biosphere_cache
                .select_salient_flora_and_fauna_names(
                    self.sim.geoticks(),
                    || self.sim.tiles().iter().map(|t| t.elevation()).collect(),
                    self.sim.climate_model(),
                    tile_idx,
                    6,
                    6,
                );
            view.flora = flora;
            view.fauna = fauna;
        }
        view.to_json()
    }

    pub fn export_snapshot_bytes(&self) -> Vec<u8> {
        self.sim.encode_snapshot()
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        if let Some(loaded) = TectonicsSimulation::decode_snapshot(bytes, self.seed) {
            self.sim = loaded;
            self.biosphere_cache.clear();
            true
        } else {
            false
        }
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        build_mesh_positions(self.sim.grid())
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        build_mesh_normals(self.sim.grid())
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        build_vertex_tile_ids(self.sim.grid())
    }
}
