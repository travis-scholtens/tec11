use tec11_geology::MoveSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmMoveSession {
    sim: MoveSandbox,
}

#[wasm_bindgen]
impl WasmMoveSession {
    #[wasm_bindgen(constructor)]
    pub fn new(grid_size: u32, direction_deg: f64, speed_deg: f64, seed: u64) -> Self {
        Self {
            sim: MoveSandbox::new(grid_size, direction_deg, speed_deg, seed),
        }
    }

    pub fn reset(&mut self, grid_size: u32, direction_deg: f64, speed_deg: f64) {
        self.sim.reset(grid_size, direction_deg, speed_deg);
    }

    pub fn step(&mut self) {
        self.sim.step();
    }

    pub fn set_direction_deg(&mut self, direction_deg: f64) {
        self.sim.set_direction_deg(direction_deg);
    }

    pub fn set_speed_deg(&mut self, speed_deg: f64) {
        self.sim.set_speed_deg(speed_deg);
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn group_tile_count(&self) -> u32 {
        self.sim.group_tile_count() as u32
    }

    pub fn steps(&self) -> u32 {
        self.sim.steps()
    }

    pub fn direction_deg(&self) -> f64 {
        self.sim.direction_deg()
    }

    pub fn speed_deg(&self) -> f64 {
        self.sim.speed_deg()
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
