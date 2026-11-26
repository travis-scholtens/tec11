use tec11_tides_sandbox::TidesSandbox;
use wasm_bindgen::prelude::*;

use crate::mesh::{build_mesh_normals, build_mesh_positions, build_vertex_tile_ids, rotate_axes};

/// Build Three.js line-segment positions (`[x0, y0, z0, x1, y1, z1, ...]`, 3 segments per arrow)
/// from sampled `(fine_idx, velocity_xyz, norm_speed)` tuples on `fine_grid`.
pub(crate) fn build_fine_arrow_segments(
    fine_grid: &tec11_math_grid::SphericalGrid,
    samples: &[(usize, [f32; 3], f32)],
    radius_lift: f64,
) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples.len() * 18);
    for &(fine_idx, vel, norm_spd) in samples {
        let vmag = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
        if vmag < 1e-6 {
            continue;
        }
        let dx = (vel[0] / vmag) as f64;
        let dy = (vel[1] / vmag) as f64;
        let dz = (vel[2] / vmag) as f64;

        let Some(&(p_raw, _)) = fine_grid.faces.get(fine_idx) else {
            continue;
        };
        let plen = (p_raw[0] * p_raw[0] + p_raw[1] * p_raw[1] + p_raw[2] * p_raw[2])
            .sqrt()
            .max(1e-12);
        let nx = p_raw[0] / plen;
        let ny = p_raw[1] / plen;
        let nz = p_raw[2] / plen;

        let px = ny * dz - nz * dy;
        let py = nz * dx - nx * dz;
        let pz = nx * dy - ny * dx;

        let cx = nx * radius_lift;
        let cy = ny * radius_lift;
        let cz = nz * radius_lift;

        let half_len = 0.0065 + 0.0135 * (norm_spd as f64).sqrt();
        let tail = [cx - half_len * dx, cy - half_len * dy, cz - half_len * dz];
        let tip = [cx + half_len * dx, cy + half_len * dy, cz + half_len * dz];
        let barb_back = 0.65 * half_len;
        let barb_wing = 0.42 * half_len;
        let left = [
            tip[0] - barb_back * dx + barb_wing * px,
            tip[1] - barb_back * dy + barb_wing * py,
            tip[2] - barb_back * dz + barb_wing * pz,
        ];
        let right = [
            tip[0] - barb_back * dx - barb_wing * px,
            tip[1] - barb_back * dy - barb_wing * py,
            tip[2] - barb_back * dz - barb_wing * pz,
        ];

        for (a, b) in [(tail, tip), (tip, left), (tip, right)] {
            let ra = rotate_axes(a);
            let rb = rotate_axes(b);
            out.extend_from_slice(&[ra[0], ra[1], ra[2], rb[0], rb[1], rb[2]]);
        }
    }
    out
}

#[wasm_bindgen]
pub struct WasmTidesSession {
    sandbox: TidesSandbox,
}

#[wasm_bindgen]
impl WasmTidesSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sandbox: TidesSandbox::new_earth(),
        }
    }

    pub fn from_snapshot_or_earth(bytes: &[u8]) -> Self {
        let sandbox = if bytes.is_empty() {
            TidesSandbox::new_earth()
        } else {
            TidesSandbox::from_snapshot_bytes(bytes).unwrap_or_else(TidesSandbox::new_earth)
        };
        Self { sandbox }
    }

    pub fn load_earth(&mut self) {
        self.sandbox.load_earth();
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        self.sandbox.load_snapshot_bytes(bytes)
    }

    pub fn reset_tides(&mut self) {
        self.sandbox.reset_tides();
    }

    pub fn step_hours(&mut self, hours: u32) {
        self.sandbox.step_hours(hours);
    }

    pub fn set_day_of_year(&mut self, day: u32) {
        self.sandbox.set_day_of_year(day);
    }

    pub fn set_moon_params(&mut self, count: u32, primary_mass: f32, primary_dist_radii: f32) {
        self.sandbox
            .set_moon_params(count, primary_mass, primary_dist_radii);
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
        self.sandbox.solver.current_sim_calendar().1
    }

    pub fn elapsed_hours(&self) -> u32 {
        self.sandbox.solver.elapsed_hours() as u32
    }

    pub fn sim_clock_label(&self) -> String {
        self.sandbox.sim_clock_label()
    }

    pub fn moon_count(&self) -> u32 {
        self.sandbox.solver.moon_count()
    }

    pub fn moon_mass(&self) -> f32 {
        self.sandbox.solver.primary_mass_lunar()
    }

    pub fn moon_dist_radii(&self) -> f32 {
        self.sandbox.solver.primary_dist_radii()
    }

    pub fn max_tide_m(&self) -> f32 {
        self.sandbox.max_tide_abs_m()
    }

    pub fn max_tidal_range_m(&self) -> f32 {
        self.sandbox.max_tidal_range_m()
    }

    pub fn max_current_mps(&self) -> f32 {
        self.sandbox.max_current_mps()
    }

    pub fn inundated_land_tiles(&self) -> u32 {
        self.sandbox.solver.inundated_land_count()
    }

    pub fn tile_colors_rgb(&self, mode: &str, selected_tile: i32) -> Vec<f32> {
        self.sandbox.tile_colors_rgb(mode, selected_tile)
    }

    pub fn tidal_arrow_positions(&self) -> Vec<f32> {
        let samples = self.sandbox.sampled_tidal_arrows();
        build_fine_arrow_segments(&self.sandbox.base.fine.grid, &samples, 1.003)
    }

    pub fn inspect_tile_structured_json(&mut self, fine_idx: usize) -> String {
        match self.sandbox.inspect_tile_structured(fine_idx) {
            Some(view) => view.to_json(),
            None => "{\"hasTile\":false}".to_string(),
        }
    }

    pub fn tile_tide_series_json(&self, fine_idx: usize) -> String {
        self.sandbox.tile_tide_series_json(fine_idx)
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

impl Default for WasmTidesSession {
    fn default() -> Self {
        Self::new()
    }
}
