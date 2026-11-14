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

impl FineParentage {
    /// Interpolate a coarse-tile scalar field onto this fine cell (`terrainmethod.py:19-35`).
    #[inline]
    pub fn interpolate(&self, coarse_values: &[f32]) -> f32 {
        match *self {
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

    /// Primary coarse parent index (for `CoarseVertex`, returns the first land parent if any, else `cs[0]`).
    #[inline]
    pub fn primary_coarse_tile(&self, coarse_elevations: &[f32]) -> usize {
        match *self {
            FineParentage::CoarseFace(c) => c,
            FineParentage::CoarseEdge { primary_coarse, .. } => primary_coarse,
            FineParentage::CoarseVertex(cs) => cs
                .into_iter()
                .find(|&c| coarse_elevations[c] > 0.0)
                .unwrap_or(cs[0]),
        }
    }
}

/// Shared classification of `FineParentage` and `coarse_to_fine_children` for any `N + 2` fine grid
/// (partial land-only `SubdividedTerrainGrid` or uniform global `FullSphereFineGrid`) given `coarse` (`N`)
/// and `level1` (`N + 1`, with `level1.vertices` intact).
pub fn classify_fine_parentage(
    coarse: &SphericalGrid,
    level1: &SphericalGrid,
    fine_grid: &SphericalGrid,
) -> (Vec<FineParentage>, Vec<Vec<usize>>) {
    let num_coarse = coarse.faces.len();
    let resolve_coarse_vertex = |vkey: &Vec3Key| -> [usize; 3] {
        let cfs = &coarse.vertices[vkey];
        [
            coarse.face_index[&Vec3Key::from_vec3(cfs[0])],
            coarse.face_index[&Vec3Key::from_vec3(cfs[1])],
            coarse.face_index[&Vec3Key::from_vec3(cfs[2])],
        ]
    };

    let mut coarse_to_fine_children = vec![Vec::new(); num_coarse];
    let parentage: Vec<FineParentage> = fine_grid
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

    (parentage, coarse_to_fine_children)
}

/// Shared 7-octave Simplex fractal land elevation synthesis (`terrainmethod.py:36-38`).
#[inline]
pub fn compute_fine_land_elevation(pos: Vec3, clamped_base_elev_km: f32, norm_mount: f32) -> f32 {
    let noise = snoise3(pos[0], pos[1], pos[2], 7, 0.85);
    clamped_base_elev_km + norm_mount * noise
}

/// Shared coarse-to-fine river routing across any `N + 2` fine grid (`terrainmethod.py:41-86`).
/// When `coarse_is_land` is `Some(mask)`, ocean mouths are identified via `!mask[next_coarse]`
/// (needed on `FullSphereFineGrid` where ocean coarse centers also exist in `fine_grid.face_index`).
pub fn route_coarse_rivers_on_fine_grid(
    coarse: &SphericalGrid,
    coarse_rivers: &[Vec<usize>],
    coarse_is_land: Option<&[bool]>,
    fine_grid: &SphericalGrid,
    fine_adj: &[Vec<usize>],
    coarse_to_fine_children: &[Vec<usize>],
    fine_elevations: &[f32],
) -> Vec<Vec<usize>> {
    coarse_rivers
        .iter()
        .filter_map(|r| {
            let &first_coarse = r.first()?;
            if let Some(land_mask) = coarse_is_land {
                if !land_mask.get(first_coarse).copied().unwrap_or(false) {
                    return None;
                }
            }
            let start_key = Vec3Key::from_vec3(coarse.faces[first_coarse].0);
            let &start_fine = fine_grid.face_index.get(&start_key)?;
            let route = (0..r.len()).fold(vec![start_fine], |mut route, i| {
                let c_curr = r[i];
                let pool_curr = &coarse_to_fine_children[c_curr];
                if pool_curr.is_empty() {
                    return route;
                }
                let Some(&next_coarse) = r.get(i + 1) else {
                    return route;
                };
                let next_center = coarse.faces[next_coarse].0;
                let next_key = Vec3Key::from_vec3(next_center);
                let next_is_land = coarse_is_land
                    .map(|m| m.get(next_coarse).copied().unwrap_or(false))
                    .unwrap_or(true);

                let mut allowed: BTreeSet<usize> = pool_curr.iter().copied().collect();
                let target_fine = if next_is_land {
                    if let Some(&next_center_fine) = fine_grid.face_index.get(&next_key) {
                        allowed.extend(coarse_to_fine_children[next_coarse].iter().copied());
                        next_center_fine
                    } else {
                        *pool_curr
                            .iter()
                            .min_by(|&&a, &&b| {
                                let da = dist2(fine_grid.faces[a].0, next_center);
                                let db = dist2(fine_grid.faces[b].0, next_center);
                                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .unwrap_or(&start_fine)
                    }
                } else {
                    // `next_coarse` is an ocean tile: route to the coastal fine face in `pool_curr`
                    // closest to `next_center`.
                    *pool_curr
                        .iter()
                        .min_by(|&&a, &&b| {
                            let da = dist2(fine_grid.faces[a].0, next_center);
                            let db = dist2(fine_grid.faces[b].0, next_center);
                            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .unwrap_or(&start_fine)
                };

                let cur = *route.last().unwrap();
                for s in find_shortest_fine_segment(
                    cur,
                    target_fine,
                    &allowed,
                    fine_adj,
                    &fine_grid.faces,
                    fine_elevations,
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

        let (parentage, coarse_to_fine_children) =
            classify_fine_parentage(coarse, &level1, &terrain_grid);

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
                compute_fine_land_elevation(f, base_elev, mount)
            })
            .collect();

        grid
    }

    /// Interpolate a coarse-tile scalar field onto `fine_idx` (`terrainmethod.py:19-35`).
    #[inline]
    pub fn interpolate_coarse(&self, fine_idx: usize, coarse_values: &[f32]) -> f32 {
        self.parentage[fine_idx].interpolate(coarse_values)
    }

    /// Primary coarse parent index for `fine_idx` (for `CoarseVertex`, returns the first elevated parent).
    #[inline]
    pub fn primary_coarse_tile(&self, fine_idx: usize, coarse_elevations: &[f32]) -> usize {
        self.parentage[fine_idx].primary_coarse_tile(coarse_elevations)
    }

    /// Route coarse rivers across the subdivided terrain grid (`terrainmethod.py:41-86`).
    pub fn route_rivers(
        &self,
        coarse: &SphericalGrid,
        coarse_rivers: &[Vec<usize>],
    ) -> Vec<Vec<usize>> {
        route_coarse_rivers_on_fine_grid(
            coarse,
            coarse_rivers,
            None,
            &self.terrain_grid,
            &self.terrain_adj,
            &self.coarse_to_fine_children,
            &self.fine_elevations,
        )
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

/// Uniform global 2-level (`coarse.size + 2`) spherical fine grid subdividing **both** land and
/// ocean bathymetry (`65,612` tiles at `~88 km` on `size = 6` Earth) while sharing `FineParentage`,
/// 7-octave Simplex land topography, and river routing with [`SubdividedTerrainGrid`].
#[derive(Clone, Debug)]
pub struct FullSphereFineGrid {
    pub coarse_size: u32,
    pub grid: SphericalGrid,
    pub adjacency: Vec<Vec<usize>>,
    pub parentage: Vec<FineParentage>,
    pub coarse_to_fine_children: Vec<Vec<usize>>,
    /// Signed elevation in kilometers (`> 0.0` subaerial land, `<= 0.0` continental shelf / abyssal ocean).
    pub fine_elevations_km: Vec<f32>,
    /// Solid bed elevation $z_b$ in meters (`fine_elevations_km * 1000.0`) for shallow-water & storm-surge PDEs.
    pub fine_bed_m: Vec<f32>,
    /// Normalized mountainosity in `[0.0, 1.0]` for rough-terrain drag, orographic lift, and vantage crags.
    pub fine_mountainosity_norm: Vec<f32>,
    /// Primary coarse parent tile index (`0..coarse.faces.len()`) for stratigraphy, rock, and Köppen lookup.
    pub primary_coarse: Vec<usize>,
    /// Coarse land mask (`coarse_elevations_km[c] > 0.0`) used for river mouth detection.
    pub coarse_is_land: Vec<bool>,
}

impl FullSphereFineGrid {
    /// Subdivide the entire sphere uniformly from `coarse` (`N`) to `N + 2`, preserving exact
    /// interior land elevation parity with [`SubdividedTerrainGrid`] while synthesizing smooth
    /// continental shelves (`-18 m .. -220 m`), slopes, and multi-octave seafloor bathymetry.
    pub fn from_coarse(
        coarse: &SphericalGrid,
        coarse_elevations_km: &[f32],
        coarse_mountainosities: &[f32],
    ) -> Self {
        let num_coarse = coarse.faces.len();
        assert_eq!(num_coarse, coarse_elevations_km.len());
        assert_eq!(num_coarse, coarse_mountainosities.len());

        let level1 = coarse.subdivide();
        let mut grid = level1.subdivide();
        let adjacency = grid.adjacency();
        grid.vertices.clear();
        let num_fine = grid.faces.len();

        let (parentage, coarse_to_fine_children) =
            classify_fine_parentage(coarse, &level1, &grid);

        let coarse_is_land: Vec<bool> = coarse_elevations_km.iter().map(|&e| e > 0.0).collect();
        let coarse_land_indicator: Vec<f32> = coarse_is_land
            .iter()
            .map(|&is_l| if is_l { 1.0 } else { 0.0 })
            .collect();
        let clamped_land_elevs: Vec<f32> = coarse_elevations_km.iter().map(|&e| e.max(0.0)).collect();
        let ocean_depths_km: Vec<f32> = coarse_elevations_km
            .iter()
            .map(|&e| {
                if e > 0.0 {
                    0.0
                } else if e < -0.001 {
                    e
                } else {
                    // Fallback abyssal depth if an un-bathymetric ocean tile was clamped to 0.0
                    -3.6
                }
            })
            .collect();
        let norm_mounts: Vec<f32> = coarse_mountainosities
            .iter()
            .map(|&m| m.clamp(0.0, 20.0) / 20.0)
            .collect();

        let mut fine_elevations_km = Vec::with_capacity(num_fine);
        let mut fine_bed_m = Vec::with_capacity(num_fine);
        let mut fine_mountainosity_norm = Vec::with_capacity(num_fine);
        let mut primary_coarse = Vec::with_capacity(num_fine);

        for fine_idx in 0..num_fine {
            let p = &parentage[fine_idx];
            let pos = grid.faces[fine_idx].0;
            let land_weight = p.interpolate(&coarse_land_indicator);
            let land_base_km = p.interpolate(&clamped_land_elevs);
            let ocean_base_km = p.interpolate(&ocean_depths_km);
            let mount_norm = p.interpolate(&norm_mounts).clamp(0.0, 1.0);
            let prim_c = p.primary_coarse_tile(coarse_elevations_km);

            let elev_km = if land_weight >= 0.999 {
                // Pure interior land: bit-for-bit identical to `SubdividedTerrainGrid`!
                compute_fine_land_elevation(pos, land_base_km, mount_norm)
            } else if land_weight >= 0.5 {
                // Land-dominated coastal plain / headland: matches `SubdividedTerrainGrid` land formula,
                // allowing low-lying coastal marshes (+1m..+40m) for realistic tidal/surge wetting.
                let raw_land = compute_fine_land_elevation(pos, land_base_km, mount_norm);
                raw_land.max(0.0015)
            } else if land_weight > 1e-5 {
                // Ocean-dominated coastal margin (`0 < land_weight < 0.5`): realistic continental shelf
                // (`-15 m .. -220 m`) sloping toward the continental rise (`ocean_base_km`).
                let s = (1.0 - 2.0 * land_weight).clamp(0.0, 1.0);
                let shelf_noise = snoise3(pos[0] * 2.1, pos[1] * 2.1, pos[2] * 2.1, 4, 0.65);
                let shelf_km = -0.015 - 0.145 * s + 0.22 * ocean_base_km * s * s + 0.012 * shelf_noise;
                shelf_km.clamp(-4.5, -0.006)
            } else {
                // Open ocean basin: interpolated bathymetry + 5-octave abyssal hill / seamount relief,
                // strictly submerged (`<= -20 m`) so phantom islands never appear in open ocean.
                let bathy_noise = snoise3(pos[0] * 1.4, pos[1] * 1.4, pos[2] * 1.4, 5, 0.72);
                let relief_amp = 0.075 + 0.34 * mount_norm;
                (ocean_base_km + relief_amp * bathy_noise).clamp(-9.5, -0.020)
            };

            fine_elevations_km.push(elev_km);
            fine_bed_m.push(elev_km * 1000.0);
            fine_mountainosity_norm.push(mount_norm);
            primary_coarse.push(prim_c);
        }

        Self {
            coarse_size: coarse.size,
            grid,
            adjacency,
            parentage,
            coarse_to_fine_children,
            fine_elevations_km,
            fine_bed_m,
            fine_mountainosity_norm,
            primary_coarse,
            coarse_is_land,
        }
    }

    #[inline]
    pub fn interpolate_coarse(&self, fine_idx: usize, coarse_values: &[f32]) -> f32 {
        self.parentage[fine_idx].interpolate(coarse_values)
    }

    /// Route coarse rivers onto the uniform `coarse + 2` fine grid, terminating at coastal mouths.
    pub fn route_rivers(
        &self,
        coarse: &SphericalGrid,
        coarse_rivers: &[Vec<usize>],
    ) -> Vec<Vec<usize>> {
        route_coarse_rivers_on_fine_grid(
            coarse,
            coarse_rivers,
            Some(&self.coarse_is_land),
            &self.grid,
            &self.adjacency,
            &self.coarse_to_fine_children,
            &self.fine_elevations_km,
        )
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

    #[test]
    fn test_full_sphere_fine_grid_counts_land_parity_shelves_and_rivers() {
        let coarse = SphericalGrid::with_size(4);
        let n = coarse.faces.len();
        let mut elevs = vec![-4.2f32; n];
        let mut mounts = vec![1.5f32; n];

        elevs[0] = 5.2;
        mounts[0] = 14.0;
        for &nb in &coarse.adjacency()[0] {
            elevs[nb] = 1.8;
            mounts[nb] = 5.0;
        }

        let sub = SubdividedTerrainGrid::from_coarse(&coarse, &elevs, &mounts);
        let full = FullSphereFineGrid::from_coarse(&coarse, &elevs, &mounts);

        // Size 4 + 2 = Size 6 -> 7,292 fine cells across the full sphere
        assert_eq!(full.grid.size, 6);
        assert_eq!(full.grid.faces.len(), 7292);
        assert_eq!(full.fine_elevations_km.len(), 7292);
        assert_eq!(full.fine_bed_m.len(), 7292);

        // Interior coarse tile 0 is surrounded purely by land neighbors -> all 7 interior fine faces
        // (center + 6 CoarseEdge) and all 6 CoarseVertex faces around tile 0 must match `SubdividedTerrainGrid` bit-for-bit!
        for &full_fine_idx in &full.coarse_to_fine_children[0] {
            let f_key = Vec3Key::from_vec3(full.grid.faces[full_fine_idx].0);
            let sub_fine_idx = sub.terrain_grid.face_index[&f_key];
            assert_eq!(
                full.fine_elevations_km[full_fine_idx],
                sub.fine_elevations[sub_fine_idx],
                "Interior fine land cell must match SubdividedTerrainGrid exactly"
            );
        }

        // Verify continental shelf cells (-250 m .. -5 m) and deep abyssal ocean cells (<-2000 m) both exist
        let shelf_count = full
            .fine_bed_m
            .iter()
            .filter(|&&z| (-250.0..=-5.0).contains(&z))
            .count();
        let abyssal_count = full.fine_bed_m.iter().filter(|&&z| z < -2500.0).count();
        assert!(shelf_count > 10, "Expected continental shelf cells around island continent");
        assert!(abyssal_count > 5000, "Expected deep ocean basin cells");

        // Verify river routing produces valid connected path from peak center to coastal mouth
        let n1 = coarse.adjacency()[0][0];
        let ocean_nb = coarse.adjacency()[n1]
            .iter()
            .copied()
            .find(|&u| elevs[u] <= 0.0)
            .unwrap();
        let full_routes = full.route_rivers(&coarse, &[vec![0, n1, ocean_nb]]);
        assert_eq!(full_routes.len(), 1);
        assert!(full_routes[0].len() >= 3);
        for pair in full_routes[0].windows(2) {
            assert!(
                full.adjacency[pair[0]].contains(&pair[1]),
                "Fine river steps must be edge-adjacent"
            );
        }
    }
}
