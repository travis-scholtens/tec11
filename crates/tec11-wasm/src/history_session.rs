use tec11_geology::HistorySimulation;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmHistorySession {
    sim: HistorySimulation,
}

#[wasm_bindgen]
impl WasmHistorySession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: HistorySimulation::new_earth(42),
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

    pub fn history_ticks(&self) -> u32 {
        self.sim.history_ticks()
    }

    pub fn years(&self) -> u32 {
        self.sim.years()
    }

    pub fn is_custom_planet(&self) -> bool {
        self.sim.is_custom_planet()
    }

    pub fn land_percent(&self) -> u32 {
        self.sim.land_percent()
    }

    pub fn active_state_count(&self) -> usize {
        self.sim.active_state_count()
    }

    pub fn total_population_thousands(&self) -> f32 {
        self.sim.total_population_thousands()
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_rivers: bool,
        selected_tile: i32,
    ) -> Vec<f32> {
        let sel = if selected_tile >= 0 {
            Some(selected_tile as usize)
        } else {
            None
        };
        self.sim.tile_colors_rgb(aspect, show_rivers, sel)
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
