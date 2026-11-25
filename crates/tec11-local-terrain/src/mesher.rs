//! Face-culled hexagonal-prism mesher (`LocalTerrainMesher`) for both Level 0–2
//! progressive `DrillDownPatch` columns and Level 3 Z-sliced `LocalTerrainVolume` cutaways.

use std::ops::Range;

use crate::chunk::{axial_to_xy_m, LocalTerrainVolume, HEX_DIRS, SQRT_3};
use crate::materials::{cell_flags, HexPrismCell, MaterialId};
use crate::DrillDownPatch;

/// Flat triangle vertex buffers ready for zero-copy Three.js `BufferGeometry` upload.
#[derive(Clone, Debug, Default)]
pub struct HexMeshBuffers {
    /// 3 floats per vertex `(x, y_up, z)` in Three.js Y-up coordinates (meters, centered near origin).
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub colors: Vec<f32>,
}

impl HexMeshBuffers {
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }
}

pub struct LocalTerrainMesher;

/// Pointy-top hex 6 corner unit offsets around `(0, 0)` for center-to-center spacing `s = 1.0`.
/// Circumradius `R = s / sqrt(3)`.
fn hex_corners(cx: f32, cz: f32, hex_spacing_m: f32) -> [[f32; 2]; 6] {
    let r = hex_spacing_m / (SQRT_3 as f32);
    // Pointy-top angles: 30, 90, 150, 210, 270, 330 deg (ordered CCW in XZ plane)
    let mut out = [[0.0f32; 2]; 6];
    for i in 0..6 {
        let ang = ((30.0 + 60.0 * (i as f32)) as f32).to_radians();
        out[i] = [cx + r * ang.cos(), cz + r * ang.sin()];
    }
    out
}

/// Map each of the 6 hex edges `i -> (i+1)%6` to the corresponding axial neighbor offset in `HEX_DIRS`.
/// Verified for pointy-top axial coordinates `(x = s*(q + 0.5*r), z = s*(sqrt(3)/2)*r)`:
/// - Edge 0 (30°→90°, midpoint 60°): `(0, +1)` -> `HEX_DIRS[5]`
/// - Edge 1 (90°→150°, midpoint 120°): `(-1, +1)` -> `HEX_DIRS[4]`
/// - Edge 2 (150°→210°, midpoint 180°): `(-1, 0)` -> `HEX_DIRS[3]`
/// - Edge 3 (210°→270°, midpoint 240°): `(0, -1)` -> `HEX_DIRS[2]`
/// - Edge 4 (270°→330°, midpoint 300°): `(+1, -1)` -> `HEX_DIRS[1]`
/// - Edge 5 (330°→30°, midpoint 0°): `(+1, 0)` -> `HEX_DIRS[0]`
const EDGE_TO_DIR: [usize; 6] = [5, 4, 3, 2, 1, 0];

