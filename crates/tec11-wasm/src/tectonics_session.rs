use tec11_geology::{RockKind, TectonicsSimulation};
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmTectonicsSession {
    sim: TectonicsSimulation,
    seed: u64,
}

#[wasm_bindgen]
impl WasmTectonicsSession {
    #[wasm_bindgen(constructor)]
    pub fn new(grid_size: u32, land_percent: f64, seed: u64) -> Self {
        let sim = TectonicsSimulation::new(grid_size, land_percent, seed);
        Self { sim, seed }
    }

    pub fn reset(&mut self, grid_size: u32, land_percent: f64) {
        self.seed = self.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        self.sim = TectonicsSimulation::new(grid_size, land_percent, self.seed);
    }

    pub fn step(&mut self) {
        self.sim.step();
    }

    pub fn grid_size(&self) -> u32 {
        self.sim.grid().size
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn geoticks(&self) -> u32 {
        self.sim.geoticks()
    }

    pub fn continent_count(&self) -> u32 {
        self.sim.continent_count() as u32
    }

    pub fn land_percent_actual(&self) -> u32 {
        self.sim.land_percent()
    }

    pub fn has_atmosphere(&self) -> bool {
        self.sim.has_atmosphere()
    }

    pub fn has_life(&self) -> bool {
        self.sim.has_life()
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.elevation()).collect()
    }

    pub fn tile_thicknesses(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.thickness()).collect()
    }

    pub fn tile_mountainosities(&self) -> Vec<f32> {
        self.sim
            .tiles()
            .iter()
            .map(|t| t.mountainosity_norm())
            .collect()
    }

    pub fn tile_subductions(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.subduction).collect()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.sim.tiles().iter().map(|t| t.top_felsity()).collect()
    }

    /// Top rock kind code per tile:
    /// - 0: Igneous (`'I'`)
    /// - 1: Sedimentary (`'S'`)
    /// - 2: Metamorphic (`'M'`)
    pub fn tile_top_rock_kinds(&self) -> Vec<u8> {
        self.sim
            .tiles()
            .iter()
            .map(|t| match t.top_rock_kind() {
                RockKind::Igneous => 0,
                RockKind::Sedimentary => 1,
                RockKind::Metamorphic => 2,
            })
            .collect()
    }

    pub fn tile_koeppen_ids(&self) -> Vec<u8> {
        self.sim.tile_koeppen_ids().to_vec()
    }

    pub fn tile_life(&self) -> Vec<f32> {
        self.sim.tile_life().to_vec()
    }

    pub fn nearest_tile(&self, x: f64, y: f64, z: f64) -> usize {
        self.sim.nearest_tile([x, y, z])
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
