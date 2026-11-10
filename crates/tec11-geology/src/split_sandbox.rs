use std::f64::consts::PI;

use tec11_math_grid::{normal, rotate, Pcg64Mcg, SphericalGrid, SphericalPolygon};

use crate::movement::split_group;

/// Standalone continent rifting / splitting sandbox (`splitpoints.py`).
pub struct SplitSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    red_tiles: Vec<usize>,
    blue_tiles: Vec<usize>,
    tile_states: Vec<u8>,
    iterations: u32,
    rng: Pcg64Mcg,
}

impl SplitSandbox {
    pub fn new(grid_size: u32, seed: u64) -> Self {
        let size = grid_size.clamp(4, 6);
        let grid = SphericalGrid::with_size(size);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let rng = Pcg64Mcg::from_seed_u64(seed);
        let mut s = Self {
            grid,
            adjacency,
            red_tiles: Vec::new(),
            blue_tiles: Vec::new(),
            tile_states: vec![0; n],
            iterations: 0,
            rng,
        };
        s.reset_continent();
        s
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.grid.faces.len()
    }

    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    pub fn red_count(&self) -> usize {
        self.red_tiles.len()
    }

    pub fn blue_count(&self) -> usize {
        self.blue_tiles.len()
    }

    /// Per-tile state:
    /// - `0`: Ocean / empty
    /// - `1`: Red half (`'R'`)
    /// - `2`: Blue half (`'B'`)
    pub fn tile_states(&self) -> &[u8] {
        &self.tile_states
    }

    pub fn reset(&mut self, grid_size: u32) {
        let size = grid_size.clamp(4, 6);
        if self.grid.size != size {
            self.grid = SphericalGrid::with_size(size);
            self.adjacency = self.grid.adjacency();
        }
        self.reset_continent();
    }

    fn reset_continent(&mut self) {
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

        let initial_tiles: Vec<usize> = self
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

        self.iterations = 0;
        self.apply_split(&initial_tiles);
    }

    /// Iterate by splitting either the Red half (`Some(true)`), the Blue half (`Some(false)`),
    /// or a randomly chosen half (`None`, matching `splitpresenter.py:39`).
    pub fn iterate(&mut self, choose_red: Option<bool>) {
        let use_red = choose_red.unwrap_or_else(|| self.rng.next_f64() < 0.5);
        let source_tiles = if use_red {
            if !self.red_tiles.is_empty() {
                self.red_tiles.clone()
            } else {
                self.blue_tiles.clone()
            }
        } else if !self.blue_tiles.is_empty() {
            self.blue_tiles.clone()
        } else {
            self.red_tiles.clone()
        };

        if source_tiles.len() >= 2 {
            self.apply_split(&source_tiles);
        }
    }

    fn apply_split(&mut self, source_tiles: &[usize]) {
        let mut split_done = false;
        for _attempt in 0..8 {
            if let Some(((a, _), (b, _))) =
                split_group(&self.grid, &self.adjacency, source_tiles, &mut self.rng)
            {
                self.red_tiles = a;
                self.blue_tiles = b;
                split_done = true;
                break;
            }
        }
        if !split_done {
            let mid = source_tiles.len() / 2;
            self.red_tiles = source_tiles[..mid].to_vec();
            self.blue_tiles = source_tiles[mid..].to_vec();
        }
        self.iterations += 1;
        self.rebuild_tile_states();
    }

    fn rebuild_tile_states(&mut self) {
        self.tile_states.resize(self.grid.faces.len(), 0);
        self.tile_states.fill(0);
        for &t in &self.red_tiles {
            self.tile_states[t] = 1;
        }
        for &t in &self.blue_tiles {
            self.tile_states[t] = 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_sandbox_lifecycle() {
        let mut s = SplitSandbox::new(5, 42);
        let initial_total = s.red_count() + s.blue_count();
        assert!(initial_total > 100);
        assert!(s.red_count() > 0);
        assert!(s.blue_count() > 0);
        assert_eq!(s.iterations(), 1);

        let red_before = s.red_count();
        s.iterate(Some(true));
        assert_eq!(s.iterations(), 2);
        assert_eq!(s.red_count() + s.blue_count(), red_before);

        let blue_before = s.blue_count();
        s.iterate(Some(false));
        assert_eq!(s.iterations(), 3);
        assert_eq!(s.red_count() + s.blue_count(), blue_before);
    }
}
