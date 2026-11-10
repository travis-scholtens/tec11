use tec11_geology::LifeformsSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmLifeformsSession {
    sim: LifeformsSandbox,
}

#[wasm_bindgen]
impl WasmLifeformsSession {
    #[wasm_bindgen(constructor)]
    pub fn new(glaciation: f64) -> Self {
        Self {
            sim: LifeformsSandbox::new_earth(glaciation),
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn set_glaciation(&mut self, glaciation: f64) {
        self.sim.set_glaciation(glaciation);
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn season_count(&self) -> usize {
        self.sim.season_count()
    }

    pub fn total_settled_species(&self) -> u32 {
        self.sim.total_settled_species() as u32
    }

    pub fn settled_animal_count(&self) -> u32 {
        self.sim.settled_animal_count() as u32
    }

    pub fn settled_plant_count(&self) -> u32 {
        self.sim.settled_plant_count() as u32
    }

    pub fn is_custom_planet(&self) -> bool {
        self.sim.is_custom_planet()
    }

    pub fn loaded_geoticks(&self) -> u32 {
        self.sim.loaded_geoticks()
    }

    pub fn loaded_land_percent(&self) -> u32 {
        self.sim.loaded_land_percent()
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.sim.tile_elevations().to_vec()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.sim.tile_felsities().to_vec()
    }

    pub fn tile_koeppen_ids(&self) -> Vec<u8> {
        self.sim.tile_koeppen_ids()
    }

    pub fn tile_animal_counts(&self, season_idx: usize) -> Vec<u16> {
        self.sim.tile_animal_counts(season_idx).to_vec()
    }

    pub fn tile_plant_counts(&self, season_idx: usize) -> Vec<u16> {
        self.sim.tile_plant_counts(season_idx).to_vec()
    }

    pub fn nearest_tile(&self, x: f64, y: f64, z: f64) -> usize {
        self.sim.nearest_tile(x, y, z)
    }

    pub fn species_on_tile_tsv(&self, tile_idx: usize, season_idx: usize, attribute: &str) -> String {
        self.sim.species_on_tile_tsv(tile_idx, season_idx, attribute)
    }

    pub fn species_presence_mask(&self, species_id: usize, season_idx: usize) -> Vec<u8> {
        self.sim.species_presence_mask(species_id, season_idx)
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
