use tec11_geology::SplitSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmSplitSession {
    sim: SplitSandbox,
}

#[wasm_bindgen]
impl WasmSplitSession {
    #[wasm_bindgen(constructor)]
    pub fn new(grid_size: u32, seed: u64) -> Self {
        Self {
            sim: SplitSandbox::new(grid_size, seed),
        }
    }

    pub fn reset(&mut self, grid_size: u32) {
        self.sim.reset(grid_size);
    }

    /// `choice`: `0` = random (`Red` or `Blue`), `1` = `Red`, `2` = `Blue`.
    pub fn iterate(&mut self, choice: u8) {
        let opt = match choice {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        };
        self.sim.iterate(opt);
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn iterations(&self) -> u32 {
        self.sim.iterations()
    }

    pub fn red_count(&self) -> u32 {
        self.sim.red_count() as u32
    }

    pub fn blue_count(&self) -> u32 {
        self.sim.blue_count() as u32
    }

    pub fn tile_states(&self) -> Vec<u8> {
        self.sim.tile_states().to_vec()
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