impl LocalTerrainMesher {
    /// Build a 3D hex-column mesh for a Level 0–2 `DrillDownPatch`.
    /// Vertical relief is centered around the patch's mean elevation and lightly exaggerated at
    /// coarse scales so ridges, valleys, and river trenches read clearly in 3D orbit.
    pub fn build_patch_mesh(patch: &DrillDownPatch) -> HexMeshBuffers {
        let mut buf = HexMeshBuffers::default();
        if patch.columns.is_empty() {
            return buf;
        }

        let mut min_elev = f32::INFINITY;
        let mut max_elev = f32::NEG_INFINITY;
        for c in &patch.columns {
            let top = c.water_elev_m.unwrap_or(c.surface_elev_m).max(c.surface_elev_m);
            min_elev = min_elev.min(c.surface_elev_m);
            max_elev = max_elev.max(top);
        }
        let base_y = min_elev - patch.hex_spacing_m * 1.5;
        let span_xy = ((patch.radius_hex * 2 + 1) as f32) * patch.hex_spacing_m;
        let span_z = (max_elev - min_elev).max(1.0);
        // Gentle vertical exaggeration at km scales so relief is legible without looking spiky
        let v_exag = (span_xy * 0.14 / span_z).clamp(1.0, 12.0);
        // Normalize coordinates into a clean ~[-30, +30] world-unit viewing box
        let scale = 40.0 / span_xy.max(1.0);

        let est_verts = patch.columns.len() * (6 * 3 + 6 * 6);
        buf.positions.reserve(est_verts * 3);
        buf.normals.reserve(est_verts * 3);
        buf.colors.reserve(est_verts * 3);

        for col in &patch.columns {
            let rel_xy = axial_to_xy_m(col.q, col.r, patch.hex_spacing_m);
            let cx = (rel_xy[0] as f32) * scale;
            let cz = (rel_xy[1] as f32) * scale;
            let spacing = patch.hex_spacing_m * scale * 0.995;

            let top_elev = col.water_elev_m.unwrap_or(col.surface_elev_m).max(col.surface_elev_m);
            let y_top = (top_elev - base_y) * v_exag * scale;
            let y_bot = 0.0f32;

            let mat = if col.water_elev_m.is_some() {
                if col.is_river || col.is_lake {
                    MaterialId::WATER
                } else {
                    MaterialId::OCEAN_WATER
                }
            } else {
                col.surface_material
            };
            let mut rgb = mat.rgb();
            // Subtle elevation shading cue
            let shade = (0.85 + 0.25 * ((top_elev - min_elev) / span_z)).clamp(0.75, 1.15);
            rgb = [
                (rgb[0] * shade).min(1.0),
                (rgb[1] * shade).min(1.0),
                (rgb[2] * shade).min(1.0),
            ];
            let side_rgb = [rgb[0] * 0.72, rgb[1] * 0.72, rgb[2] * 0.72];

            let corners = hex_corners(cx, cz, spacing);
            emit_hex_cap(&mut buf, cx, cz, y_top, &corners, [0.0, 1.0, 0.0], rgb, true);
            for edge in 0..6 {
                let c0 = corners[edge];
                let c1 = corners[(edge + 1) % 6];
                emit_hex_side(&mut buf, c0, c1, y_bot, y_top, side_rgb);
            }
        }

        buf
    }

    /// Build a face-culled 3D cutaway hex-prism mesh for Level 3 chunks clipped at `z <= z_max`.
    /// Interior faces between two opaque cells are skipped; exposed faces on the chunk cutaway
    /// borders and the `z == z_max` sliced top cap are always emitted cleanly (`SC-003`).
    pub fn build_volume_mesh(
        volume: &mut LocalTerrainVolume,
        q_range: Range<i32>,
        r_range: Range<i32>,
        z_min: i32,
        z_max: i32,
    ) -> HexMeshBuffers {
        let mut buf = HexMeshBuffers::default();
        if q_range.is_empty() || r_range.is_empty() || z_min > z_max {
            return buf;
        }

        let spacing = volume.config().hex_spacing_m;
        let z_step = volume.config().z_step_m;
        let q_len = (q_range.end - q_range.start) as usize;
        let r_len = (r_range.end - r_range.start) as usize;
        let z_len = (z_max - z_min + 1) as usize;

        // Materialize the visible window into a flat scratch slice for O(1) neighbor lookups
        // (128x128x64 = 1M cells = 4 MB transient scratch, well under 16 MB).
        let mut grid = vec![HexPrismCell::AIR; q_len * r_len * z_len];
        let idx3 = |iq: usize, ir: usize, iz: usize| (iq * r_len + ir) * z_len + iz;

        for (iq, q) in q_range.clone().enumerate() {
            for (ir, r) in r_range.clone().enumerate() {
                for (iz, z) in (z_min..=z_max).enumerate() {
                    grid[idx3(iq, ir, iz)] = volume.get_cell(q, r, z);
                }
            }
        }

        let center_q = (q_range.start + q_range.end) / 2;
        let center_r = (r_range.start + r_range.end) / 2;
        let center_xy = axial_to_xy_m(center_q, center_r, spacing);

        for (iq, q) in q_range.clone().enumerate() {
            for (ir, r) in r_range.clone().enumerate() {
                let xy = axial_to_xy_m(q, r, spacing);
                let cx = (xy[0] - center_xy[0]) as f32;
                let cz = (xy[1] - center_xy[1]) as f32;
                let corners = hex_corners(cx, cz, spacing);

                for iz in 0..z_len {
                    let cell = grid[idx3(iq, ir, iz)];
                    if cell.is_air() {
                        continue;
                    }

                    let y_bot = (iz as f32) * z_step;
                    let above_is_air = iz + 1 >= z_len || grid[idx3(iq, ir, iz + 1)].is_air();
                    let y_top = if cell.material.is_fluid() && above_is_air {
                        // Recess exposed water surfaces slightly so sea/river level reads cleanly
                        // below even a low 0–1 m shoreline bank.
                        y_bot + z_step * 0.78
                    } else {
                        y_bot + z_step
                    };
                    let mut rgb = cell.material.rgb();

                    // Slight brightness lift for flora-covered surface topsoil or ore veins
                    if (cell.flags & cell_flags::FLORA_TREE) != 0 {
                        rgb = [rgb[0] * 0.72, (rgb[1] * 1.18).min(1.0), rgb[2] * 0.72];
                    } else if (cell.flags & cell_flags::FLORA_GRASS) != 0 {
                        rgb = [rgb[0] * 0.88, (rgb[1] * 1.12).min(1.0), rgb[2] * 0.85];
                    }

                    // Top face: emit if at z_max slice cap OR cell above is transparent (air/water)
                    let show_top = if iz + 1 >= z_len {
                        true
                    } else {
                        let above = grid[idx3(iq, ir, iz + 1)];
                        above.is_air() || (above.material.is_fluid() && !cell.material.is_fluid())
                    };
                    if show_top {
                        // Highlight sliced cutaway cap subtly so underground layers pop clearly
                        let top_rgb = if iz + 1 >= z_len && (cell.flags & cell_flags::EXPOSED_SURFACE) == 0 {
                            [
                                (rgb[0] * 1.08).min(1.0),
                                (rgb[1] * 1.08).min(1.0),
                                (rgb[2] * 1.08).min(1.0),
                            ]
                        } else {
                            rgb
                        };
                        emit_hex_cap(&mut buf, cx, cz, y_top, &corners, [0.0, 1.0, 0.0], top_rgb, true);
                    }

                    // 6 side faces: emit if at volume boundary OR neighbor cell is transparent
                    let side_rgb = [rgb[0] * 0.80, rgb[1] * 0.80, rgb[2] * 0.80];
                    for edge in 0..6 {
                        let (dq, dr) = HEX_DIRS[EDGE_TO_DIR[edge]];
                        let nq = (iq as i32) + dq;
                        let nr = (ir as i32) + dr;
                        let show_side = if nq < 0 || nq >= (q_len as i32) || nr < 0 || nr >= (r_len as i32) {
                            true
                        } else {
                            let nb = grid[idx3(nq as usize, nr as usize, iz)];
                            nb.is_air() || (nb.material.is_fluid() && !cell.material.is_fluid())
                        };
                        if show_side {
                            let c0 = corners[edge];
                            let c1 = corners[(edge + 1) % 6];
                            emit_hex_side(&mut buf, c0, c1, y_bot, y_top, side_rgb);
                        }
                    }
                }
            }
        }

        buf
    }
}

