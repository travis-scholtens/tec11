use std::collections::HashMap;
use std::f64::consts::PI;

use tec11_math_grid::{SphericalGrid, Vec3, Vec3Key};

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
    pub edges: Vec<DirectedDualEdge>,
    /// Mean center-to-center distance $\bar{d}_e$ (m), useful for CFL & hyperviscosity scaling.
    pub mean_edge_dist_m: f32,
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

        for &(center, ref verts) in &grid.faces {
            let p = norm3(center);
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

        // Build unique undirected edges (left < right) and map shared vertices.
        let mut edges = Vec::with_capacity(n * 3);
        let mut edge_lookup: HashMap<(u32, u32), u32> = HashMap::with_capacity(n * 3);
        let mut dist_sum = 0.0_f64;

        for i in 0..n {
            let (pi_raw, verts_i) = &grid.faces[i];
            let pi = norm3(*pi_raw);
            for &j in &adjacency[i] {
                if j <= i {
                    continue;
                }
                let (pj_raw, verts_j) = &grid.faces[j];
                let pj = norm3(*pj_raw);

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
                    let mid = norm3([pi[0] + pj[0], pi[1] + pj[1], pi[2] + pj[2]]);
                    let dp = [pj[0] - pi[0], pj[1] - pi[1], pj[2] - pi[2]];
                    norm3([
                        dp[0] - dot3(dp, mid) * mid[0],
                        dp[1] - dot3(dp, mid) * mid[1],
                        dp[2] - dot3(dp, mid) * mid[2],
                    ])
                };

                let (east_i, north_i) = bases_f64[i];
                let (east_j, north_j) = bases_f64[j];
                let (nx_left, ny_left) = project_unit_2d(n3d, east_i, north_i);
                let (nx_right, ny_right) = project_unit_2d(n3d, east_j, north_j);

                let edge_idx = edges.len() as u32;
                edges.push(DirectedDualEdge {
                    left: i as u32,
                    right: j as u32,
                    dual_len_m: dual_len_m as f32,
                    center_dist_m: center_dist_m as f32,
                    inv_center_dist_m: (1.0 / center_dist_m) as f32,
                    nx_left: nx_left as f32,
                    ny_left: ny_left as f32,
                    nx_right: nx_right as f32,
                    ny_right: ny_right as f32,
                });
                edge_lookup.insert((i as u32, j as u32), edge_idx);
            }
        }

        let mean_edge_dist_m = if edges.is_empty() {
            1.0
        } else {
            (dist_sum / edges.len() as f64) as f32
        };

        // Build CSR cell->edge incidence with Perot weights refined by the exact 2x2 cell metric
        // tensor so uniform tangent vector fields reconstruct to machine precision on every
        // hexagon AND pentagon cell.
        let mut cell_edge_offsets = Vec::with_capacity(n + 1);
        let mut cell_edges = Vec::with_capacity(edges.len() * 2);
        cell_edge_offsets.push(0);

        for i in 0..n {
            let area = areas_m2[i] as f64;
            let start = cell_edges.len();

            let mut m_xx = 0.0_f64;
            let mut m_xy = 0.0_f64;
            let mut m_yy = 0.0_f64;

            for &j in &adjacency[i] {
                let key = if i < j {
                    (i as u32, j as u32)
                } else {
                    (j as u32, i as u32)
                };
                let Some(&edge_idx) = edge_lookup.get(&key) else {
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
                    neighbor: j as u32,
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
        debug_assert_eq!(u_edge.len(), self.edges.len());
        debug_assert_eq!(out_u_east.len(), self.num_cells);
        debug_assert_eq!(out_v_north.len(), self.num_cells);

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

    /// Computes conservative finite-volume divergence $(\nabla \cdot \mathbf{F})_i = \frac{1}{A_i}\sum_e s_{e,i} l_e F_e$,
    /// where `flux_edge[e]` is flux per unit edge length in direction `left -> right`.
    pub fn divergence_from_edge_flux(&self, flux_edge: &[f32], out_div: &mut [f32]) {
        debug_assert_eq!(flux_edge.len(), self.edges.len());
        debug_assert_eq!(out_div.len(), self.num_cells);

        for i in 0..self.num_cells {
            let mut div = 0.0_f32;
            for inc in self.incident_edges(i) {
                div += flux_edge[inc.edge_idx as usize] * inc.flux_factor;
            }
            out_div[i] = div;
        }
    }

    /// Computes conservative spherical scalar Laplacian $(\nabla^2 \psi)_i = \frac{1}{A_i}\sum_{e \in \partial i} l_e \frac{\psi_j - \psi_i}{d_e}$.
    pub fn laplacian_scalar(&self, field: &[f32], out_lap: &mut [f32]) {
        debug_assert_eq!(field.len(), self.num_cells);
        debug_assert_eq!(out_lap.len(), self.num_cells);

        for i in 0..self.num_cells {
            let fi = field[i];
            let mut acc = 0.0_f32;
            for inc in self.incident_edges(i) {
                let e = &self.edges[inc.edge_idx as usize];
                let fj = field[inc.neighbor as usize];
                // Note `inc.flux_factor.abs() == dual_len_m / area_m2`.
                acc += inc.flux_factor.abs() * (fj - fi) * e.inv_center_dist_m;
            }
            out_lap[i] = acc;
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
        debug_assert_eq!(field.len(), self.num_cells);
        debug_assert_eq!(out_gx.len(), self.num_cells);
        debug_assert_eq!(out_gy.len(), self.num_cells);

        for i in 0..self.num_cells {
            let mut gx = 0.0_f32;
            let mut gy = 0.0_f32;
            for inc in self.incident_edges(i) {
                let e = &self.edges[inc.edge_idx as usize];
                let df_dn = (field[e.right as usize] - field[e.left as usize]) * e.inv_center_dist_m;
                gx += df_dn * inc.perot_wx;
                gy += df_dn * inc.perot_wy;
            }
            out_gx[i] = gx;
            out_gy[i] = gy;
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
        debug_assert_eq!(u_east.len(), self.num_cells);
        debug_assert_eq!(v_north.len(), self.num_cells);
        debug_assert_eq!(scratch_edge_tan.len(), self.edges.len());
        debug_assert_eq!(out_zeta.len(), self.num_cells);

        for (e_idx, e) in self.edges.iter().enumerate() {
            let l = e.left as usize;
            let r = e.right as usize;
            let vt_l = -u_east[l] * e.ny_left + v_north[l] * e.nx_left;
            let vt_r = -u_east[r] * e.ny_right + v_north[r] * e.nx_right;
            scratch_edge_tan[e_idx] = 0.5 * (vt_l + vt_r);
        }
        self.divergence_from_edge_flux(scratch_edge_tan, out_zeta);
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
        // Solid-body eastward velocity in 3D: v(p) = U0 * [y, -x, 0] (since r_xy = sqrt(x^2+y^2)
        // and east = [y/r_xy, -x/r_xy, 0], so east * U0 * cos(lat) = U0 * [y, -x, 0]).
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
    }
}
