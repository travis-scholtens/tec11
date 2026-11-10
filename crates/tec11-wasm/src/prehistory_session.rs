use tec11_geology::PrehistorySimulation;
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
