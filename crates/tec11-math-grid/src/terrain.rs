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

/// A polygon face in the composite sphere mesh (`terrainsimulation.py:84-94`).
#[derive(Clone, Debug)]
pub struct DisplayFace {
    pub center: Vec3,
    pub vertices: Vec<Vec3>,
    pub source: DisplayFaceSource,
}

/// Triangulated Three.js sphere geometry for all composite [`DisplayFace`]s (`sphereview.py:50-62`).
#[derive(Clone, Debug)]
pub struct DisplayMesh {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub vertex_face_ids: Vec<u32>,
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
    pub display_faces: Vec<DisplayFace>,
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
        for (c_idx, &elev) in coarse_elevations.iter().enumerate().take(num_coarse) {
            if elev > 0.0 {
                level1.populate_from_parent(coarse, coarse.faces[c_idx].0);
            }
        }

        // Snapshot the level-1 faces generated from land tiles (`list(terrain.prev.faces.keys())`).
        let level1_land_faces: Vec<Vec3> = level1.faces.iter().map(|&(f, _)| f).collect();

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
        terrain_grid.vertices.clear();
        let num_fine = terrain_grid.faces.len();

        let resolve_coarse_vertex = |vkey: &Vec3Key| -> [usize; 3] {
            let cfs = &coarse.vertices[vkey];
            [
                coarse.face_index[&Vec3Key::from_vec3(cfs[0])],
                coarse.face_index[&Vec3Key::from_vec3(cfs[1])],
                coarse.face_index[&Vec3Key::from_vec3(cfs[2])],
            ]
        };

        // Classify `FineParentage` and `coarse_to_fine_children` (`terrainmethod.py:19-28, 59-74`).
        let mut coarse_to_fine_children = vec![Vec::new(); num_coarse];
        let parentage: Vec<FineParentage> = terrain_grid
            .faces
            .iter()
            .enumerate()
            .map(|(fine_idx, &(f_vec, _))| {
                let f_key = Vec3Key::from_vec3(f_vec);
                if let Some(&c_idx) = coarse.face_index.get(&f_key) {
                    coarse_to_fine_children[c_idx].push(fine_idx);
                    FineParentage::CoarseFace(c_idx)
                } else if coarse.vertices.contains_key(&f_key) {
                    let c_triple = resolve_coarse_vertex(&f_key);
                    for &c_idx in &c_triple {
                        coarse_to_fine_children[c_idx].push(fine_idx);
                    }
                    FineParentage::CoarseVertex(c_triple)
                } else {
                    // Edge face: vertex of `level1` between 3 `level1` faces,
                    // exactly 1 of which is in `coarse.faces` and 2 of which are in `coarse.vertices`.
                    let l1_faces = &level1.vertices[&f_key];
                    let primary_coarse = l1_faces
                        .iter()
                        .find_map(|&pf| coarse.face_index.get(&Vec3Key::from_vec3(pf)).copied())
                        .unwrap_or(0);
                    let flanking: Vec<[usize; 3]> = l1_faces
                        .iter()
                        .filter_map(|&pf| {
                            let pf_key = Vec3Key::from_vec3(pf);
                            coarse
                                .vertices
                                .contains_key(&pf_key)
                                .then(|| resolve_coarse_vertex(&pf_key))
                        })
                        .collect();
                    let flanking_coarse_vertices = match flanking.as_slice() {
                        [f0, f1, ..] => [*f0, *f1],
                        [f0] => [*f0, *f0],
                        [] => [[primary_coarse; 3], [primary_coarse; 3]],
                    };
                    coarse_to_fine_children[primary_coarse].push(fine_idx);
                    FineParentage::CoarseEdge {
                        primary_coarse,
                        flanking_coarse_vertices,
                    }
                }
            })
            .collect();

