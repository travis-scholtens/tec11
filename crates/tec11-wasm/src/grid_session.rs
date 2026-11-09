use tec11_math_grid::SphericalGrid;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmGridSession {
    grid: SphericalGrid,
}

#[wasm_bindgen]
impl WasmGridSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            grid: SphericalGrid::new(),
        }
    }

    pub fn subdivide(&mut self) {
        self.grid = self.grid.subdivide();
    }

    pub fn size(&self) -> u32 {
        self.grid.size
    }

    pub fn tile_count(&self) -> u32 {
        self.grid.faces.len() as u32
    }

    pub fn tile_centers(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.grid.faces.len() * 3);
        for (center, _) in &self.grid.faces {
            out.push(center[0] as f32);
            out.push(center[1] as f32);
            out.push(center[2] as f32);
        }
        out
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        build_mesh_positions(&self.grid)
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        build_mesh_normals(&self.grid)
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        build_vertex_tile_ids(&self.grid)
    }
}

impl Default for WasmGridSession {
    fn default() -> Self {
        Self::new()
    }
}
