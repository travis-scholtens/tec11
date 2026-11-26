use tec11_prehistory::PrehistorySimulation;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmPrehistorySession {
    sim: PrehistorySimulation,
}

#[wasm_bindgen]
impl WasmPrehistorySession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: PrehistorySimulation::new_earth(42),
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.sim.encode_snapshot()
    }

    pub fn reset(&mut self) {
        self.sim.reset();
    }

    pub fn step(&mut self) {
        self.sim.step();
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn tectonics_ticks(&self) -> u32 {
        self.sim.tectonics_ticks()
    }

    pub fn prehistory_ticks(&self) -> u32 {
        self.sim.prehistory_ticks()
    }

    pub fn glaciation(&self) -> f64 {
        self.sim.glaciation()
    }

    pub fn is_custom_planet(&self) -> bool {
        self.sim.is_custom_planet()
    }

    pub fn land_percent(&self) -> u32 {
        self.sim.land_percent()
    }

    pub fn populated_count(&self) -> usize {
        self.sim.populated_count()
    }

    pub fn active_race_count(&self) -> usize {
        self.sim.active_race_count()
    }

    pub fn active_agricultural_race_count(&self) -> usize {
        self.sim.active_agricultural_race_count()
    }

    pub fn total_population_thousands(&self) -> f32 {
        self.sim.total_population_thousands()
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.sim.tile_elevations()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.sim.tile_felsities()
    }

    pub fn tile_koeppen_ids(&self) -> Vec<u8> {
        self.sim.tile_koeppen_ids().to_vec()
    }

    pub fn tile_life(&self) -> Vec<f32> {
        self.sim.tile_life().to_vec()
    }

    pub fn river_mask(&self) -> Vec<u8> {
        self.sim.river_mask()
    }

    pub fn population_overlay_colors(&self) -> Vec<f32> {
        self.sim.population_overlay_colors()
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
        let (home_star, optics, solar_system, constellations, moon_count) =
            tec11_sky_view::resolve_sky_environment(
                self.sim.saved_environment(),
                self.sim.grid().size,
                self.sim.radius_km() as f32,
                self.sim.spin() as f32,
                self.sim.tilt_deg() as f32,
                self.sim.land_percent(),
                1,
                0x517C_C1B7_2722_0A95,
            );
        let elev = tile.elevation();
        let isl_k = self.sim.subgrid_island_koeppen().get(tile_idx).copied().unwrap_or(0);
        let is_subgrid_island = elev <= 0.0 && isl_k != 0;
        let koeppen = if is_subgrid_island {
            isl_k
        } else {
            self.sim.tile_koeppen_ids().get(tile_idx).copied().unwrap_or(0)
        };
        let loc_summary = if is_subgrid_island {
            format!(
                "Sub-grid Island ({}) | Bathymetry: {elev:.2} km",
                tec11_koeppen::koeppen_climate_name(koeppen)
            )
        } else if elev <= 0.0 {
            format!("Marine Basin | Bathymetry: {elev:.2} km")
        } else {
            format!(
                "{} | Elevation: +{elev:.2} km",
                tec11_koeppen::koeppen_climate_name(koeppen)
            )
        };
        let (pop_summary, tech_str) = if let Some(&hid) = self.sim.populated().get(&tile_idx) {
            let is_ag = self.sim.agricultural_heritages().contains(&hid);
            let is_riv = self.sim.rivergoing_heritages().contains(&hid);
            let is_sea = self.sim.seafaring_heritages().contains(&hid);
            let pop_k = self.sim.tile_population_for_heritage_thousands(tile_idx, hid);
            let mut tech = Vec::new();
            if is_ag {
                tech.push("Agricultural");
            }
            if is_riv {
                tech.push("Rivergoing");
            }
            if is_sea {
                tech.push("Seafaring");
            }
            if tech.is_empty() {
                tech.push("Paleolithic");
            }
            (format!("{pop_k:.1}K (Heritage #{hid})"), tech.join(" · "))
        } else {
            ("Uninhabited".to_string(), "Prehistory".to_string())
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
                river_tiles: Some(self.sim.river_tiles()),
                radius_km: self.sim.radius_km(),
                day_hours: 24.0 / self.sim.spin().max(0.1),
                tilt_deg: self.sim.tilt_deg(),
                has_atmosphere: true,
                has_life: true,
                current_year: 0,
                moon_count,
                home_star: &home_star,
                optics: &optics,
                solar_system: &solar_system,
                constellations: &constellations,
                title: format!("Coarse Tile #{tile_idx}"),
                location_summary: loc_summary,
                population_summary: pop_summary,
                technology: tech_str,
                stratigraphy_json: self.sim.inspect_tile_stratigraphy_json(tile_idx),
            },
        ) else {
            return "{\"hasTile\":false}".to_string();
        };
        let (flora, fauna) = self.sim.salient_flora_and_fauna_for_tile(tile_idx, 6, 6);
        view.flora = flora;
        view.fauna = fauna;
        view.to_json()
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
