use std::f64::consts::PI;

use crate::{SphericalGrid, Vec3, Vec3Key};

/// Directed dual edge connecting cell `left` ($i$) to cell `right` ($j$), with `left < right`.
///
/// Positive normal velocity `u_edge > 0` flows from `left` ($i$) to `right` ($j$).
#[derive(Clone, Copy, Debug)]
pub struct DirectedDualEdge {
    pub left: u32,
    pub right: u32,
    /// Geodesic length $l_e$ (m) of the shared primal polygon segment between `left` and `right`.
    pub dual_len_m: f32,
    /// Geodesic distance $d_e$ (m) between cell centers `left` and `right`.
    pub center_dist_m: f32,
    /// Reciprocal distance $1 / d_e$ ($\text{m}^{-1}$).
    pub inv_center_dist_m: f32,
    /// Unit edge normal pointing `left -> right` expressed in `left`'s `(east, north)` basis.
    pub nx_left: f32,
    pub ny_left: f32,
    /// Same physical 3D unit edge normal pointing `left -> right` expressed in `right`'s `(east, north)` basis.
    pub nx_right: f32,
    pub ny_right: f32,
}

/// CSR incidence entry for an edge touching a given cell.
#[derive(Clone, Copy, Debug)]
pub struct CellEdgeIncidence {
    pub edge_idx: u32,
    pub neighbor: u32,
    /// `+1.0` if this cell is `edge.left` (outward normal = `left -> right`), `-1.0` if `edge.right`.
    pub sign: f32,
    /// Signed divergence factor `sign * dual_len_m / area_m2` ($\text{m}^{-1}$).
    pub flux_factor: f32,
    /// Exact first-order consistent Perot/least-squares reconstruction weight for `u_east`
    /// multiplying the signed outward normal velocity `sign * u_edge` (or directly multiplying `u_edge` —
    /// stored pre-multiplied by `sign` so `u_east += u_edge * perot_wx`).
    pub perot_wx: f32,
    /// Pre-signed reconstruction weight so `v_north += u_edge * perot_wy`.
    pub perot_wy: f32,
}

/// Fixed-degree-6 Perot vector-reconstruction stencil ordered by 3D Morton cell code.
/// Pentagon cells pad slot `[5]` with zero weights so inner loops unroll without branches.
#[derive(Clone, Copy, Debug)]
pub struct MortonPerotStencil6 {
    pub cell: u32,
    pub edge_idx: [u32; 6],
    pub perot_wx: [f32; 6],
    pub perot_wy: [f32; 6],
}

/// Fixed-degree-6 conservative edge-flux divergence stencil ordered by 3D Morton cell code.
#[derive(Clone, Copy, Debug)]
pub struct MortonDivStencil6 {
    pub cell: u32,
    pub edge_idx: [u32; 6],
    pub flux_factor: [f32; 6],
}

/// Fixed-degree-6 conservative scalar Laplacian stencil ordered by 3D Morton cell code.
#[derive(Clone, Copy, Debug)]
pub struct MortonLapStencil6 {
    pub cell: u32,
    pub neighbor: [u32; 6],
    pub weight: [f32; 6],
}

/// Fixed-degree-6 cell-centered scalar gradient stencil ordered by 3D Morton cell code.
#[derive(Clone, Copy, Debug)]
pub struct MortonGradStencil6 {
    pub cell: u32,
    pub neighbor: [u32; 6],
    pub wx: [f32; 6],
    pub wy: [f32; 6],
}

/// Cell-indexed fixed-degree-6 Perot stencil for subset loops (e.g. wet ocean cells).
#[derive(Clone, Copy, Debug)]
pub struct CellPerotStencil6 {
    pub edge_idx: [u32; 6],
    pub perot_wx: [f32; 6],
    pub perot_wy: [f32; 6],
}

/// Cell-indexed fixed-degree-6 divergence & neighbor stencil for subset/limiter loops.
#[derive(Clone, Copy, Debug)]
pub struct CellDivStencil6 {
    pub edge_idx: [u32; 6],
    pub neighbor: [u32; 6],
    pub flux_factor: [f32; 6],
}

/// Static spherical finite-volume C-grid geometry for pentagonal/hexagonal `SphericalGrid` meshes.
#[derive(Clone, Debug)]
pub struct FiniteVolumeSphereGeom {
    pub num_cells: usize,
    pub radius_m: f64,
    pub omega_rad_s: f64,
    /// Exact spherical polygon area $A_i$ ($\text{m}^2$) via Van Oosterom & Strackee (1983).
    pub areas_m2: Vec<f32>,
    pub lats_rad: Vec<f32>,
    pub lons_rad: Vec<f32>,
    pub sin_lat: Vec<f32>,
    pub cos_lat: Vec<f32>,
    /// Planetary vorticity $f_i = 2\Omega\sin\phi_i$ ($\text{s}^{-1}$).
    pub coriolis_f: Vec<f32>,
    /// Local unit Eastward vector in `SphericalGrid` `[x, y, z]` coordinates.
    pub east_xyz: Vec<[f32; 3]>,
    /// Local unit Northward vector in `SphericalGrid` `[x, y, z]` coordinates.
    pub north_xyz: Vec<[f32; 3]>,
    /// CSR row offsets of length `num_cells + 1` into `cell_edges`.
    pub cell_edge_offsets: Vec<usize>,
    pub cell_edges: Vec<CellEdgeIncidence>,
    /// Unique undirected dual edges (`left < right`), sorted by 3D Morton midpoint code for cache locality.
    pub edges: Vec<DirectedDualEdge>,
    /// Mean center-to-center distance $\bar{d}_e$ (m), useful for CFL & hyperviscosity scaling.
    pub mean_edge_dist_m: f32,
    /// True when every cell has degree $\le 6$ (always true for pentagonal/hexagonal `SphericalGrid`s).
    pub all_deg_le_6: bool,
    /// Cell indices `0..num_cells` ordered by 30-bit 3D Morton code of cell centers.
    pub morton_cells: Vec<u32>,
    pub morton_perot6: Vec<MortonPerotStencil6>,
    /// Aligned 1:1 with `morton_perot6` for fused `reconstruct_and_div_2layer`.
    pub morton_flux6: Vec<[f32; 6]>,
    pub morton_div6: Vec<MortonDivStencil6>,
    pub morton_lap6: Vec<MortonLapStencil6>,
    pub morton_grad6: Vec<MortonGradStencil6>,
    /// Indexed directly by `cell` (`0..num_cells`).
    pub cell_perot6: Vec<CellPerotStencil6>,
    /// Indexed directly by `cell` (`0..num_cells`).
    pub cell_div6: Vec<CellDivStencil6>,
}

