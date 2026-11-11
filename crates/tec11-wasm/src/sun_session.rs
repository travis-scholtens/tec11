use tec11_geology::SunSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids};

#[wasm_bindgen]
pub struct WasmSunSession {
    sandbox: SunSandbox,
}

#[wasm_bindgen]
impl WasmSunSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sandbox: SunSandbox::new(42),
        }
    }

    pub fn generate_galaxy_and_sun(
        &mut self,
        galaxy_type_id: u32,
        vantage_id: u32,
        star_filter: u32,
    ) {
        self.sandbox
            .generate_galaxy_and_sun(galaxy_type_id, vantage_id, star_filter);
    }

    pub fn choose_sun_in_current_galaxy(&mut self, star_filter: u32) {
        self.sandbox.choose_sun_in_current_galaxy(star_filter);
    }

    pub fn load_real_hipparcos_sol(&mut self) {
        self.sandbox.load_real_hipparcos_sol();
    }

    pub fn set_atmosphere(&mut self, atmosphere_id: u32) {
        self.sandbox.set_atmosphere(atmosphere_id);
    }

    pub fn atmosphere_id(&self) -> u32 {
        self.sandbox.atmosphere().id()
    }

    pub fn atmosphere_name(&self) -> String {
        self.sandbox.atmosphere().name().to_string()
    }

    pub fn atmosphere_formula(&self) -> String {
        self.sandbox.atmosphere().short_formula().to_string()
    }

    pub fn sky_zenith_rgb(&self) -> Vec<f32> {
        self.sandbox.optics().sky_zenith_rgb.to_vec()
    }

    pub fn sky_horizon_rgb(&self) -> Vec<f32> {
        self.sandbox.optics().sky_horizon_rgb.to_vec()
    }

    pub fn sky_color_name(&self) -> String {
        self.sandbox.optics().sky_color_name.clone()
    }

    pub fn sky_mechanism(&self) -> String {
        self.sandbox.optics().sky_mechanism.clone()
    }

    pub fn sunset_rgb(&self) -> Vec<f32> {
        self.sandbox.optics().sunset_rgb.to_vec()
    }

    pub fn twilight_rgb(&self) -> Vec<f32> {
        self.sandbox.optics().twilight_rgb.to_vec()
    }

    pub fn sunset_color_name(&self) -> String {
        self.sandbox.optics().sunset_color_name.clone()
    }

    pub fn sunset_mechanism(&self) -> String {
        self.sandbox.optics().sunset_mechanism.clone()
    }

    pub fn plant_foliage_rgb(&self) -> Vec<f32> {
        self.sandbox.optics().plant_foliage_rgb.to_vec()
    }

    pub fn plant_highlight_rgb(&self) -> Vec<f32> {
        self.sandbox.optics().plant_highlight_rgb.to_vec()
    }

    pub fn plant_color_name(&self) -> String {
        self.sandbox.optics().plant_color_name.clone()
    }

    pub fn plant_pigment_desc(&self) -> String {
        self.sandbox.optics().plant_pigment_desc.clone()
    }

    pub fn tile_count(&self) -> usize {
        self.sandbox.tile_count()
    }

    pub fn star_count(&self) -> usize {
        self.sandbox.star_count()
    }

    pub fn galaxy_type_id(&self) -> u32 {
        self.sandbox.galaxy_type().id()
    }

    pub fn galaxy_type_name(&self) -> String {
        self.sandbox.galaxy_type().name().to_string()
    }

    pub fn vantage_id(&self) -> u32 {
        self.sandbox.vantage().id()
    }

    pub fn vantage_name(&self) -> String {
        self.sandbox.vantage().name().to_string()
    }

    pub fn sun_spectral_type(&self) -> String {
        self.sandbox.home_star().spectral_type.clone()
    }

    pub fn sun_class_description(&self) -> String {
        self.sandbox.home_star().class_description.clone()
    }

    pub fn sun_bv_color(&self) -> f32 {
        self.sandbox.home_star().bv_color
    }

    pub fn sun_rgb(&self) -> Vec<f32> {
        self.sandbox.home_star().rgb.to_vec()
    }

    pub fn sun_temp_k(&self) -> f32 {
        self.sandbox.home_star().temp_k
    }

    pub fn sun_mass_solar(&self) -> f32 {
        self.sandbox.home_star().mass_solar
    }

    pub fn sun_radius_solar(&self) -> f32 {
        self.sandbox.home_star().radius_solar
    }

    pub fn sun_luminosity_solar(&self) -> f32 {
        self.sandbox.home_star().luminosity_solar
    }

    pub fn sun_habitable_au(&self) -> f32 {
        self.sandbox.home_star().habitable_au
    }

    pub fn sun_orbital_period_days(&self) -> f32 {
        self.sandbox.home_star().orbital_period_days
    }

    pub fn sun_angular_diameter_deg(&self) -> f32 {
        self.sandbox.home_star().angular_diameter_deg
    }

    pub fn sun_solar_tide_factor(&self) -> f32 {
        self.sandbox.home_star().solar_tide_factor
    }

    pub fn planet_count(&self) -> usize {
        self.sandbox.planet_count()
    }

    pub fn home_planet_index(&self) -> usize {
        self.sandbox.home_planet_index()
    }

    pub fn planets_summary_text(&self) -> String {
        self.sandbox.planets_summary_text()
    }

    pub fn summary_line(&self) -> String {
        self.sandbox.summary_line()
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sandbox.inspect_tile(tile_idx)
    }

    pub fn star_points_data(&self, aspect: &str) -> Vec<f32> {
        self.sandbox.star_points_data(aspect)
    }

    pub fn overlay_line_segments(
        &self,
        aspect: &str,
        show_galactic_plane: bool,
        show_constellations: bool,
    ) -> Vec<f32> {
        self.sandbox
            .overlay_line_segments(aspect, show_galactic_plane, show_constellations)
    }

    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_galactic_plane: bool,
        selected_tile: i32,
    ) -> Vec<f32> {
        let sel = if selected_tile >= 0 {
            Some(selected_tile as usize)
        } else {
            None
        };
        self.sandbox
            .tile_colors_rgb(aspect, show_galactic_plane, sel)
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        build_mesh_positions(self.sandbox.grid())
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        build_mesh_normals(self.sandbox.grid())
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        build_vertex_tile_ids(self.sandbox.grid())
    }
}

impl Default for WasmSunSession {
    fn default() -> Self {
        Self::new()
    }
}
