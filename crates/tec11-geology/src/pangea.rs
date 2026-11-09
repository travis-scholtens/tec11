use std::f64::consts::PI;

use tec11_math_grid::{
    great_circle_distance, normal, rotate, GreatCircleArc, Pcg64Mcg, PointTree, SphericalGrid,
    SphericalPolygon, Vec3,
};

use crate::rock::Rock;
use crate::tile::TileGeology;

/// Initial planet state generated with a single supercontinent (Pangaea) and surrounding ocean floor.
#[derive(Clone, Debug)]
pub struct PangeaWorld {
    pub grid: SphericalGrid,
    pub radius_km: f64,
    pub tiles: Vec<TileGeology>,
    pub continent_tiles: Vec<usize>,
}

impl PangeaWorld {
    /// Physical planet radius in km for a given `grid_size` (`worldpresenter.py:61-66`).
    /// Scales by ~sqrt(3) per subdivision so every tile is ~285 km across (~70,000 km^2).
    /// `grid_size = 6` (`6,400 km`) is Earth-sized.
    pub fn planet_radius_km(grid_size: u32) -> f64 {
        match grid_size {
            0..=4 => 2100.0,
            5 => 3700.0,
            6 => 6400.0,
            7 => 11000.0,
            _ => 19000.0,
        }
    }

    /// Generates a new planet with the given `grid_size` (4..8) and target `land_percent` (1..75).
    /// Matches `PlanetSimulation.create` in `planetsimulation.py:88-123`.
    pub fn generate(grid_size: u32, land_percent: f64, rng: &mut Pcg64Mcg) -> Self {
        let grid = SphericalGrid::with_size(grid_size);
        let radius_km = Self::planet_radius_km(grid_size);
        let num_tiles = grid.faces.len();

        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (center, _))| (*center, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);

        // Convert target land % to angular radius (matching spherical cap / worldpresenter.py)
        let frac = (land_percent / 100.0).clamp(0.01, 0.75);
        let land_r = (1.0 - 2.0 * frac).clamp(-0.98, 0.98).acos();

        // Random Pangaea center vector `p`
        let p = loop {
            let candidate = [
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
            ];
            if candidate[0] != 0.0 || candidate[1] != 0.0 || candidate[2] != 0.0 {
                break normal(candidate);
            }
        };

        // Orienting vector `o` along the coordinate axis where |p[i]| is smallest
        let mut o = [0.0, 0.0, 0.0];
        let min_axis = (0..3)
            .min_by(|&a, &b| p[a].abs().total_cmp(&p[b].abs()))
            .unwrap_or(0);
        o[min_axis] = if p[min_axis] < 0.0 { 1.0 } else { -1.0 };

        // 16-vertex jittered SphericalPolygon around `p`
        let poly_verts: Vec<Vec3> = (0..16)
            .map(|i| {
                let th = (i as f64) * PI / 8.0;
                let r_jitter = land_r * rng.uniform(0.9, 1.1);
                rotate(rotate(p, o, r_jitter), p, th)
            })
            .collect();
        let shape = SphericalPolygon::new(&poly_verts);

        let mut in_continent = vec![false; num_tiles];
        let mut continent_tiles = Vec::new();
        for (idx, (center, _)) in grid.faces.iter().enumerate() {
            if shape.contains(*center) {
                in_continent[idx] = true;
                continent_tiles.push(idx);
            }
        }

        // Ensure at least the nearest tile to `p` is in the continent
        let c_idx = tree.nearest(p);
        if continent_tiles.is_empty() {
            in_continent[c_idx] = true;
            continent_tiles.push(c_idx);
        }

        let c_vec = grid.faces[c_idx].0;
        let r2 = land_r * land_r;

        // One random continental tile is the center of a felsic (rhyolite) chunk
        let f_idx = continent_tiles[rng.gen_index(continent_tiles.len())];
        let f_vec = grid.faces[f_idx].0;

        // Generate a 3-branch triple-junction mid-ocean spreading ridge system in Panthalassa
        // centered near the antipode of Pangaea (`anti_p`).
        let anti_p = [-p[0], -p[1], -p[2]];
        let base_az = rng.uniform(0.0, 2.0 * PI);
        let tj_offset_dir = rotate(o, anti_p, base_az);
        let tj_center = rotate(anti_p, tj_offset_dir, rng.uniform(0.05, 0.30));