fn emit_hex_cap(
    buf: &mut HexMeshBuffers,
    cx: f32,
    cz: f32,
    y: f32,
    corners: &[[f32; 2]; 6],
    normal: [f32; 3],
    rgb: [f32; 3],
    up: bool,
) {
    for i in 0..6 {
        let a = corners[i];
        let b = corners[(i + 1) % 6];
        let verts = if up {
            [[cx, y, cz], [b[0], y, b[1]], [a[0], y, a[1]]]
        } else {
            [[cx, y, cz], [a[0], y, a[1]], [b[0], y, b[1]]]
        };
        for v in verts {
            buf.positions.extend_from_slice(&v);
            buf.normals.extend_from_slice(&normal);
            buf.colors.extend_from_slice(&rgb);
        }
    }
}

fn emit_hex_side(
    buf: &mut HexMeshBuffers,
    c0: [f32; 2],
    c1: [f32; 2],
    y_bot: f32,
    y_top: f32,
    rgb: [f32; 3],
) {
    // Outward normal in XZ plane for CCW edge c0 -> c1
    let ex = c1[0] - c0[0];
    let ez = c1[1] - c0[1];
    let len = (ex * ex + ez * ez).sqrt().max(1e-6);
    let nx = ez / len;
    let nz = -ex / len;
    let normal = [nx, 0.0, nz];

    let v0 = [c0[0], y_bot, c0[1]];
    let v1 = [c1[0], y_bot, c1[1]];
    let v2 = [c1[0], y_top, c1[1]];
    let v3 = [c0[0], y_top, c0[1]];

    for v in [v0, v1, v2, v0, v2, v3] {
        buf.positions.extend_from_slice(&v);
        buf.normals.extend_from_slice(&normal);
        buf.colors.extend_from_slice(&rgb);
    }
}
