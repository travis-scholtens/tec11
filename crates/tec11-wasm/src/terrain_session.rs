use tec11_geology::TerrainSandbox;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmTerrainSession {
    sim: TerrainSandbox,
}

#[wasm_bindgen]
impl WasmTerrainSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: TerrainSandbox::new_earth(),
        }
    }

    pub fn load_earth(&mut self) {
        self.sim.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sim.load_snapshot_bytes(bytes)
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.sim.encode_snapshot()
    }

    pub fn is_custom_planet(&self) -> bool {
        self.sim.is_custom_planet()
    }

    pub fn display_face_count(&self) -> u32 {
        self.sim.display_face_count()
    }

    pub fn fine_face_count(&self) -> u32 {
        self.sim.fine_face_count()
    }

    pub fn coarse_tile_count(&self) -> u32 {
        self.sim.coarse_tile_count()
    }

    pub fn river_count(&self) -> u32 {
        self.sim.river_count()
    }

    pub fn land_percent(&self) -> u32 {
        self.sim.land_percent()
    }

    pub fn inspect_face(&self, display_idx: usize) -> String {
        self.sim.inspect_face(display_idx)
    }

    pub fn tile_colors_rgb(&self, show_rivers: bool, selected_face: i32) -> Vec<f32> {
        self.sim.tile_colors_rgb(show_rivers, selected_face)
    }

    pub fn mesh_positions(&self) -> Vec<f32> {
        self.sim.build_display_mesh().0
    }

    pub fn mesh_normals(&self) -> Vec<f32> {
        self.sim.build_display_mesh().1
    }

    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        self.sim.build_display_mesh().2
    }
}
