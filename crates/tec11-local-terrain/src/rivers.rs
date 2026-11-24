//! Deterministic hierarchical river spline subdivision (`RiverSplineSegment`),
//! strictly monotonic downstream water-surface profiles (`z_water_1_m <= z_water_0_m`),
//! riverbed carving, and valley/floodplain envelope blending (`FR-005`, `SC-005`).

use crate::anchor::{CoarseRiverEdge, LocalZoomAnchor};
use crate::stratigraphy::{value_noise2, RegionalStratigraphyField};

/// Deterministically subdivided river reach (`data-model.md`: `RiverSplineSegment`).
#[derive(Clone, Debug)]
pub struct RiverSplineSegment {
    pub p0_m: [f64; 2],
    pub p1_m: [f64; 2],
    /// Monotonicity invariant: `z_water_1_m <= z_water_0_m`.
    pub z_water_0_m: f32,
    pub z_water_1_m: f32,
    pub channel_width_m: f32,
    pub channel_depth_m: f32,
    pub valley_half_width_m: f32,
    pub discharge: f32,
}

/// Result of querying all river splines at tangent-plane point `(x_m, y_m)`.
#[derive(Copy, Clone, Debug)]
pub struct RiverQueryResult {
    /// Distance from `(x_m, y_m)` to the nearest river centerline (meters).
    pub dist_m: f32,
    /// Water-surface elevation $Z_{\text{water}}$ at the projected point on the centerline.
    pub z_water_m: f32,
    /// Carved riverbed elevation (strictly `< z_water_m` inside channel).
    pub bed_elev_m: f32,
    /// Valley blending factor in `[0, 1]` (`1.0` inside channel, tapering smoothly to `0.0` at valley edge).
    pub valley_blend: f32,
    /// True if `(x_m, y_m)` lies inside the active wetted channel (`dist_m <= channel_width_m * 0.5`).
    pub in_channel: bool,
}

/// Subdivide all parent `CoarseRiverEdge`s touching the anchor into deterministic,
/// strictly monotonic `RiverSplineSegment` chains anchored to the continuous interpolated
/// terrain elevation datum (`strat.sample_context`) so rivers never float above surrounding terrain.
pub fn build_river_splines(
    anchor: &LocalZoomAnchor,
    strat: &RegionalStratigraphyField,
) -> Vec<RiverSplineSegment> {
    if !anchor.has_atmosphere || anchor.river_edges.is_empty() {
        return Vec::new();
    }
    let mut segments = Vec::new();
    for edge in &anchor.river_edges {
        subdivide_river_edge(
            edge,
            anchor.world_seed,
            anchor.sea_level_m as f32,
            5,
            |x, y| strat.sample_context(anchor, x, y).base_elevation_m,
            &mut segments,
        );
    }
    segments
}

/// Subdivide a single `CoarseRiverEdge` `levels` times using slope-dependent midpoint
/// meandering while strictly preserving `z_water_1_m <= z_water_0_m` downstream.
pub fn subdivide_river_edge<F>(
    edge: &CoarseRiverEdge,
    world_seed: u64,
    sea_level_m: f32,
    levels: u32,
    sample_base_elev: F,
    out: &mut Vec<RiverSplineSegment>,
) where
    F: Fn(f64, f64) -> f32,
{
    // Anchor water elevation to the continuous interpolated terrain datum minus incision
    // so rivers always sit in carved valleys rather than floating above smoothed neighbors.
    let incision_m = (12.0 + 4.0 * edge.discharge.sqrt()).clamp(12.0, 32.0);
    let base_from = sample_base_elev(edge.from_xy_m[0], edge.from_xy_m[1]);
    let base_to = sample_base_elev(edge.to_xy_m[0], edge.to_xy_m[1]);
    let z_start = (base_from.min(edge.from_elev_m) - incision_m).max(sea_level_m);
    let z_end = (base_to.min(edge.to_elev_m) - incision_m)
        .max(sea_level_m)
        .min(z_start);
    let mut pts: Vec<([f64; 2], f32)> = vec![
        (edge.from_xy_m, z_start),
        (edge.to_xy_m, z_end),
    ];

    let dx_tot = edge.to_xy_m[0] - edge.from_xy_m[0];
    let dy_tot = edge.to_xy_m[1] - edge.from_xy_m[1];
    let total_len = (dx_tot * dx_tot + dy_tot * dy_tot).sqrt().max(1.0);
    let avg_slope = ((edge.from_elev_m - edge.to_elev_m).max(0.0) as f64) / total_len;

    // Lowland rivers (low slope) meander strongly; steep mountain torrents run straighter.
    let meander_factor = (0.28 / (1.0 + avg_slope * 180.0)).clamp(0.04, 0.28);

    for lvl in 0..levels {
        let mut next = Vec::with_capacity(pts.len() * 2 - 1);
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
            let n = value_noise2(
                (p0[0] + p1[0]) * 0.0002,
                (p0[1] + p1[1]) * 0.0002,
                seed,
            ) * 2.0
                - 1.0;

            let offset = n * seg_len * meander_factor;
            let mid = [
                0.5 * (p0[0] + p1[0]) + perp[0] * offset,
                0.5 * (p0[1] + p1[1]) + perp[1] * offset,
            ];

            // Follow the natural terrain surface minus valley incision, clamped to [z1, z0]
            // so the river hugs the landscape instead of sawing a 400 m slot canyon through plateaus,
            // while remaining strictly non-increasing downstream and >= sea_level_m.
            let local_target = (sample_base_elev(mid[0], mid[1]) - incision_m).max(sea_level_m);
            let z_mid = local_target.clamp(z1, z0);
            next.push((mid, z_mid));
        }
        if let Some(&last) = pts.last() {
            next.push(last);
        }
        pts = next;
    }

    // Enforce strict non-increasing monotonicity pass across all vertices
    for i in 1..pts.len() {
        if pts[i].1 > pts[i - 1].1 {
            pts[i].1 = pts[i - 1].1;
        }
    }

    let channel_width_m = (8.0 + 7.5 * edge.discharge.sqrt()).clamp(4.0, 90.0);
    let channel_depth_m = (1.5 + 1.2 * edge.discharge.sqrt()).clamp(1.0, 12.0);
    // Wider floodplains in flat lowlands, tighter canyons in steep terrain
    let valley_mult = (12.0 / (1.0 + (avg_slope as f32) * 100.0)).clamp(4.0, 12.0);
    let valley_half_width_m = (channel_width_m * valley_mult).max(180.0);

    for w in pts.windows(2) {
        let (p0, z0) = w[0];
        let (p1, z1) = w[1];
        out.push(RiverSplineSegment {
            p0_m: p0,
            p1_m: p1,
            z_water_0_m: z0,
            z_water_1_m: z1.min(z0),
            channel_width_m,
            channel_depth_m,
            valley_half_width_m,
            discharge: edge.discharge,
        });
    }
}

