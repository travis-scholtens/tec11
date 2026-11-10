use std::collections::BTreeSet;

use crate::grid::SphericalGrid;
use crate::noise::snoise3;
use crate::vec3::{dist2, normal, Vec3, Vec3Key};

/// Parentage of a subdivided `level2` (`N + 2`) terrain face (`terrainmethod.py`).
#[derive(Clone, Debug)]
pub enum FineParentage {
    /// Fine face center is a coarse face center (`f in coarse.faces`).
    CoarseFace(usize),
    /// Fine face center is a coarse vertex (`f in coarse.vertices`) shared by 3 coarse tiles.
    CoarseVertex([usize; 3]),
    /// Fine face center is a level-1 vertex (`f in level1.vertices`) between 1 primary coarse face
    /// and 2 coarse vertices (each shared by 3 coarse tiles).
    CoarseEdge {
        primary_coarse: usize,
        flanking_coarse_vertices: [[usize; 3]; 2],
    },
}

/// Source of a polygon in the composite sphere mesh (`terrainsimulation.py:84-94`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayFaceSource {
    /// Subdivided level `N + 2` terrain face (`0..terrain_grid.faces.len()`).
    Fine(usize),
    /// Un-subdivided ocean face from level `N + 1` or level `N`, mapped to a coarse ocean tile index.
    OceanCoarse(usize),
}

/// 2-level land-subdivided terrain grid (`terrainmethod.py`, `terrainsimulation.py`).
#[derive(Clone, Debug)]
pub struct SubdividedTerrainGrid {
    pub coarse_size: u32,
    pub terrain_grid: SphericalGrid,
    pub terrain_adj: Vec<Vec<usize>>,
    pub parentage: Vec<FineParentage>,
    pub coarse_to_fine_children: Vec<Vec<usize>>,
    pub fine_elevations: Vec<f32>,
    pub display_faces: Vec<(Vec3, Vec<Vec3>, DisplayFaceSource)>,
}

