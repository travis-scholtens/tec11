//! Hydrology-First Vector Drainage Network (`DrainageNetwork`, `DrainageReach`, `LakeBasin`):
//! - Anchor-invariant Chaikin $C^1$ mainstem subdivision (grounded on History fine rivers or coarse rivers)
//! - Strictly non-increasing $Z_{\text{water}}$ downstream (enabling rivers to incise deep canyons through high tiles)
//! - Deterministic multi-order tributaries snapping strictly downhill to parent $Z_{\text{water}}$ at confluences
//! - Leopold-Maddock hydraulic geometry (10 m creeks up to 2,500+ m braided mainstems & coastal estuaries)
//! - Climate-, evaporation-, permeability-, and freezing-gated structural `LakeBasin`s (`FR-005`, `SC-005`).

use crate::anchor::{CoarseRiverEdge, LocalZoomAnchor};
use crate::stratigraphy::{fbm2, value_noise2, RegionalStratigraphyField};

/// Physical state of a structural [`LakeBasin`], derived from climate, evaporation,
/// bedrock permeability (karst/porous vs. impermeable shield/clay), and temperature.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LakeState {
    Freshwater,
    Frozen,
    DryPlaya,
    SaltFlat,
}

/// Structural lake basin node inside [`DrainageNetwork`].
#[derive(Clone, Debug)]
pub struct LakeBasin {
    pub center_xy_m: [f64; 2],
    pub radius_x_m: f32,
    pub radius_y_m: f32,
    pub cos_a: f32,
    pub sin_a: f32,
    pub z_water_m: f32,
    pub max_depth_m: f32,
    pub state: LakeState,
}

/// Anchor-invariant subdivided mainstem (`order == 0`) or tributary (`order >= 1`) reach.
#[derive(Clone, Debug)]
pub struct DrainageReach {
    pub p0_m: [f64; 2],
    pub p1_m: [f64; 2],
    /// Monotonicity invariant: `z_water_1_m <= z_water_0_m`.
    pub z_water_0_m: f32,
    pub z_water_1_m: f32,
    pub order: u8,
    pub channel_width_m: f32,
    /// Smoothly matched channel widths at `p0_m` and `p1_m` so consecutive segments never create
    /// overhanging end-caps (e.g., a wide flat estuary segment swallowing an upstream steep bank).
    pub channel_width_0_m: f32,
    pub channel_width_1_m: f32,
    pub channel_depth_m: f32,
    pub valley_half_width_m: f32,
    /// Thalweg asymmetry in `[-0.55, +0.55]` derived from bend curvature (outer cut-bank vs inner point-bar).
    pub bend_bias: f32,
    pub discharge: f32,
}

/// Backwards-compatible type alias for `DrainageReach`.
pub type RiverSplineSegment = DrainageReach;

/// Result of querying [`DrainageNetwork`] at tangent-plane point `(x_m, y_m)`.
#[derive(Copy, Clone, Debug)]
pub struct RiverQueryResult {
    /// Distance from `(x_m, y_m)` to the nearest river centerline (meters).
    pub dist_m: f32,
    /// Distance from the active channel bank (`0.0` inside channel, `> 0.0` on valley wall).
    pub bank_dist_m: f32,
    /// Normalized valley wall coordinate `u` in `[0, 1]` (`0.0` at channel bank, `1.0` at valley rim).
    pub valley_u: f32,
    /// Water-surface elevation $Z_{\text{water}}$ at the projected point on the centerline.
    pub z_water_m: f32,
    /// Carved riverbed elevation (strictly `< z_water_m` inside open-water channel).
    pub bed_elev_m: f32,
    /// Valley envelope weight in `[0, 1]` (`1.0` inside channel, tapering smoothly to `0.0` at rim).
    pub valley_blend: f32,
    /// True if `(x_m, y_m)` lies inside the active wetted channel (`dist_m <= eff_half_w`).
    pub in_channel: bool,
    /// Height above `z_water_m` of an emergent braided mid-channel sandbar/island (`0.0` if open water).
    pub bar_height_m: f32,
    /// Discharge of the influencing reach.
    pub discharge: f32,
}

/// Result of querying structural [`LakeBasin`]s at `(x_m, y_m)`.
#[derive(Copy, Clone, Debug)]
pub struct LakeQueryResult {
    pub z_water_m: f32,
    pub floor_elev_m: f32,
    pub rim_blend: f32,
    pub in_basin: bool,
    pub state: LakeState,
}

/// Continuous, anchor-invariant hydrology-first vector drainage network.
#[derive(Clone, Debug, Default)]
pub struct DrainageNetwork {
    pub reaches: Vec<DrainageReach>,
    pub lakes: Vec<LakeBasin>,
}