        // Clamped non-negative coarse elevations (`Tile.elevation` in `tile.py:81`) and normalized
        // mountainosities (`min(20.0, m) / 20.0` in `tile.py:292`) for terrain interpolation.
        let clamped_elevs: Vec<f32> = coarse_elevations.iter().map(|&e| e.max(0.0)).collect();
        let norm_mounts: Vec<f32> = coarse_mountainosities
            .iter()
            .map(|&m| m.clamp(0.0, 20.0) / 20.0)
            .collect();

        // Build composite display faces (`terrainsimulation.py:84-94`).
        let mut seen_faces = BTreeSet::new();
        let fine_display_faces =
            terrain_grid
                .faces
                .iter()
                .enumerate()
                .map(|(fine_idx, (f_vec, verts))| {
                    seen_faces.insert(Vec3Key::from_vec3(*f_vec));
                    DisplayFace {
                        center: *f_vec,
                        vertices: verts.clone(),
                        source: DisplayFaceSource::Fine(fine_idx),
                    }
                });

        let mut display_faces: Vec<DisplayFace> = Vec::with_capacity(num_fine + num_coarse);
        display_faces.extend(fine_display_faces);

        display_faces.extend(level1.faces.iter().filter_map(|(f_vec, verts)| {
            let f_key = Vec3Key::from_vec3(*f_vec);
            seen_faces.insert(f_key).then(|| {
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
                DisplayFace {
                    center: *f_vec,
                    vertices: verts.clone(),
                    source: DisplayFaceSource::OceanCoarse(ocean_coarse),
                }
            })
        }));

        display_faces.extend(
            coarse
                .faces
                .iter()
                .enumerate()
                .filter_map(|(c_idx, (f_vec, verts))| {
                    seen_faces.insert(Vec3Key::from_vec3(*f_vec)).then(|| DisplayFace {
                        center: *f_vec,
                        vertices: verts.clone(),
                        source: DisplayFaceSource::OceanCoarse(c_idx),
                    })
                }),
        );

        let mut grid = Self {
            coarse_size: coarse.size,
            terrain_grid,
            terrain_adj,
            parentage,
            coarse_to_fine_children,
            fine_elevations: Vec::new(),
            display_faces,
        };

