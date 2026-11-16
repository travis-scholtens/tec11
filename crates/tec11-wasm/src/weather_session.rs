use tec11_geology::WeatherSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};
use crate::tides_session::build_fine_arrow_segments;

#[wasm_bindgen]
pub struct WasmWeatherSession {
    sandbox: WeatherSandbox,
}

#[wasm_bindgen]
impl WasmWeatherSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sandbox: WeatherSandbox::new_earth(),
        }
    }

    pub fn from_snapshot_or_earth(bytes: &[u8]) -> Self {
        let sandbox = if bytes.is_empty() {
            WeatherSandbox::new_earth()
        } else {
            WeatherSandbox::from_snapshot_bytes(bytes).unwrap_or_else(WeatherSandbox::new_earth)
        };
        Self { sandbox }
    }

    pub fn load_earth(&mut self) {
        self.sandbox.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sandbox.load_snapshot_bytes(bytes)
    }

    pub fn reset_weather(&mut self) {
        self.sandbox.reset_weather();
    }

    pub fn step_hours(&mut self, hours: u32) {
        self.sandbox.step_hours(hours);
    }

    pub fn set_day_of_year(&mut self, day: u32) {
        self.sandbox.set_day_of_year(day);
    }

    pub fn set_environment_params(
        &mut self,
        atmosphere_id: u32,
        glaciation: f32,
        moon_count: u32,
        primary_mass: f32,
        primary_dist_radii: f32,
    ) {
        self.sandbox.set_environment_params(
            atmosphere_id,
            glaciation,
            moon_count,
            primary_mass,
            primary_dist_radii,
        );
    }

    pub fn planet_name(&self) -> String {
        self.sandbox.base.planet_name()
    }

    pub fn coarse_grid_size(&self) -> u32 {
        self.sandbox.base.fine.coarse_size
    }

    pub fn fine_grid_size(&self) -> u32 {
        self.sandbox.base.fine.grid.size
    }

    pub fn tile_count(&self) -> u32 {
        self.sandbox.base.fine.grid.faces.len() as u32
    }

    pub fn days_per_year(&self) -> u32 {
        (self.sandbox.base.days_per_year().round() as u32).max(1)
    }

    pub fn sim_day_of_year(&self) -> u32 {
        self.sandbox.solver.tides.current_sim_calendar().1
    }

    pub fn elapsed_hours(&self) -> u32 {
        self.sandbox.solver.tides.elapsed_hours() as u32
    }

    pub fn sim_clock_label(&self) -> String {
        self.sandbox.sim_clock_label()
    }

    pub fn atmosphere_id(&self) -> u32 {
        self.sandbox.solver.atmosphere_id()
    }

    pub fn atmosphere_name(&self) -> String {
        format!(
            "{} · {}",
            self.sandbox.base.home_star.spectral_type,
            self.sandbox.base.atmosphere.name()
        )
    }

    pub fn glaciation(&self) -> f32 {
        self.sandbox.solver.glaciation()
    }

    pub fn moon_count(&self) -> u32 {
        self.sandbox.solver.tides.moon_count()
    }

    pub fn moon_mass(&self) -> f32 {
        self.sandbox.solver.tides.primary_mass_lunar()
    }

    pub fn moon_dist_radii(&self) -> f32 {
        self.sandbox.solver.tides.primary_dist_radii()
    }

    pub fn global_mean_temp_c(&self) -> f32 {
        self.sandbox.global_mean_temp_c()
    }

    pub fn min_slp_hpa(&self) -> f32 {
        self.sandbox.min_slp_hpa()
    }

    pub fn max_slp_hpa(&self) -> f32 {
        self.sandbox.max_slp_hpa()
    }

    pub fn max_wind_mps(&self) -> f32 {
        self.sandbox.max_wind_mps()
    }

    pub fn max_jet_mps(&self) -> f32 {
        self.sandbox.max_jet_mps()
    }

    pub fn max_precip_mm_hr(&self) -> f32 {
        self.sandbox.max_precip_mm_hr()
    }

    pub fn max_storm_surge_m(&self) -> f32 {
        self.sandbox.max_storm_surge_m()
    }

    pub fn inundated_land_tiles(&self) -> u32 {
        self.sandbox.solver.tides.inundated_land_count()
    }

    pub fn tile_colors_rgb(&self, mode: &str, selected_tile: i32) -> Vec<f32> {
        self.sandbox.tile_colors_rgb(mode, selected_tile)
    }

    pub fn weather_arrow_positions(&self, arrow_mode: &str) -> Vec<f32> {
        let samples = self.sandbox.sampled_vector_arrows(arrow_mode);
        if samples.is_empty() {
            return Vec::new();
        }
        let lift = if arrow_mode == "jet_stream" {
            1.0055
        } else {
            1.003
        };
        build_fine_arrow_segments(&self.sandbox.base.fine.grid, &samples, lift)
    }

    pub fn inspect_tile_structured_json(&mut self, fine_idx: usize) -> String {
        match self.sandbox.inspect_tile_structured(fine_idx) {
            Some(view) => view.to_json(),
            None => "{\"hasTile\":false}".to_string(),
        }
    }

    pub fn tile_meteogram_json(&self, fine_idx: usize) -> String {
        self.sandbox.tile_meteogram_json(fine_idx)
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        build_mesh_positions(&self.sandbox.base.fine.grid)
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        build_mesh_normals(&self.sandbox.base.fine.grid)
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        build_vertex_tile_ids(&self.sandbox.base.fine.grid)
    }
}

impl Default for WasmWeatherSession {
    fn default() -> Self {
        Self::new()
    }
}
