use tec11_geology::{WorldCreateParams, WorldSimulation};
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmWorldSession {
    sim: WorldSimulation,
}

#[wasm_bindgen]
impl WasmWorldSession {
    #[wasm_bindgen(constructor)]
    pub fn new(
        grid_size: u32,
        day_hours: u32,
        tilt_deg: u32,
        land_pct: u32,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        people_dt_myr: u32,
    ) -> Self {
        let params = WorldCreateParams {
            grid_size,
            day_hours,
            tilt_deg,
            land_pct,
            atm_dt_myr,
            life_dt_myr,
            people_dt_myr,
        };
        Self {
            sim: WorldSimulation::new(params, 42),
        }
    }

    pub fn create_world(
        &mut self,
        grid_size: u32,
        day_hours: u32,
        tilt_deg: u32,
        land_pct: u32,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        people_dt_myr: u32,
        randomize: bool,
    ) {
        let params = WorldCreateParams {
            grid_size,
            day_hours,
            tilt_deg,
            land_pct,
            atm_dt_myr,
            life_dt_myr,
            people_dt_myr,
        };
        self.sim.create(params, randomize);
    }

    pub fn step(&mut self) {
        self.sim.step();
    }

    pub fn advance_stage(&mut self) {
        self.sim.advance_stage();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.sim.encode_snapshot()
    }

    pub fn stage_index(&self) -> u32 {
        self.sim.stage_index()
    }

    pub fn stage_name(&self) -> String {
        self.sim.stage_name().to_string()
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn years_formatted(&self) -> String {
        self.sim.years_formatted()
    }

    pub fn total_population_thousands(&self) -> f32 {
        self.sim.total_population_thousands()
    }

    pub fn param_grid_size(&self) -> u32 {
        self.sim.params().grid_size
    }

    pub fn param_day_hours(&self) -> u32 {
        self.sim.params().day_hours
    }

    pub fn param_tilt_deg(&self) -> u32 {
        self.sim.params().tilt_deg
    }

    pub fn param_land_pct(&self) -> u32 {
        self.sim.params().land_pct
    }

    pub fn param_atm_dt_myr(&self) -> u32 {
        self.sim.params().atm_dt_myr
    }

    pub fn param_life_dt_myr(&self) -> u32 {
        self.sim.params().life_dt_myr
    }

    pub fn param_people_dt_myr(&self) -> u32 {
        self.sim.params().people_dt_myr
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn tile_colors_rgb(&self, aspect: &str, selected_tile: i32) -> Vec<f32> {
        let sel = if selected_tile >= 0 {
            Some(selected_tile as usize)
        } else {
            None
        };
        self.sim.tile_colors_rgb(aspect, sel)
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