#[inline]
fn spread_bits_10_to_30(mut v: u32) -> u32 {
    v &= 0x0000_03ff;
    v = (v | (v << 16)) & 0x0300_00ff;
    v = (v | (v << 8)) & 0x0300_f00f;
    v = (v | (v << 4)) & 0x030c_30c3;
    v = (v | (v << 2)) & 0x0924_9249;
    v
}

/// 30-bit 3D Morton (Z-order) code for a point near the unit sphere `[-1, 1]^3`.
#[inline]
pub fn morton3d_unit_sphere(p: Vec3) -> u32 {
    let qx = (((p[0] + 1.0) * 0.5).clamp(0.0, 1.0) * 1023.0) as u32;
    let qy = (((p[1] + 1.0) * 0.5).clamp(0.0, 1.0) * 1023.0) as u32;
    let qz = (((p[2] + 1.0) * 0.5).clamp(0.0, 1.0) * 1023.0) as u32;
    spread_bits_10_to_30(qx) | (spread_bits_10_to_30(qy) << 1) | (spread_bits_10_to_30(qz) << 2)
}

#[inline]
fn dot3(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross3(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm3(v: Vec3) -> Vec3 {
    let len = dot3(v, v).sqrt();
    if len > 1e-15 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 0.0]
    }
}

#[inline]
fn geodesic_angle_rad(a: Vec3, b: Vec3) -> f64 {
    let c = cross3(a, b);
    let sin_ab = dot3(c, c).sqrt();
    let cos_ab = dot3(a, b);
    sin_ab.atan2(cos_ab)
}

/// Spherical triangle solid angle $\Omega(\mathbf{a}, \mathbf{b}, \mathbf{c})$ on the unit sphere
/// via Van Oosterom & Strackee (1983), immune to small-triangle catastrophic cancellation.
#[inline]
fn spherical_triangle_solid_angle(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    let bc = cross3(b, c);
    let triple = dot3(a, bc).abs();
    let denom = 1.0 + dot3(a, b) + dot3(b, c) + dot3(c, a);
    2.0 * triple.atan2(denom)
}

#[inline]
fn tangent_basis_f64([x, y, z]: Vec3) -> (Vec3, Vec3) {
    let xy = (x * x + y * y).sqrt();
    if xy > 1e-9 {
        // Matches `ocean.rs` and `model.rs`: lat = asin(z), lon = atan2(-y, x).
        let east = [y / xy, -x / xy, 0.0];
        let north = [-x * z / xy, -y * z / xy, xy];
        (east, north)
    } else {
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0])
    }
}

#[inline]
fn project_unit_2d(vec3: Vec3, east: Vec3, north: Vec3) -> (f64, f64) {
    let vx = dot3(vec3, east);
    let vy = dot3(vec3, north);
    let len = (vx * vx + vy * vy).sqrt();
    if len > 1e-12 {
        (vx / len, vy / len)
    } else {
        (1.0, 0.0)
    }
}