/// Query the nearest influencing river segment at `(x_m, y_m)`.
/// `min_channel_half_w_m` can be set to `0.0` for physical Level 3 queries, or `~0.55 * hex_spacing_m`
/// at coarse Level 0–2 drill-down patches so rivers remain visually continuous across km-scale hexes.
pub fn query_rivers_with_min_width(
    segments: &[RiverSplineSegment],
    x_m: f64,
    y_m: f64,
    min_channel_half_w_m: f32,
) -> Option<RiverQueryResult> {
    let mut best: Option<RiverQueryResult> = None;

    for seg in segments {
        let eff_half_w = (seg.channel_width_m * 0.5).max(min_channel_half_w_m);
        let eff_valley_w = seg.valley_half_width_m.max(eff_half_w * 3.0);
        // Quick AABB reject using eff_valley_w
        let pad = eff_valley_w as f64;
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
        let dist_m = ((x_m - px).powi(2) + (y_m - py).powi(2)).sqrt() as f32;

        if dist_m > eff_valley_w {
            continue;
        }

        let z_water_m = seg.z_water_0_m + (seg.z_water_1_m - seg.z_water_0_m) * (t as f32);
        let in_channel = dist_m <= eff_half_w;

        // Parabolic riverbed profile strictly below z_water_m inside channel
        let bed_elev_m = if in_channel {
            let r_norm = (dist_m / eff_half_w.max(1e-3)).clamp(0.0, 1.0);
            let depth = seg.channel_depth_m * (1.0 - 0.75 * r_norm * r_norm);
            z_water_m - depth.max(0.6)
        } else {
            z_water_m + 0.5
        };

        // Smooth Hermite valley envelope: 1.0 at channel bank -> 0.0 at eff_valley_w
        let valley_blend = if in_channel {
            1.0
        } else {
            let u = ((dist_m - eff_half_w) / (eff_valley_w - eff_half_w).max(1.0)).clamp(0.0, 1.0);
            1.0 - (u * u * (3.0 - 2.0 * u))
        };

        let replace = match best {
            None => true,
            Some(prev) => dist_m < prev.dist_m,
        };
        if replace {
            best = Some(RiverQueryResult {
                dist_m,
                z_water_m,
                bed_elev_m,
                valley_blend,
                in_channel,
            });
        }
    }

    best
}

/// Query the nearest influencing river segment at `(x_m, y_m)` at true physical channel width.
pub fn query_rivers(
    segments: &[RiverSplineSegment],
    x_m: f64,
    y_m: f64,
) -> Option<RiverQueryResult> {
    query_rivers_with_min_width(segments, x_m, y_m, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_river_subdivision_enforces_strict_monotonic_water_elevation() {
        let edge = CoarseRiverEdge {
            edge_id: 42,
            from_xy_m: [-20_000.0, -10_000.0],
            to_xy_m: [25_000.0, 15_000.0],
            from_elev_m: 1200.0,
            to_elev_m: 180.0,
            discharge: 4.0,
        };
        let mut segs = Vec::new();
        subdivide_river_edge(&edge, 12345, 0.0, 5, |_, _| 1200.0, &mut segs);
        assert_eq!(segs.len(), 32);
        for seg in &segs {
            assert!(
                seg.z_water_1_m <= seg.z_water_0_m + 1e-6,
                "River segment must be non-increasing downstream ({} -> {})",
                seg.z_water_0_m,
                seg.z_water_1_m
            );
            // Query midpoint inside channel: riverbed must be strictly below z_water
            let mid_x = 0.5 * (seg.p0_m[0] + seg.p1_m[0]);
            let mid_y = 0.5 * (seg.p0_m[1] + seg.p1_m[1]);
            let q = query_rivers(&segs, mid_x, mid_y).expect("Midpoint should hit channel");
            assert!(q.in_channel);
            assert!(
                q.bed_elev_m < q.z_water_m,
                "Riverbed ({}) must lie strictly below water surface ({})",
                q.bed_elev_m,
                q.z_water_m
            );
        }
    }
}
