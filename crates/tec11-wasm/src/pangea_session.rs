use tec11_geology::PangeaWorld;
use tec11_math_grid::Pcg64Mcg;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmPangeaSession {
    rng: Pcg64Mcg,
    world: PangeaWorld,
}

#[wasm_bindgen]
impl WasmPangeaSession {
    #[wasm_bindgen(constructor)]
    pub fn new(grid_size: u32, land_percent: f64, seed: u64) -> Self {
        let mut rng = Pcg64Mcg::from_seed_u64(seed);
        let world = PangeaWorld::generate(grid_size, land_percent, &mut rng);
        Self { rng, world }
    }

    /// Generates a new random initial landmass (Pangaea) and ocean floor.
    pub fn regenerate(&mut self, grid_size: u32, land_percent: f64) {
        self.world = PangeaWorld::generate(grid_size, land_percent, &mut self.rng);
    }

    pub fn grid_size(&self) -> u32 {
        self.world.grid.size
    }

    pub fn tile_count(&self) -> u32 {
        self.world.tiles.len() as u32
    }

    pub fn land_percent_actual(&self) -> u32 {
        self.world.land_percent_actual()
    }

    /// Per-tile signed elevation in km (`> 0` for continental landmass, `<= 0` for ocean floor).
    pub fn tile_elevations(&self) -> Vec<f32> {
        self.world.tiles.iter().map(|t| t.elevation()).collect()
    }

    /// Per-tile surface rock felsity (silica fraction in `[0.5, 1.0]`).
    pub fn tile_felsities(&self) -> Vec<f32> {
        self.world.tiles.iter().map(|t| t.top_felsity()).collect()
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        build_mesh_positions(&self.world.grid)
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        build_mesh_normals(&self.world.grid)
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        build_vertex_tile_ids(&self.world.grid)
    }
}