impl SubdividedTerrainGrid {
    /// Subdivide all coarse land tiles (`coarse_elevations[c] > 0.0`) by 2 levels (`N -> N + 2`)
    /// and compute interpolated + 7-octave Simplex noise elevations (`terrainmethod.py:6-38`).
    pub fn from_coarse(
        coarse: &SphericalGrid,
        coarse_elevations: &[f32],
        coarse_mountainosities: &[f32],
    ) -> Self {
        let num_coarse = coarse.faces.len();

        // Level 1 (`N + 1`): populate every coarse land tile (`elevation > 0.0`).
        let mut level1 = SphericalGrid::new_empty(coarse.size + 1);
        for c_idx in 0..num_coarse {
            if coarse_elevations[c_idx] > 0.0 {
                level1.populate_from_parent(coarse, coarse.faces[c_idx].0);
            }
        }

        // Snapshot the level-1 faces generated from land tiles (`list(terrain.prev.faces.keys())`).
        let level1_land_faces: Vec<Vec3> = level1.faces.iter().map(|(f, _)| *f).collect();

        // Complete boundary vertices in `level1` (`grid.py:115-121`):
        // If any vertex of `p` in `level1` has fewer than 3 adjacent faces in `level1`,
        // `p` was a vertex in `coarse`, so populate all 3 coarse faces meeting at `p` into `level1`.
        for &p in &level1_land_faces {
            let p_key = Vec3Key::from_vec3(p);
            let p_idx = level1.face_index[&p_key];
            let needs_neighbors = level1.faces[p_idx]
                .1
                .iter()
                .any(|&v| level1.vertices[&Vec3Key::from_vec3(v)].len() < 3);
            if needs_neighbors {
                if let Some(coarse_neighbors) = coarse.vertices.get(&p_key) {
                    for &cn in coarse_neighbors {
                        level1.populate_from_parent(coarse, cn);
                    }
                }
            }
        }

        // Level 2 (`N + 2`): populate every face from `level1_land_faces`.
        let mut terrain_grid = SphericalGrid::new_empty(coarse.size + 2);
        for &p in &level1_land_faces {
            terrain_grid.populate_from_parent(&level1, p);
        }

        let terrain_adj = terrain_grid.adjacency();
        let num_fine = terrain_grid.faces.len();

        // Classify `FineParentage` and `coarse_to_fine_children` (`terrainmethod.py:19-28, 59-74`).
        let mut parentage = Vec::with_capacity(num_fine);
        let mut coarse_to_fine_children = vec![Vec::new(); num_coarse];

        let resolve_coarse_vertex = |vkey: &Vec3Key| -> [usize; 3] {
            let cfs = &coarse.vertices[vkey];
            [
                coarse.face_index[&Vec3Key::from_vec3(cfs[0])],
                coarse.face_index[&Vec3Key::from_vec3(cfs[1])],
                coarse.face_index[&Vec3Key::from_vec3(cfs[2])],
            ]
        };

        for (fine_idx, (f_vec, _)) in terrain_grid.faces.iter().enumerate() {
            let f_key = Vec3Key::from_vec3(*f_vec);
            if let Some(&c_idx) = coarse.face_index.get(&f_key) {
                parentage.push(FineParentage::CoarseFace(c_idx));
                coarse_to_fine_children[c_idx].push(fine_idx);
            } else if coarse.vertices.contains_key(&f_key) {
                let c_triple = resolve_coarse_vertex(&f_key);
                for &c_idx in &c_triple {
                    coarse_to_fine_children[c_idx].push(fine_idx);
                }
                parentage.push(FineParentage::CoarseVertex(c_triple));
            } else {
                // Edge face: vertex of `level1` between 3 `level1` faces,
                // exactly 1 of which is in `coarse.faces` and 2 of which are in `coarse.vertices`.
                let l1_faces = &level1.vertices[&f_key];
                let mut primary_coarse = 0usize;
                let mut flanking = Vec::with_capacity(2);
                for &pf in l1_faces {
                    let pf_key = Vec3Key::from_vec3(pf);
                    if let Some(&c_idx) = coarse.face_index.get(&pf_key) {
                        primary_coarse = c_idx;
                    } else if coarse.vertices.contains_key(&pf_key) {
                        flanking.push(resolve_coarse_vertex(&pf_key));
                    }
                }
                let flanking_arr = if flanking.len() >= 2 {
                    [flanking[0], flanking[1]]
                } else if flanking.len() == 1 {
                    [flanking[0], flanking[0]]
                } else {
                    [[primary_coarse; 3], [primary_coarse; 3]]
                };
                coarse_to_fine_children[primary_coarse].push(fine_idx);
                parentage.push(FineParentage::CoarseEdge {
                    primary_coarse,
                    flanking_coarse_vertices: flanking_arr,
                });
            }
        }

        // Clamped non-negative coarse elevations (`Tile.elevation` in `tile.py:81`) and normalized
        // mountainosities (`min(20.0, m) / 20.0` in `tile.py:292`) for terrain interpolation.
        let clamped_elevs: Vec<f32> = coarse_elevations.iter().map(|&e| e.max(0.0)).collect();
        let norm_mounts: Vec<f32> = coarse_mountainosities
            .iter()
            .map(|&m| m.clamp(0.0, 20.0) / 20.0)
            .collect();

        // Buildcomposite display faces (`terrainsimulation.py:84-94`).
        let mut display_faces = Vec::with_capacity(num_fine + num_coarse);
        let mut seen_faces = BTreeSet::new();

        for (fine_idx, (f_vec, verts)) in terrain_grid.faces.iter().enumerate() {
            seen_faces.insert(Vec3Key::from_vec3(*f_vec));
            display_faces.push((*f_vec, verts.clone(), DisplayFaceSource::Fine(fine_idx)));
        }
        for (f_vec, verts) in &level1.faces {
            let f_key = Vec3Key::from_vec3(*f_vec);
            if seen_faces.insert(f_key) {
                let ocean_coarse = if let Some(&c_idx) = coarse.face_index.get(&f_key) {
                    c_idx
                } else if let Some(cfs) = coarse.vertices.get(&f_key) {
                    cfs.iter()
                        .map(|&cf| coarse.face_index[&Vec3Key::from_vec3(cf)])
                        .find(|&ci| coarse_elevations[ci] <= 0.0)
                        .unwrap_or_else(|| coarse.face_index[&Vec3Key::from_vec3(cfs[0])])
                } else {
                    0
                };
                display_faces.push((
                    *f_vec,
                    verts.clone(),
                    DisplayFaceSource::OceanCoarse(ocean_coarse),
                ));
            }
        }
        for (c_idx, (f_vec, verts)) in coarse.faces.iter().enumerate() {
            let f_key = Vec3Key::from_vec3(*f_vec);
            if seen_faces.insert(f_key) {
                display_faces.push((
                    *f_vec,
                    verts.clone(),
                    DisplayFaceSource::OceanCoarse(c_idx),
                ));
            }
        }

        let mut grid = Self {
            coarse_size: coarse.size,
            terrain_grid,
            terrain_adj,
            parentage,
            coarse_to_fine_children,
            fine_elevations: Vec::new(),
            display_faces,
        };

        let mut fine_elevations = Vec::with_capacity(num_fine);
        for fine_idx in 0..num_fine {
            let base_elev = grid.interpolate_coarse(fine_idx, &clamped_elevs);
            let mount = grid.interpolate_coarse(fine_idx, &norm_mounts);
            let f = grid.terrain_grid.faces[fine_idx].0;
            let noise = snoise3(f[0], f[1], f[2], 7, 0.85);
            fine_elevations.push(base_elev + mount * noise);
        }
        grid.fine_elevations = fine_elevations;

        grid
    }