impl FiniteVolumeSphereGeom {
    /// Builds static spherical finite-volume geometry for `grid` with planetary `radius_m`
    /// and sidereal rotation period `day_hours` (use negative `day_hours` for retrograde spin,
    /// or `0.0` / `f64::INFINITY` for non-rotating).
    pub fn new(
        grid: &SphericalGrid,
        adjacency: &[Vec<usize>],
        radius_m: f64,
        day_hours: f64,
    ) -> Self {
        let n = grid.faces.len();
        let omega_rad_s = if day_hours.is_finite() && day_hours.abs() > 1e-6 {
            2.0 * PI / (day_hours * 3600.0)
        } else {
            0.0
        };
        let r2 = radius_m * radius_m;

        let mut areas_m2 = Vec::with_capacity(n);
        let mut lats_rad = Vec::with_capacity(n);
        let mut lons_rad = Vec::with_capacity(n);
        let mut sin_lat = Vec::with_capacity(n);
        let mut cos_lat = Vec::with_capacity(n);
        let mut coriolis_f = Vec::with_capacity(n);
        let mut east_xyz = Vec::with_capacity(n);
        let mut north_xyz = Vec::with_capacity(n);
        let mut bases_f64 = Vec::with_capacity(n);
        let mut cell_centers = Vec::with_capacity(n);
        let mut all_deg_le_6 = true;

        for (cell_idx, &(center, ref verts)) in grid.faces.iter().enumerate() {
            if adjacency[cell_idx].len() > 6 {
                all_deg_le_6 = false;
            }
            let p = norm3(center);
            cell_centers.push(p);
            let mut omega = 0.0_f64;
            let m = verts.len();
            for k in 0..m {
                let v0 = norm3(verts[k]);
                let v1 = norm3(verts[(k + 1) % m]);
                omega += spherical_triangle_solid_angle(p, v0, v1);
            }
            areas_m2.push((omega * r2).max(1.0) as f32);

            let xy = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let lat = p[2].atan2(xy);
            let lon = (-p[1]).atan2(p[0]);
            let s_lat = p[2].clamp(-1.0, 1.0);
            let c_lat = xy.clamp(0.0, 1.0);
            lats_rad.push(lat as f32);
            lons_rad.push(lon as f32);
            sin_lat.push(s_lat as f32);
            cos_lat.push(c_lat as f32);
            coriolis_f.push((2.0 * omega_rad_s * s_lat) as f32);

            let (east, north) = tangent_basis_f64(p);
            bases_f64.push((east, north));
            east_xyz.push([east[0] as f32, east[1] as f32, east[2] as f32]);
            north_xyz.push([north[0] as f32, north[1] as f32, north[2] as f32]);
        }

        // Build unique undirected edges (left < right) and tag each with its 3D Morton midpoint code.
        let mut keyed_edges: Vec<(u32, DirectedDualEdge)> = Vec::with_capacity(n * 3);
        let mut dist_sum = 0.0_f64;

        for i in 0..n {
            let pi = cell_centers[i];
            let verts_i = &grid.faces[i].1;
            for &j in &adjacency[i] {
                if j <= i {
                    continue;
                }
                let pj = cell_centers[j];
                let verts_j = &grid.faces[j].1;

                // Find the 2 shared primal polygon vertices between face i and face j.
                let mut shared: [Vec3; 2] = [[0.0; 3]; 2];
                let mut shared_count = 0usize;
                for &vi in verts_i {
                    let key_i = Vec3Key::from_vec3(vi);
                    if verts_j.iter().any(|&vj| Vec3Key::from_vec3(vj) == key_i) {
                        if shared_count < 2 {
                            shared[shared_count] = norm3(vi);
                        }
                        shared_count += 1;
                    }
                }

                let dual_len_m = if shared_count >= 2 {
                    (geodesic_angle_rad(shared[0], shared[1]) * radius_m).max(1.0)
                } else {
                    (geodesic_angle_rad(pi, pj) * radius_m * 0.577_350_27).max(1.0)
                };
                let center_dist_m = (geodesic_angle_rad(pi, pj) * radius_m).max(1.0);
                dist_sum += center_dist_m;

                let cell_mid = norm3([pi[0] + pj[0], pi[1] + pj[1], pi[2] + pj[2]]);

                // Construct the unit 3D edge normal at the shared edge midpoint pointing `i -> j`.
                let n3d = if shared_count >= 2 {
                    let mid = norm3([
                        shared[0][0] + shared[1][0],
                        shared[0][1] + shared[1][1],
                        shared[0][2] + shared[1][2],
                    ]);
                    let edge_tan = [
                        shared[1][0] - shared[0][0],
                        shared[1][1] - shared[0][1],
                        shared[1][2] - shared[0][2],
                    ];
                    let mut cand = norm3(cross3(edge_tan, mid));
                    let dp = [pj[0] - pi[0], pj[1] - pi[1], pj[2] - pi[2]];
                    if dot3(cand, dp) < 0.0 {
                        cand = [-cand[0], -cand[1], -cand[2]];
                    }
                    if dot3(cand, cand) > 0.5 {
                        cand
                    } else {
                        let dp_tan = [
                            dp[0] - dot3(dp, mid) * mid[0],
                            dp[1] - dot3(dp, mid) * mid[1],
                            dp[2] - dot3(dp, mid) * mid[2],
                        ];
                        norm3(dp_tan)
                    }
                } else {
                    let dp = [pj[0] - pi[0], pj[1] - pi[1], pj[2] - pi[2]];
                    norm3([
                        dp[0] - dot3(dp, cell_mid) * cell_mid[0],
                        dp[1] - dot3(dp, cell_mid) * cell_mid[1],
                        dp[2] - dot3(dp, cell_mid) * cell_mid[2],
                    ])
                };

                let (east_i, north_i) = bases_f64[i];
                let (east_j, north_j) = bases_f64[j];
                let (nx_left, ny_left) = project_unit_2d(n3d, east_i, north_i);
                let (nx_right, ny_right) = project_unit_2d(n3d, east_j, north_j);

                keyed_edges.push((
                    morton3d_unit_sphere(cell_mid),
                    DirectedDualEdge {
                        left: i as u32,
                        right: j as u32,
                        dual_len_m: dual_len_m as f32,
                        center_dist_m: center_dist_m as f32,
                        inv_center_dist_m: (1.0 / center_dist_m) as f32,
                        nx_left: nx_left as f32,
                        ny_left: ny_left as f32,
                        nx_right: nx_right as f32,
                        ny_right: ny_right as f32,
                    },
                ));
            }
        }

        // Sort dual edges in 3D Morton order so edge sweeps keep endpoint cells hot in L1/L2 cache.
        keyed_edges.sort_unstable_by_key(|&(m, ref e)| (m, e.left, e.right));
        let edges: Vec<DirectedDualEdge> = keyed_edges.into_iter().map(|(_, e)| e).collect();

        let mean_edge_dist_m = if edges.is_empty() {
            1.0
        } else {
            (dist_sum / edges.len() as f64) as f32
        };

        // Direct per-cell `(neighbor, edge_idx)` table without HashMap hashing overhead.
        let mut cell_adj_edges: Vec<Vec<(u32, u32)>> = (0..n)
            .map(|i| Vec::with_capacity(adjacency[i].len()))
            .collect();
        for (edge_idx, e) in edges.iter().enumerate() {
            let e_u32 = edge_idx as u32;
            cell_adj_edges[e.left as usize].push((e.right, e_u32));
            cell_adj_edges[e.right as usize].push((e.left, e_u32));
        }

        // Build CSR cell->edge incidence with Perot weights refined by the exact 2x2 cell metric
        // tensor so uniform tangent vector fields reconstruct to machine precision on every
        // hexagon AND pentagon cell.
        let mut cell_edge_offsets = Vec::with_capacity(n + 1);
        let mut cell_edges = Vec::with_capacity(edges.len() * 2);
        cell_edge_offsets.push(0);

        for i in 0..n {
            let area = areas_m2[i] as f64;
            let start = cell_edges.len();
            let adj_edges_i = &cell_adj_edges[i];

            let mut m_xx = 0.0_f64;
            let mut m_xy = 0.0_f64;
            let mut m_yy = 0.0_f64;

            for &j in &adjacency[i] {
                let j_u32 = j as u32;
                let Some(&(_, edge_idx)) = adj_edges_i.iter().find(|&&(nb, _)| nb == j_u32) else {
                    continue;
                };
                let e = &edges[edge_idx as usize];
                let (sign, nx_dir, ny_dir) = if i < j {
                    (1.0_f64, e.nx_left as f64, e.ny_left as f64)
                } else {
                    (-1.0_f64, e.nx_right as f64, e.ny_right as f64)
                };
                // Outward unit normal in cell i's basis is `sign * (nx_dir, ny_dir)`, and
                // `u_outward = sign * u_edge = u_east * n_out_x + v_north * n_out_y`, so
                // `u_edge = u_east * nx_dir + v_north * ny_dir` regardless of sign!
                let w_perot = (e.dual_len_m as f64) * 0.5 * (e.center_dist_m as f64) / area;
                m_xx += w_perot * nx_dir * nx_dir;
                m_xy += w_perot * nx_dir * ny_dir;
                m_yy += w_perot * ny_dir * ny_dir;

                cell_edges.push(CellEdgeIncidence {
                    edge_idx,
                    neighbor: j_u32,
                    sign: sign as f32,
                    flux_factor: (sign * (e.dual_len_m as f64) / area) as f32,
                    perot_wx: (w_perot * nx_dir) as f32,
                    perot_wy: (w_perot * ny_dir) as f32,
                });
            }

            let det = m_xx * m_yy - m_xy * m_xy;
            if det.abs() > 1e-8 {
                let inv_xx = m_yy / det;
                let inv_xy = -m_xy / det;
                let inv_yy = m_xx / det;
                for inc in &mut cell_edges[start..] {
                    let raw_wx = inc.perot_wx as f64;
                    let raw_wy = inc.perot_wy as f64;
                    inc.perot_wx = (inv_xx * raw_wx + inv_xy * raw_wy) as f32;
                    inc.perot_wy = (inv_xy * raw_wx + inv_yy * raw_wy) as f32;
                }
            }

            cell_edge_offsets.push(cell_edges.len());
        }

        // Order cells by 30-bit 3D Morton code of cell centers.
        let mut keyed_cells: Vec<(u32, u32)> = (0..n)
            .map(|i| (morton3d_unit_sphere(cell_centers[i]), i as u32))
            .collect();
        keyed_cells.sort_unstable();
        let morton_cells: Vec<u32> = keyed_cells.into_iter().map(|(_, i)| i).collect();

        let mut cell_perot6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });
        let mut cell_div6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });
        let mut morton_perot6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });
        let mut morton_flux6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });
        let mut morton_div6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });
        let mut morton_lap6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });
        let mut morton_grad6 = Vec::with_capacity(if all_deg_le_6 { n } else { 0 });

        if all_deg_le_6 {
            for i in 0..n {
                let start = cell_edge_offsets[i];
                let end = cell_edge_offsets[i + 1];
                let slice = &cell_edges[start..end];
                let fallback_edge = slice.first().map_or(0, |inc| inc.edge_idx);
                let mut edge_idx = [fallback_edge; 6];
                let mut neighbor = [i as u32; 6];
                let mut flux_factor = [0.0_f32; 6];
                let mut perot_wx = [0.0_f32; 6];
                let mut perot_wy = [0.0_f32; 6];
                for (k, inc) in slice.iter().enumerate() {
                    edge_idx[k] = inc.edge_idx;
                    neighbor[k] = inc.neighbor;
                    flux_factor[k] = inc.flux_factor;
                    perot_wx[k] = inc.perot_wx;
                    perot_wy[k] = inc.perot_wy;
                }
                cell_perot6.push(CellPerotStencil6 {
                    edge_idx,
                    perot_wx,
                    perot_wy,
                });
                cell_div6.push(CellDivStencil6 {
                    edge_idx,
                    neighbor,
                    flux_factor,
                });
            }

            for &cell_u32 in &morton_cells {
                let i = cell_u32 as usize;
                let cp = cell_perot6[i];
                let cd = cell_div6[i];
                morton_perot6.push(MortonPerotStencil6 {
                    cell: cell_u32,
                    edge_idx: cp.edge_idx,
                    perot_wx: cp.perot_wx,
                    perot_wy: cp.perot_wy,
                });
                morton_flux6.push(cd.flux_factor);
                morton_div6.push(MortonDivStencil6 {
                    cell: cell_u32,
                    edge_idx: cd.edge_idx,
                    flux_factor: cd.flux_factor,
                });

                let start = cell_edge_offsets[i];
                let end = cell_edge_offsets[i + 1];
                let slice = &cell_edges[start..end];
                let mut lap_w = [0.0_f32; 6];
                let mut grad_wx = [0.0_f32; 6];
                let mut grad_wy = [0.0_f32; 6];
                for (k, inc) in slice.iter().enumerate() {
                    let inv_d = edges[inc.edge_idx as usize].inv_center_dist_m;
                    lap_w[k] = inc.flux_factor.abs() * inv_d;
                    let signed_inv_d = inc.sign * inv_d;
                    grad_wx[k] = signed_inv_d * inc.perot_wx;
                    grad_wy[k] = signed_inv_d * inc.perot_wy;
                }
                morton_lap6.push(MortonLapStencil6 {
                    cell: cell_u32,
                    neighbor: cd.neighbor,
                    weight: lap_w,
                });
                morton_grad6.push(MortonGradStencil6 {
                    cell: cell_u32,
                    neighbor: cd.neighbor,
                    wx: grad_wx,
                    wy: grad_wy,
                });
            }
        }

        Self {
            num_cells: n,
            radius_m,
            omega_rad_s,
            areas_m2,
            lats_rad,
            lons_rad,
            sin_lat,
            cos_lat,
            coriolis_f,
            east_xyz,
            north_xyz,
            cell_edge_offsets,
            cell_edges,
            edges,
            mean_edge_dist_m,
            all_deg_le_6,
            morton_cells,
            morton_perot6,
            morton_flux6,
            morton_div6,
            morton_lap6,
            morton_grad6,
            cell_perot6,
            cell_div6,
        }
    }

    /// Updates planetary rotation period `day_hours` and recomputes `coriolis_f`.
    pub fn set_rotation(&mut self, day_hours: f64) {
        self.omega_rad_s = if day_hours.is_finite() && day_hours.abs() > 1e-6 {
            2.0 * PI / (day_hours * 3600.0)
        } else {
            0.0
        };
        for i in 0..self.num_cells {
            self.coriolis_f[i] = (2.0 * self.omega_rad_s * (self.sin_lat[i] as f64)) as f32;
        }
    }

    /// Returns the CSR slice of incident edges for cell `cell`.
    #[inline]
    pub fn incident_edges(&self, cell: usize) -> &[CellEdgeIncidence] {
        let start = self.cell_edge_offsets[cell];
        let end = self.cell_edge_offsets[cell + 1];
        &self.cell_edges[start..end]
    }

    /// Reconstructs cell-centered `(u_east, v_north)` velocity vectors from normal edge velocities `u_edge`.
    pub fn reconstruct_cell_vectors(
        &self,
        u_edge: &[f32],
        out_u_east: &mut [f32],
        out_v_north: &mut [f32],
    ) {
        assert_eq!(u_edge.len(), self.edges.len());
        assert_eq!(out_u_east.len(), self.num_cells);
        assert_eq!(out_v_north.len(), self.num_cells);

        if self.all_deg_le_6 {
            for st in &self.morton_perot6 {
                let mut ux = 0.0_f32;
                let mut vy = 0.0_f32;
                for k in 0..6 {
                    let ue = u_edge[st.edge_idx[k] as usize];
                    ux += ue * st.perot_wx[k];
                    vy += ue * st.perot_wy[k];
                }
                let c = st.cell as usize;
                out_u_east[c] = ux;
                out_v_north[c] = vy;
            }
        } else {
            for i in 0..self.num_cells {
                let mut ux = 0.0_f32;
                let mut vy = 0.0_f32;
                for inc in self.incident_edges(i) {
                    let ue = u_edge[inc.edge_idx as usize];
                    ux += ue * inc.perot_wx;
                    vy += ue * inc.perot_wy;
                }
                out_u_east[i] = ux;
                out_v_north[i] = vy;
            }
        }
    }

    /// Fused 2-layer Perot velocity reconstruction AND normal-velocity divergence in a single pass.
    #[allow(clippy::too_many_arguments)]
    pub fn reconstruct_and_div_2layer(
        &self,
        u1_edge: &[f32],
        u2_edge: &[f32],
        out_u1_east: &mut [f32],
        out_v1_north: &mut [f32],
        out_div1: &mut [f32],
        out_u2_east: &mut [f32],
        out_v2_north: &mut [f32],
        out_div2: &mut [f32],
    ) {
        assert_eq!(u1_edge.len(), self.edges.len());
        assert_eq!(u2_edge.len(), self.edges.len());
        assert_eq!(out_u1_east.len(), self.num_cells);
        assert_eq!(out_v1_north.len(), self.num_cells);
        assert_eq!(out_div1.len(), self.num_cells);
        assert_eq!(out_u2_east.len(), self.num_cells);
        assert_eq!(out_v2_north.len(), self.num_cells);
        assert_eq!(out_div2.len(), self.num_cells);

        if self.all_deg_le_6 {
            for (st, ff) in self.morton_perot6.iter().zip(self.morton_flux6.iter()) {
                let mut u1x = 0.0_f32;
                let mut v1y = 0.0_f32;
                let mut d1 = 0.0_f32;
                let mut u2x = 0.0_f32;
                let mut v2y = 0.0_f32;
                let mut d2 = 0.0_f32;
                for k in 0..6 {
                    let e = st.edge_idx[k] as usize;
                    let ue1 = u1_edge[e];
                    let ue2 = u2_edge[e];
                    let wx = st.perot_wx[k];
                    let wy = st.perot_wy[k];
                    let f = ff[k];
                    u1x += ue1 * wx;
                    v1y += ue1 * wy;
                    d1 += ue1 * f;
                    u2x += ue2 * wx;
                    v2y += ue2 * wy;
                    d2 += ue2 * f;
                }
                let c = st.cell as usize;
                out_u1_east[c] = u1x;
                out_v1_north[c] = v1y;
                out_div1[c] = d1;
                out_u2_east[c] = u2x;
                out_v2_north[c] = v2y;
                out_div2[c] = d2;
            }
        } else {
            self.reconstruct_cell_vectors(u1_edge, out_u1_east, out_v1_north);
            self.reconstruct_cell_vectors(u2_edge, out_u2_east, out_v2_north);
            self.divergence_from_edge_flux(u1_edge, out_div1);
            self.divergence_from_edge_flux(u2_edge, out_div2);
        }
    }

    /// Computes conservative finite-volume divergence $(\nabla \cdot \mathbf{F})_i = \frac{1}{A_i}\sum_e s_{e,i} l_e F_e$,
    /// where `flux_edge[e]` is flux per unit edge length in direction `left -> right`.
    pub fn divergence_from_edge_flux(&self, flux_edge: &[f32], out_div: &mut [f32]) {
        assert_eq!(flux_edge.len(), self.edges.len());
        assert_eq!(out_div.len(), self.num_cells);

        if self.all_deg_le_6 {
            for st in &self.morton_div6 {
                let mut div = 0.0_f32;
                for k in 0..6 {
                    div += flux_edge[st.edge_idx[k] as usize] * st.flux_factor[k];
                }
                out_div[st.cell as usize] = div;
            }
        } else {
            for i in 0..self.num_cells {
                let mut div = 0.0_f32;
                for inc in self.incident_edges(i) {
                    div += flux_edge[inc.edge_idx as usize] * inc.flux_factor;
                }
                out_div[i] = div;
            }
        }
    }

    /// Fused 2-layer conservative finite-volume divergence in a single pass.
    pub fn divergence_2layer(
        &self,
        flux1_edge: &[f32],
        flux2_edge: &[f32],
        out_div1: &mut [f32],
        out_div2: &mut [f32],
    ) {
        assert_eq!(flux1_edge.len(), self.edges.len());
        assert_eq!(flux2_edge.len(), self.edges.len());
        assert_eq!(out_div1.len(), self.num_cells);
        assert_eq!(out_div2.len(), self.num_cells);

        if self.all_deg_le_6 {
            for st in &self.morton_div6 {
                let mut d1 = 0.0_f32;
                let mut d2 = 0.0_f32;
                for k in 0..6 {
                    let e = st.edge_idx[k] as usize;
                    let f = st.flux_factor[k];
                    d1 += flux1_edge[e] * f;
                    d2 += flux2_edge[e] * f;
                }
                let c = st.cell as usize;
                out_div1[c] = d1;
                out_div2[c] = d2;
            }
        } else {
            self.divergence_from_edge_flux(flux1_edge, out_div1);
            self.divergence_from_edge_flux(flux2_edge, out_div2);
        }
    }

    /// Computes conservative spherical scalar Laplacian $(\nabla^2 \psi)_i = \frac{1}{A_i}\sum_{e \in \partial i} l_e \frac{\psi_j - \psi_i}{d_e}$.
    pub fn laplacian_scalar(&self, field: &[f32], out_lap: &mut [f32]) {
        assert_eq!(field.len(), self.num_cells);
        assert_eq!(out_lap.len(), self.num_cells);

        if self.all_deg_le_6 {
            for st in &self.morton_lap6 {
                let c = st.cell as usize;
                let fi = field[c];
                let mut acc = 0.0_f32;
                for k in 0..6 {
                    acc += (field[st.neighbor[k] as usize] - fi) * st.weight[k];
                }
                out_lap[c] = acc;
            }
        } else {
            for i in 0..self.num_cells {
                let fi = field[i];
                let mut acc = 0.0_f32;
                for inc in self.incident_edges(i) {
                    let e = &self.edges[inc.edge_idx as usize];
                    let fj = field[inc.neighbor as usize];
                    acc += inc.flux_factor.abs() * (fj - fi) * e.inv_center_dist_m;
                }
                out_lap[i] = acc;
            }
        }
    }

    /// Fused 2-field conservative spherical scalar Laplacian in a single stencil pass.
    pub fn laplacian_2layer(
        &self,
        field1: &[f32],
        field2: &[f32],
        out_lap1: &mut [f32],
        out_lap2: &mut [f32],
    ) {
        assert_eq!(field1.len(), self.num_cells);
        assert_eq!(field2.len(), self.num_cells);
        assert_eq!(out_lap1.len(), self.num_cells);
        assert_eq!(out_lap2.len(), self.num_cells);

        if self.all_deg_le_6 {
            for st in &self.morton_lap6 {
                let c = st.cell as usize;
                let f1i = field1[c];
                let f2i = field2[c];
                let mut acc1 = 0.0_f32;
                let mut acc2 = 0.0_f32;
                for k in 0..6 {
                    let nb = st.neighbor[k] as usize;
                    let w = st.weight[k];
                    acc1 += (field1[nb] - f1i) * w;
                    acc2 += (field2[nb] - f2i) * w;
                }
                out_lap1[c] = acc1;
                out_lap2[c] = acc2;
            }
        } else {
            self.laplacian_scalar(field1, out_lap1);
            self.laplacian_scalar(field2, out_lap2);
        }
    }

    /// Computes cell-centered scalar gradient $(\partial_x \psi, \partial_y \psi)$ in `(east, north)`
    /// via edge finite differences reconstructed with `perot_wx, perot_wy`.
    pub fn gradient_scalar(
        &self,
        field: &[f32],
        out_gx: &mut [f32],
        out_gy: &mut [f32],
    ) {
        assert_eq!(field.len(), self.num_cells);
        assert_eq!(out_gx.len(), self.num_cells);
        assert_eq!(out_gy.len(), self.num_cells);

        if self.all_deg_le_6 {
            for st in &self.morton_grad6 {
                let c = st.cell as usize;
                let fi = field[c];
                let mut gx = 0.0_f32;
                let mut gy = 0.0_f32;
                for k in 0..6 {
                    let df = field[st.neighbor[k] as usize] - fi;
                    gx += df * st.wx[k];
                    gy += df * st.wy[k];
                }
                out_gx[c] = gx;
                out_gy[c] = gy;
            }
        } else {
            for i in 0..self.num_cells {
                let mut gx = 0.0_f32;
                let mut gy = 0.0_f32;
                for inc in self.incident_edges(i) {
                    let e = &self.edges[inc.edge_idx as usize];
                    let df_dn =
                        (field[e.right as usize] - field[e.left as usize]) * e.inv_center_dist_m;
                    gx += df_dn * inc.perot_wx;
                    gy += df_dn * inc.perot_wy;
                }
                out_gx[i] = gx;
                out_gy[i] = gy;
            }
        }
    }

    /// Computes conservative spherical relative vorticity $\zeta_i = (\hat{\mathbf{k}} \cdot \nabla \times \mathbf{u})_i$
    /// via Stokes' theorem $\zeta_i = \frac{1}{A_i}\sum_{e \in \partial i} s_{e,i} l_e (\mathbf{u}_e \cdot (\hat{\mathbf{k}} \times \mathbf{n}_e))$,
    /// automatically capturing spherical metric curvature $\frac{u\tan\phi}{R}$ and guaranteeing $\sum_i A_i \zeta_i \equiv 0$.
    pub fn curl_from_cell_vectors(
        &self,
        u_east: &[f32],
        v_north: &[f32],
        scratch_edge_tan: &mut [f32],
        out_zeta: &mut [f32],
    ) {
        assert_eq!(u_east.len(), self.num_cells);
        assert_eq!(v_north.len(), self.num_cells);
        assert_eq!(scratch_edge_tan.len(), self.edges.len());
        assert_eq!(out_zeta.len(), self.num_cells);

        for (e_idx, e) in self.edges.iter().enumerate() {
            let l = e.left as usize;
            let r = e.right as usize;
            let vt_l = -u_east[l] * e.ny_left + v_north[l] * e.nx_left;
            let vt_r = -u_east[r] * e.ny_right + v_north[r] * e.nx_right;
            scratch_edge_tan[e_idx] = 0.5 * (vt_l + vt_r);
        }
        self.divergence_from_edge_flux(scratch_edge_tan, out_zeta);
    }

    /// Fused 2-layer Stokes relative vorticity in a single edge pass + single Morton stencil pass.
    #[allow(clippy::too_many_arguments)]
    pub fn curl_2layer(
        &self,
        u1_east: &[f32],
        v1_north: &[f32],
        u2_east: &[f32],
        v2_north: &[f32],
        scratch_tan1: &mut [f32],
        scratch_tan2: &mut [f32],
        out_zeta1: &mut [f32],
        out_zeta2: &mut [f32],
    ) {
        assert_eq!(u1_east.len(), self.num_cells);
        assert_eq!(v1_north.len(), self.num_cells);
        assert_eq!(u2_east.len(), self.num_cells);
        assert_eq!(v2_north.len(), self.num_cells);
        assert_eq!(scratch_tan1.len(), self.edges.len());
        assert_eq!(scratch_tan2.len(), self.edges.len());

        for (e_idx, e) in self.edges.iter().enumerate() {
            let l = e.left as usize;
            let r = e.right as usize;
            let vt1_l = -u1_east[l] * e.ny_left + v1_north[l] * e.nx_left;
            let vt1_r = -u1_east[r] * e.ny_right + v1_north[r] * e.nx_right;
            scratch_tan1[e_idx] = 0.5 * (vt1_l + vt1_r);

            let vt2_l = -u2_east[l] * e.ny_left + v2_north[l] * e.nx_left;
            let vt2_r = -u2_east[r] * e.ny_right + v2_north[r] * e.nx_right;
            scratch_tan2[e_idx] = 0.5 * (vt2_l + vt2_r);
        }
        self.divergence_2layer(scratch_tan1, scratch_tan2, out_zeta1, out_zeta2);
    }

    /// Converts local `(u_east, v_north)` velocity at `cell` into 3D `SphericalGrid` `[vx, vy, vz]`.
    #[inline]
    pub fn local_uv_to_xyz(&self, cell: usize, u_east: f32, v_north: f32) -> [f32; 3] {
        let e = self.east_xyz[cell];
        let n = self.north_xyz[cell];
        [
            u_east * e[0] + v_north * n[0],
            u_east * e[1] + v_north * n[1],
            u_east * e[2] + v_north * n[2],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fv_sphere_area_gauss_divergence_and_perot_reconstruction() {
        let grid = SphericalGrid::with_size(5);
        let adj = grid.adjacency();
        let radius_m = 6_371_000.0_f64;
        let geom = FiniteVolumeSphereGeom::new(&grid, &adj, radius_m, 24.0);
        assert!(geom.all_deg_le_6);

        // 1. Total spherical area matches 4 * pi * R^2 to high precision.
        let total_area: f64 = geom.areas_m2.iter().map(|&a| a as f64).sum();
        let exact_area = 4.0 * PI * radius_m * radius_m;
        let rel_area_err = (total_area - exact_area).abs() / exact_area;
        assert!(
            rel_area_err < 1e-5,
            "expected exact spherical area sum, got rel_err={rel_area_err}"
        );

        // 2. Global discrete Gauss divergence theorem: sum_i A_i * div_i == 0.
        let flux_edge: Vec<f32> = (0..geom.edges.len())
            .map(|k| ((k as f32 * 0.37).sin() * 120.0) + ((k as f32 * 0.11).cos() * 45.0))
            .collect();
        let mut div = vec![0.0_f32; geom.num_cells];
        geom.divergence_from_edge_flux(&flux_edge, &mut div);

        let global_integral: f64 = div
            .iter()
            .zip(geom.areas_m2.iter())
            .map(|(&d, &a)| (d as f64) * (a as f64))
            .sum();
        let flux_scale: f64 = flux_edge
            .iter()
            .zip(geom.edges.iter())
            .map(|(&f, e)| (f.abs() as f64) * (e.dual_len_m as f64))
            .sum();
        assert!(
            global_integral.abs() / flux_scale < 1e-6,
            "discrete divergence failed conservation: integral={global_integral}, scale={flux_scale}"
        );

        // 3. Perot vector reconstruction recovers solid-body zonal rotation U0 * cos(lat)
        // on both hexagons and pentagons.
        let u0 = 25.0_f64;
        let mut u_edge = vec![0.0_f32; geom.edges.len()];
        for (idx, e) in geom.edges.iter().enumerate() {
            let l = e.left as usize;
            let r = e.right as usize;
            let ul_east = (u0 * geom.cos_lat[l] as f64) as f32;
            let ur_east = (u0 * geom.cos_lat[r] as f64) as f32;
            u_edge[idx] = 0.5 * (ul_east * e.nx_left + ur_east * e.nx_right);
        }

        let mut rec_u = vec![0.0_f32; geom.num_cells];
        let mut rec_v = vec![0.0_f32; geom.num_cells];
        geom.reconstruct_cell_vectors(&u_edge, &mut rec_u, &mut rec_v);

        let mut max_err = 0.0_f32;
        for i in 0..geom.num_cells {
            let expected_u = (u0 as f32) * geom.cos_lat[i];
            let err_u = (rec_u[i] - expected_u).abs();
            let err_v = rec_v[i].abs();
            max_err = max_err.max(err_u).max(err_v);
        }
        assert!(
            max_err < 0.25,
            "expected accurate solid-body zonal reconstruction, got max_err={max_err} m/s"
        );

        // 4. Stokes relative vorticity recovers exact planetary curl (2 * U0 / R) * sin(lat)
        // for solid-body rotation and sums to zero globally.
        let mut scratch_tan = vec![0.0_f32; geom.edges.len()];
        let mut zeta = vec![0.0_f32; geom.num_cells];
        geom.curl_from_cell_vectors(&rec_u, &rec_v, &mut scratch_tan, &mut zeta);

        let zeta_global: f64 = zeta
            .iter()
            .zip(geom.areas_m2.iter())
            .map(|(&z, &a)| (z as f64) * (a as f64))
            .sum();
        let zeta_scale: f64 = zeta
            .iter()
            .zip(geom.areas_m2.iter())
            .map(|(&z, &a)| (z.abs() as f64) * (a as f64))
            .sum();
        assert!(
            zeta_global.abs() / zeta_scale < 1e-6,
            "expected zero global vorticity integral, got integral={zeta_global}, scale={zeta_scale}"
        );

        let mut max_zeta_err = 0.0_f32;
        for i in 0..geom.num_cells {
            let expected_z = ((2.0 * u0 / radius_m) as f32) * geom.sin_lat[i];
            max_zeta_err = max_zeta_err.max((zeta[i] - expected_z).abs());
        }
        assert!(
            max_zeta_err < 1.5e-6,
            "expected accurate solid-body curl, got max_zeta_err={max_zeta_err}"
        );

        // 5. Fused 2-layer operators match single-layer operators and constant fields give exact 0.
        let u2_edge: Vec<f32> = u_edge.iter().enumerate().map(|(k, &v)| -0.6 * v + (k as f32 * 0.13).sin()).collect();
        let mut rec_u2_ref = vec![0.0_f32; geom.num_cells];
        let mut rec_v2_ref = vec![0.0_f32; geom.num_cells];
        let mut div1_ref = vec![0.0_f32; geom.num_cells];
        let mut div2_ref = vec![0.0_f32; geom.num_cells];
        geom.reconstruct_cell_vectors(&u2_edge, &mut rec_u2_ref, &mut rec_v2_ref);
        geom.divergence_from_edge_flux(&u_edge, &mut div1_ref);
        geom.divergence_from_edge_flux(&u2_edge, &mut div2_ref);

        let mut fu1 = vec![0.0_f32; geom.num_cells];
        let mut fv1 = vec![0.0_f32; geom.num_cells];
        let mut fd1 = vec![0.0_f32; geom.num_cells];
        let mut fu2 = vec![0.0_f32; geom.num_cells];
        let mut fv2 = vec![0.0_f32; geom.num_cells];
        let mut fd2 = vec![0.0_f32; geom.num_cells];
        geom.reconstruct_and_div_2layer(
            &u_edge, &u2_edge, &mut fu1, &mut fv1, &mut fd1, &mut fu2, &mut fv2, &mut fd2,
        );
        for i in 0..geom.num_cells {
            assert!((fu1[i] - rec_u[i]).abs() <= 1e-6);
            assert!((fv1[i] - rec_v[i]).abs() <= 1e-6);
            assert!((fd1[i] - div1_ref[i]).abs() <= 1e-9);
            assert!((fu2[i] - rec_u2_ref[i]).abs() <= 1e-6);
            assert!((fv2[i] - rec_v2_ref[i]).abs() <= 1e-6);
            assert!((fd2[i] - div2_ref[i]).abs() <= 1e-9);
        }

        let mut zeta2_ref = vec![0.0_f32; geom.num_cells];
        geom.curl_from_cell_vectors(&rec_u2_ref, &rec_v2_ref, &mut scratch_tan, &mut zeta2_ref);
        let mut scratch_tan2 = vec![0.0_f32; geom.edges.len()];
        let mut fz1 = vec![0.0_f32; geom.num_cells];
        let mut fz2 = vec![0.0_f32; geom.num_cells];
        geom.curl_2layer(
            &rec_u,
            &rec_v,
            &rec_u2_ref,
            &rec_v2_ref,
            &mut scratch_tan,
            &mut scratch_tan2,
            &mut fz1,
            &mut fz2,
        );
        for i in 0..geom.num_cells {
            assert!((fz1[i] - zeta[i]).abs() <= 1e-9);
            assert!((fz2[i] - zeta2_ref[i]).abs() <= 1e-9);
        }

        let const_field = vec![288.15_f32; geom.num_cells];
        let mut lap_const = vec![1.0_f32; geom.num_cells];
        let mut gx_const = vec![1.0_f32; geom.num_cells];
        let mut gy_const = vec![1.0_f32; geom.num_cells];
        geom.laplacian_scalar(&const_field, &mut lap_const);
        geom.gradient_scalar(&const_field, &mut gx_const, &mut gy_const);
        assert!(lap_const.iter().all(|&v| v == 0.0));
        assert!(gx_const.iter().all(|&v| v == 0.0));
        assert!(gy_const.iter().all(|&v| v == 0.0));
    }
}
