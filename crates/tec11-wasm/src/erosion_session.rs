use tec11_geology::ErosionSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmErosionSession {
    sim: ErosionSandbox,
}

#[wasm_bindgen]
impl WasmErosionSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: ErosionSandbox::new_earth(),
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn step_erode(&mut self) {
        self.sim.erode();
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn steps(&self) -> u32 {
        self.sim.steps()
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

    pub fn tile_lost(&self) -> Vec<f32> {
        self.sim.tile_lost().to_vec()
    }

    pub fn tile_gained(&self) -> Vec<f32> {
        self.sim.tile_gained().to_vec()
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
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
