use tec11_geology::RacinationSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmRacinationSession {
    sim: RacinationSandbox,
}

#[wasm_bindgen]
impl WasmRacinationSession {
    #[wasm_bindgen(constructor)]
    pub fn new(travel_range: usize) -> Self {
        Self {
            sim: RacinationSandbox::new(travel_range, 42),
        }
    }

    pub fn reset(&mut self) {
        self.sim.reset();
    }

    pub fn set_travel_range(&mut self, travel_range: usize) {
        self.sim.set_travel_range(travel_range);
    }

    pub fn isolate(&mut self) {
        self.sim.isolate();
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn peoples(&self) -> usize {
        self.sim.peoples()
    }

    pub fn populated_count(&self) -> usize {
        self.sim.populated_count()
    }

    pub fn isolations(&self) -> u32 {
        self.sim.isolations()
    }

    pub fn tile_colors_rgb(&self) -> Vec<f32> {
        self.sim.tile_colors_rgb()
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