        grid.fine_elevations = (0..num_fine)
            .map(|fine_idx| {
                let base_elev = grid.interpolate_coarse(fine_idx, &clamped_elevs);
                let mount = grid.interpolate_coarse(fine_idx, &norm_mounts);
                let f = grid.terrain_grid.faces[fine_idx].0;
                let noise = snoise3(f[0], f[1], f[2], 7, 0.85);
                base_elev + mount * noise
            })
            .collect();

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
                let v1_avg =
                    (coarse_values[v1[0]] + coarse_values[v1[1]] + coarse_values[v1[2]]) / 3.0;
                let v2_avg =
                    (coarse_values[v2[0]] + coarse_values[v2[1]] + coarse_values[v2[2]]) / 3.0;
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
    /// Each coarse transition `r[i] -> r[i + 1]` routes deterministically from the center fine face
    /// of `r[i]` to the center fine face of `r[i + 1]` (or the coastal mouth fine face adjacent to
    /// the ocean tile `r.last()`) along the shortest monotonic descent path, so tributaries that
    /// join at a coarse tile converge at its center fine face and never cross or braid.
    pub fn route_rivers(
        &self,
        coarse: &SphericalGrid,
        coarse_rivers: &[Vec<usize>],
    ) -> Vec<Vec<usize>> {
        coarse_rivers
            .iter()
            .filter_map(|r| {
                let &first_coarse = r.first()?;
                let start_key = Vec3Key::from_vec3(coarse.faces[first_coarse].0);
                let &start_fine = self.terrain_grid.face_index.get(&start_key)?;
                let route = (0..r.len()).fold(vec![start_fine], |mut route, i| {
                    let c_curr = r[i];
                    let pool_curr = &self.coarse_to_fine_children[c_curr];
                    if pool_curr.is_empty() {
                        return route;
                    }
                    let Some(&next_coarse) = r.get(i + 1) else {
                        return route;
                    };
                    let next_center = coarse.faces[next_coarse].0;
                    let next_key = Vec3Key::from_vec3(next_center);

                    let mut allowed: BTreeSet<usize> = pool_curr.iter().copied().collect();
                    let target_fine = if let Some(&next_center_fine) =
                        self.terrain_grid.face_index.get(&next_key)
                    {
                        allowed.extend(self.coarse_to_fine_children[next_coarse].iter().copied());
                        next_center_fine
                    } else {
                        // `next_coarse` is an un-subdivided ocean tile: route to the coastal fine face
                        // in `pool_curr` closest to `next_center`.
                        *pool_curr
                            .iter()
                            .min_by(|&&a, &&b| {
                                let da = dist2(self.terrain_grid.faces[a].0, next_center);
                                let db = dist2(self.terrain_grid.faces[b].0, next_center);
                                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .unwrap_or(&start_fine)
                    };

                    let cur = *route.last().unwrap();
                    for s in find_shortest_fine_segment(
                        cur,
                        target_fine,
                        &allowed,
                        &self.terrain_adj,
                        &self.terrain_grid.faces,
                        &self.fine_elevations,
                    ) {
                        if route.last().copied() != Some(s) {
                            route.push(s);
                        }
                    }
                    route
                });
                Some(route)
            })
            .collect()
    }

    /// Build Three.js triangulated sphere geometry (`positions`, `normals`, `vertex_face_ids`)
    /// for all composite `display_faces` (`sphereview.py:50-62`).
    pub fn build_display_mesh(&self) -> DisplayMesh {
        let total_tris: usize = self
            .display_faces
            .iter()
            .map(|face| face.vertices.len())
            .sum();
        let mut positions = Vec::with_capacity(total_tris * 9);
        let mut normals = Vec::with_capacity(total_tris * 9);
        let mut vertex_face_ids = Vec::with_capacity(total_tris * 3);

        for (d_idx, face) in self.display_faces.iter().enumerate() {
            let center = face.center;
            let verts = &face.vertices;
            let c = [-center[1] as f32, center[2] as f32, center[0] as f32];
            let n_vec = normal(center);
            let n = [-n_vec[1] as f32, n_vec[2] as f32, n_vec[0] as f32];
            let k = verts.len();
            let id = d_idx as u32;
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

                vertex_face_ids.extend_from_slice(&[id, id, id]);
            }
        }

        DisplayMesh {
            positions,
            normals,
            vertex_face_ids,
        }
    }
}

fn find_shortest_fine_segment(
    source: usize,
    target: usize,
    allowed: &BTreeSet<usize>,
    adj: &[Vec<usize>],
    faces: &[(Vec3, Vec<Vec3>)],
    elevations: &[f32],
) -> Vec<usize> {
    if source == target {
        return vec![source];
    }
    let target_pos = faces[target].0;
    let mut path = vec![source];
    let mut curr = source;
    let mut visited = BTreeSet::from([source]);

    for _ in 0..8 {
        if curr == target {
            break;
        }
        let curr_dist = dist2(faces[curr].0, target_pos);
        let next_opt = adj[curr]
            .iter()
            .copied()
            .filter(|&nb| {
                allowed.contains(&nb)
                    && !visited.contains(&nb)
                    && (nb == target || dist2(faces[nb].0, target_pos) < curr_dist)
            })
            .min_by(|&a, &b| {
                let da = dist2(faces[a].0, target_pos);
                let db = dist2(faces[b].0, target_pos);
                let score_a = da + (elevations[a].max(0.0) as f64) * 0.0002;
                let score_b = db + (elevations[b].max(0.0) as f64) * 0.0002;
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(&b))
            });
        let Some(next_face) = next_opt else {
            break;
        };
        visited.insert(next_face);
        path.push(next_face);
        curr = next_face;
    }
    path
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