    /// Interpolate a coarse-tile scalar field onto `fine_idx` (`terrainmethod.py:19-35`).
    #[inline]
    pub fn interpolate_coarse(&self, fine_idx: usize, coarse_values: &[f32]) -> f32 {
        match self.parentage[fine_idx] {
            FineParentage::CoarseFace(c) => coarse_values[c],
            FineParentage::CoarseVertex([c0, c1, c2]) => {
                (coarse_values[c0] + coarse_values[c1] + coarse_values[c2]) / 3.0
            }
            FineParentage::CoarseEdge {
                primary_coarse,
                flanking_coarse_vertices: [v1, v2],
            } => {
                let v1_avg = (coarse_values[v1[0]] + coarse_values[v1[1]] + coarse_values[v1[2]]) / 3.0;
                let v2_avg = (coarse_values[v2[0]] + coarse_values[v2[1]] + coarse_values[v2[2]]) / 3.0;
                (coarse_values[primary_coarse] + v1_avg + v2_avg) / 3.0
            }
        }
    }

    /// Primary coarse parent index for `fine_idx` (for `CoarseVertex`, returns the first elevated parent).
    #[inline]
    pub fn primary_coarse_tile(&self, fine_idx: usize, coarse_elevations: &[f32]) -> usize {
        match self.parentage[fine_idx] {
            FineParentage::CoarseFace(c) => c,
            FineParentage::CoarseEdge { primary_coarse, .. } => primary_coarse,
            FineParentage::CoarseVertex(cs) => cs
                .into_iter()
                .find(|&c| coarse_elevations[c] > 0.0)
                .unwrap_or(cs[0]),
        }
    }

