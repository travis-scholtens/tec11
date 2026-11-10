use std::f64::consts::PI;

use tec11_math_grid::{
    cross, length, normal, rotate, Pcg64Mcg, PointTree, SphericalGrid, SphericalPolygon, Vec3,
};

use crate::movement::{average_vec3, move_group};

/// Standalone plate movement / spherical advection sandbox (`movepoints.py`).
pub struct MoveSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    tree: PointTree,
    group_tiles: Vec<usize>,
    trail_tiles: Vec<usize>,
    tile_states: Vec<u8>,
    direction_deg: f64,
    speed_deg: f64,
    velocity: Vec3,
    steps: u32,
    rng: Pcg64Mcg,
}

impl MoveSandbox {
    pub fn new(grid_size: u32, direction_deg: f64, speed_deg: f64, seed: u64) -> Self {
        let size = grid_size.clamp(4, 6);
        let grid = SphericalGrid::with_size(size);
        let adjacency = grid.adjacency();
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let n = grid.faces.len();
        let rng = Pcg64Mcg::from_seed_u64(seed);
        let mut s = Self {
            grid,
            adjacency,
            tree,
            group_tiles: Vec::new(),
            trail_tiles: Vec::new(),
            tile_states: vec![0; n],
            direction_deg,
            speed_deg: speed_deg.clamp(0.5, 25.0),
            velocity: [0.0, 0.0, 0.0],
            steps: 0,
            rng,
        };
        s.reset_continent(direction_deg, speed_deg);
        s
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.grid.faces.len()
    }

    pub fn group_tile_count(&self) -> usize {
        self.group_tiles.len()
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn direction_deg(&self) -> f64 {
        self.direction_deg
    }

    pub fn speed_deg(&self) -> f64 {
        self.speed_deg
    }

    /// Per-tile state:
    /// - `0`: Ocean / empty
    /// - `1`: Current continent tile (`'T'`)
    /// - `2`: Previous step's trail tile (`'M'`)
    pub fn tile_states(&self) -> &[u8] {
        &self.tile_states
    }

    pub fn reset(&mut self, grid_size: u32, direction_deg: f64, speed_deg: f64) {
        let size = grid_size.clamp(4, 6);
        if self.grid.size != size {
            self.grid = SphericalGrid::with_size(size);
            self.adjacency = self.grid.adjacency();
            let indexed_pts: Vec<(Vec3, usize)> = self
                .grid
                .faces
                .iter()
                .enumerate()
                .map(|(i, (c, _))| (*c, i))
                .collect();
            self.tree = PointTree::new(&indexed_pts);
        }
        self.reset_continent(direction_deg, speed_deg);
    }

    fn reset_continent(&mut self, direction_deg: f64, speed_deg: f64) {
        let raw_p = [
            self.rng.uniform(-1.0, 1.0),
            self.rng.uniform(-1.0, 1.0),
            self.rng.uniform(-1.0, 1.0),
        ];
        let p = normal(raw_p);

        let mut mini = 0;
        if p[1].abs() < p[mini].abs() {
            mini = 1;
        }
        if p[2].abs() < p[mini].abs() {
            mini = 2;
        }
        let mut o = [0.0, 0.0, 0.0];
        o[mini] = if p[mini] < 0.0 { 1.0 } else { -1.0 };

        let spd = speed_deg.clamp(0.5, 25.0);
        let spd_rad = spd * PI / 180.0;
        let base_v = normal(cross(p, o));
        let rotated_v = rotate(base_v, p, direction_deg * PI / 180.0);
        self.velocity = [
            rotated_v[0] * spd_rad,
            rotated_v[1] * spd_rad,
            rotated_v[2] * spd_rad,
        ];
        self.direction_deg = direction_deg;
        self.speed_deg = spd;

        let r = 1.145;
        let mut poly_verts = Vec::with_capacity(16);
        for i in 0..16 {
            let th = (i as f64) * PI / 8.0;
            let jitter = self.rng.uniform(0.9, 1.1);
            let v1 = rotate(p, o, r * jitter);
            let v2 = rotate(v1, p, th);
            poly_verts.push(v2);
        }
        let shape = SphericalPolygon::new(&poly_verts);

        self.group_tiles = self
            .grid
            .faces
            .iter()
            .enumerate()
            .filter_map(|(idx, (center, _))| {
                if shape.contains(*center) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        self.trail_tiles.clear();
        self.steps = 0;
        self.rebuild_tile_states();
    }

    pub fn step(&mut self) {
        if self.group_tiles.is_empty() {
            return;
        }
        self.trail_tiles = self.group_tiles.clone();
        let (dest_map, next_v) = move_group(
            &self.grid,
            &self.adjacency,
            &self.tree,
            &self.group_tiles,
            self.velocity,
        );
        self.group_tiles = dest_map.keys().copied().collect();
        self.velocity = next_v;
        self.steps += 1;
        self.rebuild_tile_states();
    }

    /// Rotate the group's velocity vector around the group centroid (`movepoints.py:99-103`).
    pub fn set_direction_deg(&mut self, value_deg: f64) {
        if !self.group_tiles.is_empty() {
            let vs: Vec<Vec3> = self
                .group_tiles
                .iter()
                .map(|&t| self.grid.faces[t].0)
                .collect();
            let avg = average_vec3(&vs);
            if length(avg) > 1e-12 {
                let a = normal(avg);
                let delta_rad = (value_deg - self.direction_deg) * PI / 180.0;
                self.velocity = rotate(self.velocity, a, delta_rad);
            }
        }
        self.direction_deg = value_deg;
    }

    /// Update the group's speed in degrees per step (`movepoints.py:105-107`).
    pub fn set_speed_deg(&mut self, value_deg: f64) {
        let spd = value_deg.clamp(0.5, 25.0);
        let spd_rad = spd * PI / 180.0;
        let len = length(self.velocity);
        if len > 1e-12 {
            let scale = spd_rad / len;
            self.velocity = [
                self.velocity[0] * scale,
                self.velocity[1] * scale,
                self.velocity[2] * scale,
            ];
        }
        self.speed_deg = spd;
    }

    fn rebuild_tile_states(&mut self) {
        self.tile_states.resize(self.grid.faces.len(), 0);
        self.tile_states.fill(0);
        for &t in &self.trail_tiles {
            self.tile_states[t] = 2;
        }
        for &t in &self.group_tiles {
            self.tile_states[t] = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_sandbox_conserves_tile_count() {
        let mut m = MoveSandbox::new(5, 0.0, 5.0, 42);
        let initial_count = m.group_tile_count();
        assert!(initial_count > 100);
        for _ in 0..10 {
            m.step();
            assert_eq!(m.group_tile_count(), initial_count);
        }
        m.set_direction_deg(90.0);
        m.set_speed_deg(8.0);
        m.step();
        assert_eq!(m.group_tile_count(), initial_count);
        assert_eq!(m.steps(), 11);
    }
}
