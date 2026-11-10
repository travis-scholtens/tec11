use tec11_geology::PopulationSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmPopulationSession {
    sim: PopulationSandbox,
}

#[wasm_bindgen]
impl WasmPopulationSession {
    #[wasm_bindgen(constructor)]
    pub fn new(coast_prox: usize, travel_range: usize) -> Self {
        Self {
            sim: PopulationSandbox::new_earth(coast_prox, travel_range, 42),
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn reset(&mut self) {
        self.sim.reset();
    }

    pub fn set_coast_prox(&mut self, coast_prox: usize) {
        self.sim.set_coast_prox(coast_prox);
    }

    pub fn set_travel_range(&mut self, travel_range: usize) {
        self.sim.set_travel_range(travel_range);
    }

    pub fn step(&mut self) -> bool {
        self.sim.step()
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn populated_count(&self) -> usize {
        self.sim.populated_count()
    }

    pub fn ticks(&self) -> u32 {
        self.sim.ticks()
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

    pub fn populated_mask(&self) -> Vec<u8> {
        self.sim.populated_mask()
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