    /// Route coarse rivers across the subdivided terrain grid (`terrainmethod.py:41-86`).
    pub fn route_rivers(
        &self,
        coarse: &SphericalGrid,
        coarse_rivers: &[Vec<usize>],
    ) -> Vec<Vec<usize>> {
        let mut routes = Vec::with_capacity(coarse_rivers.len());

        for r in coarse_rivers {
            if r.is_empty() {
                continue;
            }
            let start_key = Vec3Key::from_vec3(coarse.faces[r[0]].0);
            let Some(&start_fine) = self.terrain_grid.face_index.get(&start_key) else {
                continue;
            };
            let mut route = vec![start_fine];

            for i in 0..r.len() {
                let pool = &self.coarse_to_fine_children[r[i]];
                if pool.is_empty() {
                    continue;
                }
                let pool_set: BTreeSet<usize> = pool.iter().copied().collect();

                let edge: BTreeSet<usize> = if i + 1 < r.len() {
                    let next_center = coarse.faces[r[i + 1]].0;
                    let mut sorted_pool = pool.clone();
                    sorted_pool.sort_by(|&a, &b| {
                        let da = dist2(self.terrain_grid.faces[a].0, next_center);
                        let db = dist2(self.terrain_grid.faces[b].0, next_center);
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    sorted_pool.into_iter().take(3).collect()
                } else {
                    let mut sorted_pool = pool.clone();
                    sorted_pool.sort_by(|&a, &b| {
                        self.fine_elevations[a]
                            .partial_cmp(&self.fine_elevations[b])
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    sorted_pool.into_iter().take(1).collect()
                };

                let cur = *route.last().unwrap();
                let mut seen = BTreeSet::new();
                if let Some(steps) = find_fine_path(
                    cur,
                    &edge,
                    &pool_set,
                    &mut seen,
                    &self.terrain_adj,
                    &self.fine_elevations,
                ) {
                    for &s in &steps {
                        if route.last().copied() != Some(s) {
                            route.push(s);
                        }
                    }
                }
            }

            routes.push(route);
        }

        routes
    }

    /// Build Three.js triangulated sphere geometry (`positions`, `normals`, `vertex_display_face_ids`)
    /// for all composite `display_faces` (`sphereview.py:50-62`).
    pub fn build_display_mesh(&self) -> (Vec<f32>, Vec<f32>, Vec<u32>) {
        let total_tris: usize = self.display_faces.iter().map(|(_, vs, _)| vs.len()).sum();
        let mut positions = Vec::with_capacity(total_tris * 9);
        let mut normals = Vec::with_capacity(total_tris * 9);
        let mut vertex_face_ids = Vec::with_capacity(total_tris * 3);

        for (d_idx, (center, verts, _)) in self.display_faces.iter().enumerate() {
            let c = [-center[1] as f32, center[2] as f32, center[0] as f32];
            let n_vec = normal(*center);
            let n = [-n_vec[1] as f32, n_vec[2] as f32, n_vec[0] as f32];
            let k = verts.len();
            for i in 0..k {
                let v_prev = verts[(i + k - 1) % k];
                let v_curr = verts[i];
                let vp = [-v_prev[1] as f32, v_prev[2] as f32, v_prev[0] as f32];
                let vc = [-v_curr[1] as f32, v_curr[2] as f32, v_curr[0] as f32];

                positions.extend_from_slice(&c);
                positions.extend_from_slice(&vp);
                positions.extend_from_slice(&vc);

                normals.extend_from_slice(&n);
                normals.extend_from_slice(&n);
                normals.extend_from_slice(&n);

                let id = d_idx as u32;
                vertex_face_ids.push(id);
                vertex_face_ids.push(id);
                vertex_face_ids.push(id);
            }
        }

        (positions, normals, vertex_face_ids)
    }
}

fn find_fine_path(
    source: usize,
    edge: &BTreeSet<usize>,
    pool: &BTreeSet<usize>,
    seen: &mut BTreeSet<usize>,
    adj: &[Vec<usize>],
    elevations: &[f32],
) -> Option<Vec<usize>> {
    seen.insert(source);
    if edge.contains(&source) {
        return Some(vec![source]);
    }

    let mut neighbors = adj[source].clone();
    neighbors.sort_by(|&a, &b| {
        elevations[a]
            .partial_cmp(&elevations[b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });

    for n in neighbors {
        if pool.contains(&n) && !seen.contains(&n) {
            if let Some(mut steps) = find_fine_path(n, edge, pool, seen, adj, elevations) {
                let mut out = Vec::with_capacity(1 + steps.len());
                out.push(source);
                out.append(&mut steps);
                return Some(out);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subdivided_terrain_grid_and_river_routing() {
        let coarse = SphericalGrid::with_size(4);
        let n = coarse.faces.len();
        let mut elevs = vec![-4.0f32; n];
        let mut mounts = vec![0.0f32; n];

        // Create a small connected continent of 8 tiles with a mountain peak
        elevs[0] = 6.5;
        mounts[0] = 12.0;
        for &nb in &coarse.adjacency()[0] {
            elevs[nb] = 2.0;
            mounts[nb] = 6.0;
        }

        let sub = SubdividedTerrainGrid::from_coarse(&coarse, &elevs, &mounts);
        assert!(sub.terrain_grid.faces.len() > 40);
        assert_eq!(sub.terrain_grid.faces.len(), sub.fine_elevations.len());
        assert!(sub.display_faces.len() > n);

        // Route a coarse river from peak tile 0 through one neighbor to an ocean neighbor
        let n1 = coarse.adjacency()[0][0];
        let ocean_nb = coarse.adjacency()[n1]
            .iter()
            .copied()
            .find(|&u| elevs[u] <= 0.0)
            .unwrap();
        let routes = sub.route_rivers(&coarse, &[vec![0, n1, ocean_nb]]);
        assert_eq!(routes.len(), 1);
        assert!(routes[0].len() >= 3);
    }
}