impl DrainageNetwork {
    /// Build the complete drainage network (mainstems, multi-order tributaries, and climate/rock-gated
    /// structural lake basins) for `anchor`.
    pub fn build(anchor: &LocalZoomAnchor, strat: &RegionalStratigraphyField) -> Self {
        if !anchor.has_atmosphere {
            return Self::default();
        }

        let sea_m = anchor.sea_level_m as f32;
        let sample_ground = |x: f64, y: f64| -> f32 {
            let ctx = strat.sample_context(anchor, x, y);
            anchor.sample_history_fine_ground(x, y, ctx.base_elevation_m)
        };
        let sample_tough = |x: f64, y: f64| -> f32 {
            let ctx = strat.sample_context(anchor, x, y);
            strat
                .evaluate_column(&ctx, x, y, ctx.base_elevation_m)
                .first()
                .map(|s| s.toughness)
                .unwrap_or(0.5)
        };

        // Solve a single shared, strictly non-increasing `z_water` at every river graph node
        // across all mainstem edges so converging branches at a confluence always meet at the
        // exact same water elevation (no phantom waterfall steps or inverted-flow mismatches).
        let quant = |xy: [f64; 2]| -> (i64, i64) { (xy[0].round() as i64, xy[1].round() as i64) };
        let mut node_map: std::collections::BTreeMap<(i64, i64), usize> =
            std::collections::BTreeMap::new();
        let mut node_xy: Vec<[f64; 2]> = Vec::new();
        let mut node_elev: Vec<f32> = Vec::new();
        let mut node_max_q: Vec<f32> = Vec::new();
        let mut edge_nodes: Vec<(usize, usize)> = Vec::with_capacity(anchor.river_edges.len());

        let mut intern_node = |xy: [f64; 2], elev_m: f32, q: f32| -> usize {
            let key = quant(xy);
            if let Some(&id) = node_map.get(&key) {
                node_elev[id] = node_elev[id].min(elev_m);
                node_max_q[id] = node_max_q[id].max(q);
                id
            } else {
                let id = node_xy.len();
                node_map.insert(key, id);
                node_xy.push(xy);
                node_elev.push(elev_m);
                node_max_q.push(q);
                id
            }
        };

        for edge in &anchor.river_edges {
            let u = intern_node(edge.from_xy_m, edge.from_elev_m, edge.discharge);
            let v = intern_node(edge.to_xy_m, edge.to_elev_m, edge.discharge);
            edge_nodes.push((u, v));
        }

        let mut z_node: Vec<f32> = (0..node_xy.len())
            .map(|i| {
                if node_elev[i] <= sea_m + 0.05 {
                    sea_m
                } else {
                    let incision = (14.0 + 4.5 * node_max_q[i].sqrt()).clamp(14.0, 36.0);
                    let base_e = sample_ground(node_xy[i][0], node_xy[i][1]);
                    (base_e.min(node_elev[i]) - incision).max(sea_m)
                }
            })
            .collect();

        // Propagate downstream monotonicity (`z_node[v] <= z_node[u]`) across the full graph.
        for _ in 0..edge_nodes.len().clamp(1, 64) {
            let mut changed = false;
            for &(u, v) in &edge_nodes {
                if z_node[v] > z_node[u] {
                    z_node[v] = z_node[u];
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut reaches = Vec::new();
        for (edge, &(u, v)) in anchor.river_edges.iter().zip(edge_nodes.iter()) {
            // Subdivide coastal mouth edges one level finer so `z_water` hits sea level cleanly
            // right at the coastline without long linear ramps offshore.
            let levels = if z_node[v] <= sea_m + 0.05 && z_node[u] > sea_m + 5.0 {
                5
            } else {
                4
            };
            subdivide_river_edge_with_z(
                edge,
                anchor.world_seed,
                sea_m,
                Some((z_node[u], z_node[v])),
                0,
                levels,
                &sample_ground,
                &sample_tough,
                &mut reaches,
            );
        }

        // Spawn deterministic 1st-order procedural tributaries on humid valley slopes,
        // snapping each tributary's mouth strictly to its parent reach's `z_water`.
        let mainstem_count = reaches.len();
        let mut tributaries = Vec::new();
        let stride = (mainstem_count / 14).max(4);
        for idx in (2..mainstem_count.saturating_sub(2)).step_by(stride) {
            let parent = &reaches[idx];
            let z_conf = 0.5 * (parent.z_water_0_m + parent.z_water_1_m);
            if z_conf <= sea_m + 18.0 || parent.discharge < 0.6 {
                continue;
            }
            let mx = 0.5 * (parent.p0_m[0] + parent.p1_m[0]);
            let my = 0.5 * (parent.p0_m[1] + parent.p1_m[1]);
            let ctx = strat.sample_context(anchor, mx, my);
            if ctx.precip_norm < 0.24 {
                continue;
            }

            let vx = parent.p1_m[0] - parent.p0_m[0];
            let vy = parent.p1_m[1] - parent.p0_m[1];
            let vlen = (vx * vx + vy * vy).sqrt().max(1.0);
            let ux = vx / vlen;
            let uy = vy / vlen;
            // Alternate left/right bank deterministically
            let h = value_noise2(
                (idx as f64) * 1.73,
                (parent.discharge as f64) * 0.91,
                anchor.world_seed ^ 0x4b1d_93a1,
            );
            let side = if h >= 0.5 { 1.0 } else { -1.0 };
            // Branch upstream at ~55 deg acute angle to parent flow
            let dir_x = -ux * 0.55 + (-uy * side) * 0.835;
            let dir_y = -uy * 0.55 + (ux * side) * 0.835;
            let trib_len = (10_000.0 + 18_000.0 * (ctx.precip_norm as f64)).clamp(8_000.0, 28_000.0);
            let head_xy = [mx + dir_x * trib_len, my + dir_y * trib_len];
            let head_ground = sample_ground(head_xy[0], head_xy[1]);

            // Only spawn a tributary if the headwater genuinely lies uphill of the confluence
            if head_ground <= z_conf + 24.0 {
                continue;
            }

            let q_trib = (parent.discharge * 0.16 * ctx.precip_norm).clamp(0.25, 18.0);
            let trib_edge = CoarseRiverEdge {
                edge_id: (anchor.world_seed.wrapping_mul(0x9e37_79b9))
                    ^ ((idx as u64) << 16)
                    ^ 0x7a5c_1234,
                from_xy_m: head_xy,
                to_xy_m: [mx, my],
                from_elev_m: head_ground,
                to_elev_m: z_conf,
                discharge: q_trib,
            };
            let before = tributaries.len();
            subdivide_river_edge(
                &trib_edge,
                anchor.world_seed ^ 0x3c6e_f372,
                z_conf, // Snap mouth strictly to parent confluence `z_water`
                1,
                3,
                &sample_ground,
                &sample_tough,
                &mut tributaries,
            );
            if tributaries.len() > before {
                if let Some(last) = tributaries.last_mut() {
                    last.z_water_1_m = z_conf.min(last.z_water_0_m);
                }
            }
        }
        reaches.extend(tributaries);

        // Detect climate- & rock-gated structural LakeBasins (impact craters, glacial cirques/troughs,
        // and impermeable structural depressions).
        let lakes = build_structural_lakes(anchor, strat, &reaches);

        Self { reaches, lakes }
    }

    /// Query the nearest influencing river reach at `(x_m, y_m)`.
    pub fn query_rivers(
        &self,
        x_m: f64,
        y_m: f64,
        min_channel_half_w_m: f32,
        include_tributaries: bool,
    ) -> Option<RiverQueryResult> {
        query_rivers_with_min_width(&self.reaches, x_m, y_m, min_channel_half_w_m, include_tributaries)
    }

    /// Query the nearest influencing structural [`LakeBasin`] at `(x_m, y_m)`.
    pub fn query_lakes(&self, x_m: f64, y_m: f64) -> Option<LakeQueryResult> {
        let mut best: Option<LakeQueryResult> = None;
        for lake in &self.lakes {
            let dx = (x_m - lake.center_xy_m[0]) as f32;
            let dy = (y_m - lake.center_xy_m[1]) as f32;
            let lx = dx * lake.cos_a + dy * lake.sin_a;
            let ly = -dx * lake.sin_a + dy * lake.cos_a;
            let rn = ((lx / lake.radius_x_m).powi(2) + (ly / lake.radius_y_m).powi(2)).sqrt();
            if rn > 1.85 {
                continue;
            }
            let in_basin = rn <= 1.0;
            let floor_elev_m = if in_basin {
                lake.z_water_m - lake.max_depth_m * (1.0 - rn * rn).max(0.05)
            } else {
                lake.z_water_m
            };
            let rim_blend = if in_basin {
                1.0
            } else {
                let u = ((rn - 1.0) / 0.85).clamp(0.0, 1.0);
                1.0 - (u * u * (3.0 - 2.0 * u))
            };
            let replace = match best {
                None => true,
                Some(prev) => rim_blend > prev.rim_blend,
            };
            if replace {
                best = Some(LakeQueryResult {
                    z_water_m: lake.z_water_m,
                    floor_elev_m,
                    rim_blend,
                    in_basin,
                    state: lake.state,
                });
            }
        }
        best
    }
}

/// Evaluate physical [`LakeState`] from climate evaporation, bedrock permeability (karst/porous vs.
/// impermeable shield/clay), and annual temperature.
pub fn classify_lake_state(
    precip_norm: f32,
    annual_temp_c: f32,
    glacier_cover: f32,
    calcity: f32,
    clasticity: f32,
    toughness: f32,
    salinity: f32,
) -> LakeState {
    // 1. Bedrock permeability: soluble karst limestone/chalk or coarse porous sandstone drains
    // water underground into aquifers/caverns instead of holding a surface lake.
    if calcity > 0.65 || (clasticity > 5.0 && toughness < 0.52) {
        return LakeState::DryPlaya;
    }

    // 2. Evaporation vs. inflow balance
    let inflow = precip_norm * 1.25 + glacier_cover * 0.65;
    let evap = ((annual_temp_c + 10.0) / 35.0).clamp(0.08, 1.6) * (1.25 - precip_norm).max(0.15);
    if inflow < evap * 0.42 {
        return if salinity > 0.22 || precip_norm < 0.16 {
            LakeState::SaltFlat
        } else {
            LakeState::DryPlaya
        };
    }

    // 3. Freezing threshold
    if annual_temp_c < -2.0 {
        LakeState::Frozen
    } else {
        LakeState::Freshwater
    }
}

fn build_structural_lakes(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
    reaches: &[DrainageReach],
) -> Vec<LakeBasin> {
    let mut lakes = Vec::new();
    let sea_m = anchor.sea_level_m as f32;

    for nb in anchor.neighbors.iter().take(7) {
        if nb.elevation_m <= sea_m + 35.0 {
            continue;
        }
        // Only spawn structural lakes where geology creates a basin:
        // impact craters, glaciated alpine troughs/shields, or low-slope structural basins
        let is_crater = nb.impact_scar > 0.25;
        let is_glacial_basin = nb.is_glaciated || nb.glacier_cover > 0.35;
        let basin_roll = value_noise2(
            (nb.tile_id as f64) * 0.73,
            (anchor.world_seed & 0xffff) as f64 * 0.19,
            anchor.world_seed ^ 0x6f2a_19d3,
        );
        let is_structural = nb.mountainosity < 2.4 && basin_roll > 0.76;
        if !is_crater && !is_glacial_basin && !is_structural {
            continue;
        }

        // Deterministic offset inside tile so lakes aren't all pinned to (0, 0)
        let ox = (value_noise2(nb.tile_id as f64 * 1.1, 2.3, anchor.world_seed ^ 0x1111) - 0.5)
            * anchor.parent_tile_radius_m
            * 0.55;
        let oy = (value_noise2(nb.tile_id as f64 * 1.1, 7.9, anchor.world_seed ^ 0x2222) - 0.5)
            * anchor.parent_tile_radius_m
            * 0.55;
        let cx = if is_crater { nb.local_xy_m[0] } else { nb.local_xy_m[0] + ox };
        let cy = if is_crater { nb.local_xy_m[1] } else { nb.local_xy_m[1] + oy };

        // Don't overlap an active mainstem channel
        if let Some(rh) = query_rivers_with_min_width(reaches, cx, cy, 0.0, false) {
            if rh.dist_m < rh.bank_dist_m + 6000.0 || rh.valley_blend > 0.35 {
                continue;
            }
        }

        let ctx = strat.sample_context(anchor, cx, cy);
        let base_e = anchor.sample_history_fine_ground(cx, cy, ctx.base_elevation_m);
        if base_e <= sea_m + 25.0 {
            continue;
        }
        let col_strata = strat.evaluate_column(&ctx, cx, cy, base_e);
        let top = &col_strata[0];
        let clasticity = nb
            .layers
            .last()
            .and_then(|l| l.rock.clasticity)
            .unwrap_or(0.0);

        let state = classify_lake_state(
            ctx.precip_norm,
            ctx.annual_temp_c,
            ctx.glacier_cover,
            top.calcity,
            clasticity,
            top.toughness,
            top.salinity,
        );

        let (rx, ry, depth) = if is_crater {
            (2200.0f32, 2100.0f32, 34.0f32)
        } else if is_glacial_basin {
            (3800.0f32, 1600.0f32, 26.0f32)
        } else {
            (4600.0f32, 2800.0f32, 16.0f32)
        };
        let angle = (basin_roll * std::f64::consts::PI) as f32;
        lakes.push(LakeBasin {
            center_xy_m: [cx, cy],
            radius_x_m: rx,
            radius_y_m: ry,
            cos_a: angle.cos(),
            sin_a: angle.sin(),
            z_water_m: (base_e - 8.0).max(sea_m + 4.0),
            max_depth_m: depth,
            state,
        });
    }

    lakes
}

/// Subdivide all parent `CoarseRiverEdge`s into a `DrainageNetwork` (convenience wrapper).
pub fn build_river_splines(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
) -> Vec<RiverSplineSegment> {
    DrainageNetwork::build(anchor, strat).reaches
}

/// Subdivide a single river edge with anchor-invariant, slope- & toughness-dependent meandering
/// followed by 3 Chaikin $C^1$ corner-cutting passes, strictly preserving `z_water_1_m <= z_water_0_m`.
pub fn subdivide_river_edge<F, G>(
    edge: &CoarseRiverEdge,
    world_seed: u64,
    min_water_z_m: f32,
    order: u8,
    levels: u32,
    sample_base_elev: F,
    sample_toughness: G,
    out: &mut Vec<DrainageReach>,
) where
    F: Fn(f64, f64) -> f32,
    G: Fn(f64, f64) -> f32,
{
    subdivide_river_edge_with_z(
        edge,
        world_seed,
        min_water_z_m,
        None,
        order,
        levels,
        sample_base_elev,
        sample_toughness,
        out,
    );
}

#[allow(clippy::too_many_arguments)]
fn subdivide_river_edge_with_z<F, G>(
    edge: &CoarseRiverEdge,
    world_seed: u64,
    min_water_z_m: f32,
    forced_z: Option<(f32, f32)>,
    order: u8,
    levels: u32,
    sample_base_elev: F,
    sample_toughness: G,
    out: &mut Vec<DrainageReach>,
) where
    F: Fn(f64, f64) -> f32,
    G: Fn(f64, f64) -> f32,
{
    let incision_m = if order == 0 {
        (14.0 + 4.5 * edge.discharge.sqrt()).clamp(14.0, 36.0)
    } else {
        (6.0 + 2.5 * edge.discharge.sqrt()).clamp(5.0, 16.0)
    };
    let (z_start, z_end) = if let Some((zs, ze)) = forced_z {
        (zs.max(min_water_z_m), ze.max(min_water_z_m).min(zs.max(min_water_z_m)))
    } else {
        let base_from = sample_base_elev(edge.from_xy_m[0], edge.from_xy_m[1]);
        let base_to = sample_base_elev(edge.to_xy_m[0], edge.to_xy_m[1]);
        let zs = (base_from.min(edge.from_elev_m) - incision_m).max(min_water_z_m);
        let ze = if order > 0 {
            min_water_z_m.min(zs)
        } else {
            (base_to.min(edge.to_elev_m) - incision_m)
                .max(min_water_z_m)
                .min(zs)
        };
        (zs, ze)
    };
    let mut pts: Vec<([f64; 2], f32)> = vec![(edge.from_xy_m, z_start), (edge.to_xy_m, z_end)];

    let dx_tot = edge.to_xy_m[0] - edge.from_xy_m[0];
    let dy_tot = edge.to_xy_m[1] - edge.from_xy_m[1];
    let total_len = (dx_tot * dx_tot + dy_tot * dy_tot).sqrt().max(1.0);
    let avg_slope = ((z_start - z_end).max(0.0) as f64) / total_len;
    let mid_tough = sample_toughness(
        0.5 * (edge.from_xy_m[0] + edge.to_xy_m[0]),
        0.5 * (edge.from_xy_m[1] + edge.to_xy_m[1]),
    )
    .clamp(0.1, 1.0) as f64;

    // Lowland soft-rock rivers meander widely; steep tough-rock gorges run straighter
    let tough_mod = 1.25 - 0.55 * mid_tough;
    let meander_factor = ((0.32 * tough_mod) / (1.0 + avg_slope * 150.0)).clamp(0.05, 0.34);

    for lvl in 0..levels {
        let mut next = Vec::with_capacity(pts.len() * 2 - 1);
        let seg_count = pts.len().saturating_sub(1).max(1) as f64;
        for (seg_idx, w) in pts.windows(2).enumerate() {
            let (p0, z0) = w[0];
            let (p1, z1) = w[1];
            next.push((p0, z0));

            let dx = p1[0] - p0[0];
            let dy = p1[1] - p0[1];
            let seg_len = (dx * dx + dy * dy).sqrt().max(1e-6);
            let perp = [-dy / seg_len, dx / seg_len];

            let seed = world_seed
                ^ edge.edge_id.wrapping_mul(0x9e3779b9)
                ^ ((lvl as u64) << 24)
                ^ (seg_idx as u64);
            // Anchor-invariant edge-local coordinates so river meanders never shift when re-anchoring
            let frac_along = (seg_idx as f64 + 0.5) / seg_count;
            let n = value_noise2(frac_along * 16.0, (lvl as f64) * 3.7 + 0.5, seed) * 2.0 - 1.0;

            let lvl_att = 1.0 / (1.0 + 0.20 * (lvl as f64));
            let offset = n * seg_len * meander_factor * lvl_att;
            let mid = [
                0.5 * (p0[0] + p1[0]) + perp[0] * offset,
                0.5 * (p0[1] + p1[1]) + perp[1] * offset,
            ];

            // Never let `z_water` float above the ground on steep descents (which would force
            // the valley envelope to build a raised levee/causeway): snap directly to `local_target`
            // whenever the ground drops below `z_lin`, while keeping `z_lin` through high ridges
            // so rivers incise deep canyons.
            let mid_ground = sample_base_elev(mid[0], mid[1]);
            let local_target = (mid_ground - incision_m).max(min_water_z_m);
            let z_lin = 0.5 * (z0 + z1);
            let z_mid = if mid_ground <= min_water_z_m {
                min_water_z_m
            } else if local_target < z_lin {
                local_target.clamp(z1, z0)
            } else {
                (0.65 * z_lin + 0.35 * local_target).clamp(z1, z0)
            };
            next.push((mid, z_mid));
        }
        if let Some(&last) = pts.last() {
            next.push(last);
        }
        pts = next;
    }

    // 3 Chaikin corner-cutting passes for smooth $C^1$ river bends
    for _ in 0..3 {
        if pts.len() < 3 {
            break;
        }
        let mut smooth = Vec::with_capacity(pts.len() * 2);
        smooth.push(pts[0]);
        for w in pts.windows(2) {
            let (p0, z0) = w[0];
            let (p1, z1) = w[1];
            let q = [0.75 * p0[0] + 0.25 * p1[0], 0.75 * p0[1] + 0.25 * p1[1]];
            let r = [0.25 * p0[0] + 0.75 * p1[0], 0.25 * p0[1] + 0.75 * p1[1]];
            let zq = (0.75 * z0 + 0.25 * z1).max(min_water_z_m);
            let zr = (0.25 * z0 + 0.75 * z1).max(min_water_z_m);
            smooth.push((q, zq));
            smooth.push((r, zr));
        }
        smooth.push(*pts.last().unwrap());
        pts = smooth;
    }

    // Clamp smoothed vertices so Chaikin corner-cutting never lifts `z_water` above the local
    // ground elevation on steep escarpments, then enforce strict downstream monotonicity.
    for i in 1..pts.len().saturating_sub(1) {
        let g = sample_base_elev(pts[i].0[0], pts[i].0[1]);
        let cap = if g <= min_water_z_m {
            min_water_z_m
        } else {
            (g - incision_m).max(min_water_z_m)
        };
        if pts[i].1 > cap {
            pts[i].1 = cap;
        }
    }

    // Strictly enforce non-increasing monotonicity pass across all smoothed vertices
    for i in 1..pts.len() {
        if pts[i].1 > pts[i - 1].1 {
            pts[i].1 = pts[i - 1].1;
        }
        if pts[i].1 < min_water_z_m {
            pts[i].1 = min_water_z_m;
        }
    }

    // Leopold-Maddock hydraulic geometry scaling (W ~ Q^0.55, D ~ Q^0.40)
    let q_clamped = edge.discharge.max(0.2);
    let base_width_m = if order == 0 {
        (12.0 + 64.0 * q_clamped.powf(0.55)).clamp(15.0, 2200.0)
    } else {
        (8.0 + 28.0 * q_clamped.powf(0.50)).clamp(8.0, 140.0)
    };
    let base_depth_m = (2.2 + 3.2 * q_clamped.powf(0.40)).clamp(1.8, 30.0);
    let num_segs = pts.len().saturating_sub(1).max(1);

    let start_out = out.len();
    for i in 0..num_segs {
        let (p0, z0) = pts[i];
        let (p1, z1) = pts[i + 1];
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let seg_len = (dx * dx + dy * dy).sqrt().max(1.0);
        let seg_slope = ((z0 - z1).max(0.0) as f64) / seg_len;

        let mut bend_bias = 0.0f32;
        if i > 0 && i + 2 < pts.len() {
            let p_prev = pts[i - 1].0;
            let p_next = pts[i + 2].0;
            let t0 = [p0[0] - p_prev[0], p0[1] - p_prev[1]];
            let t1 = [p_next[0] - p1[0], p_next[1] - p1[1]];
            let l0 = (t0[0] * t0[0] + t0[1] * t0[1]).sqrt().max(1e-6);
            let l1 = (t1[0] * t1[0] + t1[1] * t1[1]).sqrt().max(1e-6);
            let cross = (t0[0] * t1[1] - t0[1] * t1[0]) / (l0 * l1);
            bend_bias = (-cross as f32 * 2.2).clamp(-0.55, 0.55);
        }

        let seg_tough = sample_toughness(0.5 * (p0[0] + p1[0]), 0.5 * (p0[1] + p1[1])).clamp(0.1, 1.0);
        let tough_w_mod = (1.22 - 0.50 * seg_tough).clamp(0.65, 1.20);
        let slope_width_mod = (1.30 / (1.0 + (seg_slope as f32) * 150.0)).clamp(0.45, 1.35);
        let slope_depth_mod = (0.90 + (seg_slope as f32) * 40.0).clamp(0.85, 1.35);
        let z_mid = 0.5 * (z0 + z1);
        let rel_sea = (z_mid - min_water_z_m).max(0.0);
        let estuary_flare = if order == 0 {
            1.0 + 1.15 * (1.0 - (rel_sea / 35.0).clamp(0.0, 1.0)).powi(2)
        } else {
            1.0
        };

        let frac_down = (i as f32) / (num_segs as f32);
        let downstream_grow = 0.85 + 0.30 * frac_down;
        let reach_noise = fbm2(
            (frac_down as f64) * 18.0,
            ((edge.edge_id & 0xffff) as f64) * 0.17,
            world_seed ^ edge.edge_id.wrapping_mul(0x517cc1b7),
            2,
        );
        let width_var = (1.0 + 0.28 * reach_noise).clamp(0.70, 1.35);
        let depth_var = (1.0 - 0.20 * reach_noise + 0.22 * bend_bias.abs()).clamp(0.74, 1.42);

        let channel_width_m = (base_width_m
            * downstream_grow
            * slope_width_mod
            * tough_w_mod
            * estuary_flare
            * width_var)
            .clamp(8.0, 2800.0);
        let channel_depth_m =
            (base_depth_m * downstream_grow * slope_depth_mod * depth_var).clamp(1.8, 34.0);
        let valley_mult = (9.0 / (1.0 + (seg_slope as f32) * 90.0)).clamp(3.0, 9.0);
        let valley_half_width_m = (channel_width_m * valley_mult).clamp(260.0, 14_000.0);

        out.push(DrainageReach {
            p0_m: p0,
            p1_m: p1,
            z_water_0_m: z0,
            z_water_1_m: z1.min(z0),
            order,
            channel_width_m,
            channel_width_0_m: channel_width_m,
            channel_width_1_m: channel_width_m,
            channel_depth_m,
            valley_half_width_m,
            bend_bias,
            discharge: edge.discharge,
        });
    }

    // Smooth per-vertex channel widths across consecutive reaches of this edge so adjacent
    // segments share the exact same radius at their shared vertex (preventing wide downstream
    // estuary end-caps from overhanging upstream banks).
    let added = out.len() - start_out;
    if added >= 2 {
        let mut vw = vec![0.0f32; added + 1];
        vw[0] = out[start_out].channel_width_m;
        for k in 1..added {
            vw[k] = 0.5 * (out[start_out + k - 1].channel_width_m + out[start_out + k].channel_width_m);
        }
        vw[added] = out[start_out + added - 1].channel_width_m;
        for _ in 0..3 {
            let prev = vw.clone();
            for k in 1..added {
                vw[k] = 0.25 * prev[k - 1] + 0.5 * prev[k] + 0.25 * prev[k + 1];
            }
        }
        for k in 0..added {
            let seg = &mut out[start_out + k];
            seg.channel_width_0_m = vw[k];
            seg.channel_width_1_m = vw[k + 1];
            seg.channel_width_m = 0.5 * (vw[k] + vw[k + 1]);
        }
    }
}

/// Query the influencing river network at `(x_m, y_m)`.
/// Active wetted-channel hits take precedence; outside channels, overlapping valley walls from
/// adjacent segments and converging tributaries are blended with $C^1$-smooth inverse-distance
/// weights (`w_i = (valley_blend_i / (bank_dist_i + 12.0))^2`) so interfluve ridges between
/// tributaries never show sharp Voronoi bisector seams.
pub fn query_rivers_with_min_width(
    segments: &[DrainageReach],
    x_m: f64,
    y_m: f64,
    min_channel_half_w_m: f32,
    include_tributaries: bool,
) -> Option<RiverQueryResult> {
    let mut best_channel: Option<RiverQueryResult> = None;

    // Smooth valley-wall accumulators across all overlapping segments/tributaries
    let mut w_sum = 0.0f64;
    let mut z_sum = 0.0f64;
    let mut u_sum = 0.0f64;
    let mut dist_sum = 0.0f64;
    let mut bank_dist_sum = 0.0f64;
    let mut q_sum = 0.0f64;
    let mut one_minus_blend_prod = 1.0f64;

    for seg in segments {
        if !include_tributaries && seg.order > 0 {
            continue;
        }
        let max_seg_half_w = if seg.order == 0 {
            (seg.channel_width_0_m.max(seg.channel_width_1_m) * 0.5).max(min_channel_half_w_m)
        } else {
            (seg.channel_width_0_m.max(seg.channel_width_1_m) * 0.5).max(min_channel_half_w_m * 0.65)
        };
        let eff_valley_w = seg.valley_half_width_m.max(max_seg_half_w * 3.5);
        let pad = (eff_valley_w + max_seg_half_w * 0.25) as f64;
        let min_x = seg.p0_m[0].min(seg.p1_m[0]) - pad;
        let max_x = seg.p0_m[0].max(seg.p1_m[0]) + pad;
        if x_m < min_x || x_m > max_x {
            continue;
        }
        let min_y = seg.p0_m[1].min(seg.p1_m[1]) - pad;
        let max_y = seg.p0_m[1].max(seg.p1_m[1]) + pad;
        if y_m < min_y || y_m > max_y {
            continue;
        }

        let vx = seg.p1_m[0] - seg.p0_m[0];
        let vy = seg.p1_m[1] - seg.p0_m[1];
        let len2 = vx * vx + vy * vy;
        let t = if len2 > 1e-9 {
            (((x_m - seg.p0_m[0]) * vx + (y_m - seg.p0_m[1]) * vy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let px = seg.p0_m[0] + t * vx;
        let py = seg.p0_m[1] + t * vy;
        let dx = x_m - px;
        let dy = y_m - py;
        let dist_m = (dx * dx + dy * dy).sqrt() as f32;

        if dist_m > eff_valley_w {
            continue;
        }

        let interp_w = seg.channel_width_0_m + (seg.channel_width_1_m - seg.channel_width_0_m) * (t as f32);
        let base_half_w = if seg.order == 0 {
            (interp_w * 0.5).max(min_channel_half_w_m)
        } else {
            (interp_w * 0.5).max(min_channel_half_w_m * 0.65)
        };

        let bank_wiggle = if min_channel_half_w_m < 1.0 {
            let n = fbm2(x_m * 0.022, y_m * 0.022, 0x71e3_9a4b, 2);
            1.0 + 0.14 * n
        } else {
            1.0
        };
        let eff_half_w = (base_half_w * bank_wiggle).max(3.0);

        let z_water_m = seg.z_water_0_m + (seg.z_water_1_m - seg.z_water_0_m) * (t as f32);
        let in_channel = dist_m <= eff_half_w;
        let bank_dist_m = (dist_m - eff_half_w).max(0.0);
        let valley_u = (bank_dist_m / (eff_valley_w - eff_half_w).max(1.0)).clamp(0.0, 1.0);
        let valley_blend = if in_channel {
            1.0
        } else {
            1.0 - (valley_u * valley_u * (3.0 - 2.0 * valley_u))
        };

        if in_channel {
            let mut bar_height_m = 0.0f32;
            let seg_len = len2.sqrt().max(1e-6);
            let ux = vx / seg_len;
            let uy = vy / seg_len;
            let signed_perp = (((-uy * dx + ux * dy) as f32) / eff_half_w).clamp(-1.0, 1.0);
            let shifted =
                ((signed_perp - seg.bend_bias) / (1.0 + seg.bend_bias.abs())).clamp(-1.0, 1.0);
            let profile = (1.0 - shifted * shifted).max(0.14);
            let depth = seg.channel_depth_m * profile;

            // Braided mid-channel sandbars / alluvial islands in wide low-slope rivers
            if seg.channel_width_m > 320.0 && signed_perp.abs() < 0.72 {
                let thalweg_dist = (signed_perp - seg.bend_bias).abs();
                if thalweg_dist > 0.24 {
                    let along = x_m * ux + y_m * uy;
                    let across = -x_m * uy + y_m * ux;
                    let bar_n = fbm2(along / 420.0, across / 105.0, 0x3b2c_91d5, 2);
                    let center_env = (1.0 - (signed_perp / 0.72).powi(2)).clamp(0.0, 1.0);
                    let width_factor = ((seg.channel_width_m - 320.0) / 500.0).clamp(0.0, 1.0);
                    let bar_signal = bar_n * center_env * width_factor;
                    if bar_signal > 0.26 {
                        bar_height_m = ((bar_signal - 0.26) / 0.55).clamp(0.0, 1.0) * 3.5;
                    }
                }
            }

            let bed_elev_m = if bar_height_m > 0.0 {
                z_water_m + bar_height_m
            } else {
                z_water_m - depth.max(0.8)
            };

            let replace = match best_channel {
                None => true,
                Some(prev) => dist_m < prev.dist_m,
            };
            if replace {
                best_channel = Some(RiverQueryResult {
                    dist_m,
                    bank_dist_m: 0.0,
                    valley_u: 0.0,
                    z_water_m,
                    bed_elev_m,
                    valley_blend: 1.0,
                    in_channel: true,
                    bar_height_m,
                    discharge: seg.discharge,
                });
            }
        } else if valley_blend > 1e-5 {
            let ratio = (valley_blend as f64) / ((bank_dist_m as f64) + 12.0);
            let w = ratio * ratio;
            w_sum += w;
            z_sum += w * (z_water_m as f64);
            u_sum += w * (valley_u as f64);
            dist_sum += w * (dist_m as f64);
            bank_dist_sum += w * (bank_dist_m as f64);
            q_sum += w * (seg.discharge as f64);
            one_minus_blend_prod *= (1.0 - (valley_blend as f64)).clamp(0.0, 1.0);
        }
    }

    if best_channel.is_some() {
        return best_channel;
    }

    if w_sum > 0.0 {
        let z_water_m = (z_sum / w_sum) as f32;
        let valley_u = (u_sum / w_sum).clamp(0.0, 1.0) as f32;
        let valley_blend = (1.0 - one_minus_blend_prod).clamp(0.0, 1.0) as f32;
        return Some(RiverQueryResult {
            dist_m: (dist_sum / w_sum) as f32,
            bank_dist_m: (bank_dist_sum / w_sum) as f32,
            valley_u,
            z_water_m,
            bed_elev_m: z_water_m,
            valley_blend,
            in_channel: false,
            bar_height_m: 0.0,
            discharge: (q_sum / w_sum) as f32,
        });
    }

    None
}

/// Query river segments at exact physical width (`min_channel_half_w_m = 0.0`, including tributaries).
pub fn query_rivers(segments: &[DrainageReach], x_m: f64, y_m: f64) -> Option<RiverQueryResult> {
    query_rivers_with_min_width(segments, x_m, y_m, 0.0, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_river_monotonicity_and_canyon_carving_invariant() {
        let edge = CoarseRiverEdge {
            edge_id: 42,
            from_xy_m: [-50_000.0, 0.0],
            to_xy_m: [50_000.0, 0.0],
            from_elev_m: 600.0,
            to_elev_m: 50.0,
            discharge: 18.0,
        };
        let mut segs = Vec::new();
        // High mountain barrier in the middle (1200 m) — river must STILL flow strictly downhill!
        subdivide_river_edge(
            &edge,
            12345,
            0.0,
            0,
            4,
            |x, _| {
                if x.abs() < 15_000.0 {
                    1200.0
                } else {
                    (600.0 - (x + 50_000.0) * 0.0055) as f32
                }
            },
            |_, _| 0.65,
            &mut segs,
        );
        assert!(segs.len() >= 32);
        let mut prev_z = segs[0].z_water_0_m;
        for s in &segs {
            assert!(
                s.z_water_0_m <= prev_z + 1e-4,
                "Non-monotonic jump between segments: {} > {}",
                s.z_water_0_m,
                prev_z
            );
            assert!(s.z_water_1_m <= s.z_water_0_m + 1e-4);
            prev_z = s.z_water_1_m;
        }
    }

    #[test]
    fn test_lake_state_climate_and_permeability_gating() {
        // Humid temperate impermeable granite -> Freshwater lake
        assert_eq!(
            classify_lake_state(0.65, 12.0, 0.0, 0.0, 0.0, 0.85, 0.0),
            LakeState::Freshwater
        );
        // Cold alpine impermeable -> Frozen lake
        assert_eq!(
            classify_lake_state(0.55, -6.0, 0.4, 0.0, 0.0, 0.85, 0.0),
            LakeState::Frozen
        );
        // Humid soluble karst limestone -> drains underground (DryPlaya)
        assert_eq!(
            classify_lake_state(0.80, 14.0, 0.0, 0.90, 2.0, 0.45, 0.0),
            LakeState::DryPlaya
        );
        // Hot arid high-salinity basin -> SaltFlat
        assert_eq!(
            classify_lake_state(0.08, 28.0, 0.0, 0.0, 2.0, 0.50, 0.40),
            LakeState::SaltFlat
        );
    }
}
