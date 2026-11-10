use tec11_geology::RiversSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmRiversSession {
    sim: RiversSandbox,
}

#[wasm_bindgen]
impl WasmRiversSession {
    #[wasm_bindgen(constructor)]
    pub fn new(hmin: f32, pmin: f32) -> Self {
        Self {
            sim: RiversSandbox::new_earth(hmin, pmin),
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn set_params(&mut self, hmin: f32, pmin: f32) {
        self.sim.set_params(hmin, pmin);
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn river_count(&self) -> u32 {
        self.sim.river_count() as u32
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

    pub fn tile_river_progress(&self) -> Vec<f32> {
        self.sim.tile_river_progress().to_vec()
    }

    pub fn river_line_positions(&self) -> Vec<f32> {
        self.sim.river_line_positions()
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
