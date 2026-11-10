use tec11_geology::MoonSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmMoonSession {
    sim: MoonSandbox,
}

#[wasm_bindgen]
impl WasmMoonSession {
    #[wasm_bindgen(constructor)]
    pub fn new(
        moon_count: u32,
        moon_mass: f32,
        target_moon_dist_radii: u32,
        epoch_pct: u32,
    ) -> Self {
        let mut sim = MoonSandbox::new_earth();
        sim.set_moon_params(
            moon_count,
            moon_mass as f64,
            target_moon_dist_radii as f64,
            epoch_pct,
        );
        Self { sim }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn reset_layers(&mut self) {
        self.sim.reset_layers();
    }

    pub fn set_moon_params(
        &mut self,
        moon_count: u32,
        moon_mass: f32,
        target_moon_dist_radii: u32,
        epoch_pct: u32,
    ) {
        self.sim.set_moon_params(
            moon_count,
            moon_mass as f64,
            target_moon_dist_radii as f64,
            epoch_pct,
        );
    }

    pub fn step_erode(&mut self) {
        self.sim.step_erode();
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

    pub fn current_day_hours(&self) -> f32 {
        self.sim.current_day_hours() as f32
    }

    pub fn current_tilt_deg(&self) -> f32 {
        self.sim.current_tilt_deg() as f32
    }

    pub fn current_moon_dist_radii(&self) -> f32 {
        self.sim.current_moon_dist_radii() as f32
    }

    pub fn current_tidal_strength(&self) -> f32 {
        self.sim.current_tidal_strength() as f32
    }

    pub fn max_tidal_range_m(&self) -> f32 {
        self.sim.max_tidal_range_m()
    }

    pub fn mean_coastal_tidal_scour(&self) -> f32 {
        self.sim.mean_coastal_tidal_scour()
    }

    pub fn coastal_tidal_erosion_share_pct(&self) -> f32 {
        self.sim.coastal_tidal_erosion_share_pct()
    }

    pub fn harmonic_chain_summary(&self) -> String {
        self.sim.harmonic_chain_summary()
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_tidal_scour: bool,
        show_fluvial: bool,
        show_deposition: bool,
        selected_tile: i32,
    ) -> Vec<f32> {
        let sel = if selected_tile >= 0 {
            Some(selected_tile as usize)
        } else {
            None
        };
        self.sim
            .tile_colors_rgb(aspect, show_tidal_scour, show_fluvial, show_deposition, sel)
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