        let mut ridge_arcs = Vec::with_capacity(6);
        for arm in 0..3 {
            let arm_az = base_az + (arm as f64) * (2.0 * PI / 3.0) + rng.uniform(-0.22, 0.22);
            let arm_axis = rotate(o, anti_p, arm_az);
            let mid_pt = rotate(tj_center, arm_axis, rng.uniform(0.45, 0.65));
            let bend_axis = rotate(arm_axis, mid_pt, rng.uniform(-0.35, 0.35));
            let end_pt = rotate(mid_pt, bend_axis, rng.uniform(0.45, 0.65));
            ridge_arcs.push(GreatCircleArc::new(tj_center, mid_pt));
            ridge_arcs.push(GreatCircleArc::new(mid_pt, end_pt));
        }

        let mut tiles = Vec::with_capacity(num_tiles);
        let adj = grid.adjacency();
        for (idx, (t_vec, _)) in grid.faces.iter().enumerate() {
            if in_continent[idx] {
                let dc = great_circle_distance(*t_vec, c_vec);
                let df = great_circle_distance(*t_vec, f_vec);

                let silica = (1.0 - (df * df) / r2).max(0.5) as f32;
                let rock = Rock::extrusive(silica);
                let h = (1.0 - (dc * dc) / r2).max(0.05) as f32;

                tiles.push(TileGeology::empty_land(rock, h));
            } else {
                // At ~285 km per tile, a realistic passive continental shelf/slope (~80-285 km wide)
                // is 1 tile wide: only ocean tiles directly bordering a continental land tile.
                let is_coastal_shelf = adj[idx].iter().any(|&nb| in_continent[nb]);

                let ocean_elev = if is_coastal_shelf {
                    let d_coast_km = shape.distance_to_boundary(*t_vec) * radius_km;
                    let t_shelf = (d_coast_km / 285.0).clamp(0.0, 1.0);
                    (-0.20 - 1.20 * t_shelf) as f32
                } else {
                    let d_ridge_rad = ridge_arcs
                        .iter()
                        .map(|a| a.distance_to_point(*t_vec))
                        .fold(f64::INFINITY, f64::min);
                    let d_ridge_km = d_ridge_rad * radius_km;

                    // Parsons & Sclater thermal subsidence in physical km:
                    // -2.5 km at ridge axis (~1 tile wide crest), subsiding over ~2,600 km (~9 tiles,
                    // ~75 Myr at 35 km/Myr) to a flat -5.5 km abyssal plain beyond.
                    let u_ridge = (d_ridge_km / 2600.0).clamp(0.0, 1.0);
                    (-2.5 - 3.0 * u_ridge.sqrt()) as f32
                };

                tiles.push(TileGeology::empty_ocean_with_depth(
                    Rock::seafloor(),
                    ocean_elev,
                ));
            }
        }

        Self {
            grid,
            radius_km,
            tiles,
            continent_tiles,
        }
    }

    /// Rounded percentage of tiles that are land (`elevation > 0`).
    pub fn land_percent_actual(&self) -> u32 {
        let pct = 100.0 * (self.continent_tiles.len() as f64) / (self.tiles.len() as f64);
        (pct + 0.5) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pangea_generation() {
        let mut rng = Pcg64Mcg::from_seed_u64(42);
        let world = PangeaWorld::generate(5, 29.0, &mut rng);
        assert_eq!(world.tiles.len(), 2432);
        let actual_land = world.land_percent_actual();
        assert!(
            (20..=38).contains(&actual_land),
            "Expected ~29% land, got {}%",
            actual_land
        );
        let land_count = world.tiles.iter().filter(|t| t.elevation() > 0.0).count();
        let ocean_count = world.tiles.iter().filter(|t| t.elevation() <= 0.0).count();
        assert_eq!(land_count, world.continent_tiles.len());
        assert_eq!(land_count + ocean_count, 2432);

        let max_ocean_elev = world
            .tiles
            .iter()
            .filter(|t| t.elevation() <= 0.0)
            .map(|t| t.elevation())
            .fold(f32::NEG_INFINITY, f32::max);
        let min_ocean_elev = world
            .tiles
            .iter()
            .filter(|t| t.elevation() <= 0.0)
            .map(|t| t.elevation())
            .fold(f32::INFINITY, f32::min);
        assert!(
            max_ocean_elev > -1.5,
            "Expected shallow continental shelf / ridge, got max ocean elev {}",
            max_ocean_elev
        );
        assert!(
            min_ocean_elev < -4.5,
            "Expected deep abyssal plain, got min ocean elev {}",
            min_ocean_elev
        );
    }
}
