use tec11_geology::HistorySimulation;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmHistorySession {
    sim: HistorySimulation,
    cached_mesh: Option<(Vec<f32>, Vec<f32>, Vec<u32>)>,
}

#[wasm_bindgen]
impl WasmHistorySession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: HistorySimulation::new_earth(42),
            cached_mesh: None,
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
        self.cached_mesh = None;
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let ok = self.sim.load_snapshot_bytes(bytes);
        if ok {
            self.cached_mesh = None;
        }
        ok
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.sim.encode_snapshot()
    }

    pub fn reset(&mut self) {
        self.sim.reset();
        self.cached_mesh = None;
    }

    pub fn step(&mut self) {
        self.sim.step();
    }

    pub fn tile_count(&self) -> usize {
        self.sim.tile_count()
    }

    pub fn history_ticks(&self) -> u32 {
        self.sim.history_ticks()
    }

    pub fn years(&self) -> u32 {
        self.sim.years()
    }

    pub fn is_custom_planet(&self) -> bool {
        self.sim.is_custom_planet()
    }

    pub fn land_percent(&self) -> u32 {
        self.sim.land_percent()
    }

    pub fn active_state_count(&self) -> usize {
        self.sim.active_state_count()
    }

    pub fn total_population_thousands(&self) -> f32 {
        self.sim.total_population_thousands()
    }

    pub fn active_travel_route_count(&self) -> usize {
        self.sim.active_travel_route_count()
    }

    pub fn potential_travel_route_count(&self) -> usize {
        self.sim.potential_travel_route_count()
    }

    pub fn water_access_port_count(&self) -> usize {
        self.sim.water_access_port_count()
    }

    pub fn maritime_trade_route_count(&self) -> usize {
        self.sim.maritime_trade_route_count()
    }

    pub fn maritime_conflict_route_count(&self) -> usize {
        self.sim.maritime_conflict_route_count()
    }

    pub fn travel_route_segments_colored(&self, include_potential: bool) -> Vec<f32> {
        self.sim.travel_route_segments_colored(include_potential)
    }

    pub fn wind_current_vector_segments(&self) -> Vec<f32> {
        self.sim.wind_current_vector_segments()
    }

    pub fn inspect_travel_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_travel_tile(tile_idx)
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        self.sim.inspect_tile(tile_idx)
    }

    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_rivers: bool,
        selected_tile: i32,
    ) -> Vec<f32> {
        let sel = if selected_tile >= 0 {
            Some(selected_tile as usize)
        } else {
            None
        };
        self.sim.tile_colors_rgb(aspect, show_rivers, sel)
    }

    fn ensure_mesh(&mut self) -> &(Vec<f32>, Vec<f32>, Vec<u32>) {
        if self.cached_mesh.is_none() {
            self.cached_mesh = Some(self.sim.build_display_mesh());
        }
        self.cached_mesh.as_ref().unwrap()
    }

    pub fn mesh_positions(&mut self) -> Vec<f32> {
        self.ensure_mesh().0.clone()
    }

    pub fn mesh_normals(&mut self) -> Vec<f32> {
        self.ensure_mesh().1.clone()
    }

    pub fn vertex_tile_ids(&mut self) -> Vec<u32> {
        self.ensure_mesh().2.clone()
    }
}
