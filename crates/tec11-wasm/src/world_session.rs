use tec11_geology::{WorldCreateParams, WorldSimulation, WorldStageState};
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmWorldSession {
    sim: WorldSimulation,
}

#[wasm_bindgen]
impl WasmWorldSession {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        grid_size: u32,
        day_hours: u32,
        tilt_deg: u32,
        land_pct: u32,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        people_dt_myr: u32,
        moon_count: u32,
        moon_mass: f32,
        moon_dist_radii: u32,
        meteor_freq: f32,
    ) -> Self {
        let params = WorldCreateParams {
            grid_size,
            day_hours,
            tilt_deg,
            land_pct,
            atm_dt_myr,
            life_dt_myr,
            people_dt_myr,
            moon_count,
            moon_mass,
            moon_dist_radii,
            meteor_freq,
        };
        Self {
            sim: WorldSimulation::new(params, 42),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_world(
        &mut self,
        grid_size: u32,
        day_hours: u32,
        tilt_deg: u32,
        land_pct: u32,
        atm_dt_myr: u32,
        life_dt_myr: u32,
        people_dt_myr: u32,
        moon_count: u32,
        moon_mass: f32,
        moon_dist_radii: u32,
        meteor_freq: f32,
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
            moon_count,
            moon_mass,
            moon_dist_radii,
            meteor_freq,
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

    pub fn param_moon_count(&self) -> u32 {
        self.sim.params().moon_count
    }

    pub fn param_moon_mass(&self) -> f32 {
        self.sim.params().moon_mass
    }

    pub fn param_moon_dist_radii(&self) -> u32 {
        self.sim.params().moon_dist_radii
    }

    pub fn param_meteor_freq(&self) -> f32 {
        self.sim.params().meteor_freq
    }

    pub fn current_day_hours(&self) -> f32 {
        self.sim.current_day_hours()
    }

    pub fn current_tilt_deg(&self) -> f32 {
        self.sim.current_tilt_deg()
    }

    pub fn current_moon_dist_radii(&self) -> f32 {
        self.sim.current_moon_dist_radii()
    }

    pub fn current_tidal_strength(&self) -> f32 {
        self.sim.current_tidal_strength()
    }

    pub fn impact_count(&self) -> u32 {
        self.sim.impact_count()
    }

    pub fn set_sun_and_atmosphere(&mut self, star_filter: u32, atmosphere_id: u32) {
        self.sim.set_sun_and_atmosphere(star_filter, atmosphere_id);
    }

    pub fn star_filter(&self) -> u32 {
        self.sim.star_filter()
    }

    pub fn sun_spectral_type(&self) -> String {
        self.sim.home_star().spectral_type.clone()
    }

    pub fn sun_temp_k(&self) -> f32 {
        self.sim.home_star().temp_k
    }

    pub fn sun_rgb(&self) -> Vec<f32> {
        self.sim.home_star().rgb.to_vec()
    }

    pub fn atmosphere_id(&self) -> u32 {
        self.sim.atmosphere().id()
    }

    pub fn atmosphere_name(&self) -> String {
        self.sim.atmosphere().name().to_string()
    }

    pub fn sky_zenith_rgb(&self) -> Vec<f32> {
        self.sim.optics().sky_zenith_rgb.to_vec()
    }

    pub fn sky_color_name(&self) -> String {
        self.sim.optics().sky_color_name.clone()
    }

    pub fn sunset_rgb(&self) -> Vec<f32> {
        self.sim.optics().sunset_rgb.to_vec()
    }

    pub fn sunset_color_name(&self) -> String {
        self.sim.optics().sunset_color_name.clone()
    }

    pub fn plant_foliage_rgb(&self) -> Vec<f32> {
        self.sim.optics().plant_foliage_rgb.to_vec()
    }

    pub fn plant_color_name(&self) -> String {
        self.sim.optics().plant_color_name.clone()
    }

    pub fn planet_count(&self) -> usize {
        self.sim.solar_system().len()
    }

    pub fn home_planet_index(&self) -> usize {
        self.sim.home_planet_index()
    }

    pub fn planets_summary_text(&self) -> String {
        self.sim.planets_summary_text()
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn tile_colors_rgb(&self, aspect: &str, selected_tile: i32) -> Vec<f32> {
        self.sim.tile_colors_rgb(
            aspect,
            (selected_tile >= 0).then_some(selected_tile as usize),
        )
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        match self.sim.stage() {
            WorldStageState::History(hist) => hist.build_display_mesh().positions,
            _ => build_mesh_positions(self.sim.grid()),
        }
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        match self.sim.stage() {
            WorldStageState::History(hist) => hist.build_display_mesh().normals,
            _ => build_mesh_normals(self.sim.grid()),
        }
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        match self.sim.stage() {
            WorldStageState::History(hist) => hist.build_display_mesh().vertex_face_ids,
            _ => build_vertex_tile_ids(self.sim.grid()),
        }
    }

    pub fn travel_route_segments_colored(&self) -> Vec<f32> {
        match self.sim.stage() {
            WorldStageState::History(hist) => hist.travel_route_segments_colored(false),
            _ => Vec::new(),
        }
    }
}
