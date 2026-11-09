use tec11_math_grid::{normal, SphericalGrid, Vec3};
use wasm_bindgen::prelude::*;

#[inline]
fn rotate_axes(v: Vec3) -> [f32; 3] {
    [-v[1] as f32, v[2] as f32, v[0] as f32]
}

#[wasm_bindgen]
pub struct WasmGridSession {
    grid: SphericalGrid,
}

#[wasm_bindgen]
impl WasmGridSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            grid: SphericalGrid::new(),
        }
    }

    /// Subdivides the current spherical grid to the next finer level (size -> size + 1).
    pub fn subdivide(&mut self) {
        self.grid = self.grid.subdivide();
    }

    /// Current subdivision level (0 = regular dodecahedron).
    pub fn size(&self) -> u32 {
        self.grid.size
    }

    /// Number of tiles (faces) in the current grid.
    pub fn tile_count(&self) -> u32 {
        self.grid.faces.len() as u32
    }

    /// Per-tile unit-sphere center coordinates `[x0, y0, z0, x1, y1, z1, ...]` in [-1, 1].
    /// The frontend uses these raw tile positions to compute per-tile colors (e.g. r/g/b = x/y/z).
    pub fn tile_centers(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.grid.faces.len() * 3);
        for (center, _) in &self.grid.faces {
            out.push(center[0] as f32);
            out.push(center[1] as f32);
            out.push(center[2] as f32);
        }
        out
    }

    /// Triangulated fan vertex positions `[x, y, z, ...]` in Three.js view coordinates
    /// (`rotate_axes(x, y, z) = (-y, z, x)` matching `sphereview.py`).
    pub fn mesh_positions(&self) -> Vec<f32> {
        let total_tris: usize = self.grid.faces.iter().map(|(_, vs)| vs.len()).sum();
        let mut positions = Vec::with_capacity(total_tris * 9);

        for (center, vs) in &self.grid.faces {
            let t = rotate_axes(*center);
            let len = vs.len();
            for i in 0..len {
                let v_prev = rotate_axes(vs[(i + len - 1) % len]);
                let v_curr = rotate_axes(vs[i]);
                positions.extend_from_slice(&t);
                positions.extend_from_slice(&v_prev);
                positions.extend_from_slice(&v_curr);
            }
        }
        positions
    }

    /// Per-vertex normal vectors `[nx, ny, nz, ...]` matching `mesh_positions`.
    pub fn mesh_normals(&self) -> Vec<f32> {
        let total_tris: usize = self.grid.faces.iter().map(|(_, vs)| vs.len()).sum();
        let mut normals = Vec::with_capacity(total_tris * 9);

        for (center, vs) in &self.grid.faces {
            let n = rotate_axes(normal(*center));
            for _ in 0..vs.len() {
                normals.extend_from_slice(&n);
                normals.extend_from_slice(&n);
                normals.extend_from_slice(&n);
            }
        }
        normals
    }

    /// Maps each vertex in `mesh_positions` to its logical `TileId` (`0..tile_count`).
    pub fn vertex_tile_ids(&self) -> Vec<u32> {
        let total_tris: usize = self.grid.faces.iter().map(|(_, vs)| vs.len()).sum();
        let mut ids = Vec::with_capacity(total_tris * 3);

        for (tile_idx, (_, vs)) in self.grid.faces.iter().enumerate() {
            let tid = tile_idx as u32;
            for _ in 0..(vs.len() * 3) {
                ids.push(tid);
            }
        }
        ids
    }
}

impl Default for WasmGridSession {
    fn default() -> Self {
        Self::new()
    }
}
