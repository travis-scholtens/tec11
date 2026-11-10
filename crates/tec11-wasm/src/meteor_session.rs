use tec11_geology::MeteorSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmMeteorSession {
    sim: MeteorSandbox,
}

#[wasm_bindgen]
impl WasmMeteorSession {
    #[wasm_bindgen(constructor)]
    pub fn new(diameter_km: f32, velocity_km_s: f32, density_kg_m3: f32) -> Self {
        let mut sim = MeteorSandbox::new_earth();
        sim.set_impactor_params(
            diameter_km as f64,
            velocity_km_s as f64,
            density_kg_m3 as f64,
        );
        Self { sim }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn reset_planet(&mut self) {
        self.sim.reset_planet();
    }

    pub fn set_impactor_params(&mut self, diameter_km: f32, velocity_km_s: f32, density_kg_m3: f32) {
        self.sim.set_impactor_params(
            diameter_km as f64,
            velocity_km_s as f64,
            density_kg_m3 as f64,
        );
    }

    pub fn strike_tile(&mut self, target_tile: i32) -> usize {
        let tgt = if target_tile >= 0 {
            Some(target_tile as usize)
        } else {
            None
        };
        self.sim.strike_tile(tgt)
    }

    pub fn strike_barrage(&mut self, count: u32) -> usize {
        self.sim.strike_barrage(count)
    }

    pub fn step_recover_and_erode(&mut self) {
        self.sim.step_recover_and_erode();
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn impact_count(&self) -> u32 {
        self.sim.impact_count()
    }

    pub fn recovery_steps(&self) -> u32 {
        self.sim.recovery_steps()
    }

    pub fn current_cooling_c(&self) -> f32 {
        self.sim.current_cooling_c()
    }

    pub fn biosphere_survival_pct(&self) -> f32 {
        self.sim.biosphere_survival_pct()
    }

    pub fn configured_crater_diameter_km(&self) -> f32 {
        self.sim.configured_crater_diameter_km() as f32
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

    pub fn impact_summary(&self) -> String {
        self.sim.impact_summary()
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_ejecta: bool,
        show_blast: bool,
        selected_tile: i32,
    ) -> Vec<f32> {
        let sel = if selected_tile >= 0 {
            Some(selected_tile as usize)
        } else {
            None
        };
        self.sim
            .tile_colors_rgb(aspect, show_ejecta, show_blast, sel)
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
