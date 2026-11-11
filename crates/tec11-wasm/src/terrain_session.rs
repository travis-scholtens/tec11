use tec11_geology::TerrainSandbox;
use tec11_math_grid::DisplayMesh;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmTerrainSession {
    sim: TerrainSandbox,
    cached_mesh: Option<DisplayMesh>,
}

#[wasm_bindgen]
impl WasmTerrainSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: TerrainSandbox::new_earth(),
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

    fn ensure_mesh(&mut self) -> &DisplayMesh {
        self.cached_mesh
            .get_or_insert_with(|| self.sim.build_display_mesh())
    }

    pub fn mesh_positions(&mut self) -> Vec<f32> {
        self.ensure_mesh().positions.clone()
    }

    pub fn mesh_normals(&mut self) -> Vec<f32> {
        self.ensure_mesh().normals.clone()
    }

    pub fn vertex_tile_ids(&mut self) -> Vec<u32> {
        self.ensure_mesh().vertex_face_ids.clone()
    }
}
