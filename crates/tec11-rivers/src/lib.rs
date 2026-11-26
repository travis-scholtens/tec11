use std::cmp::Reverse;
use std::collections::BinaryHeap;



/// Compute a globally consistent, cycle-free downstream flow target `downstream[t]` for every land tile `t`
/// using Priority-Flood depression routing from the ocean (`elevation <= 0.0`).
/// Because every land tile has at most one downstream neighbor, coarse river networks form a strict
/// dendritic forest where tributaries merge cleanly at confluences and never cross or diverge.
fn compute_downstream_targets(elevations: &[f32], adjacency: &[Vec<usize>]) -> Vec<Option<usize>> {
    let n = elevations.len();
    let mut downstream = vec![None; n];
    let mut visited = vec![false; n];
    let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();

    let elev_key = |e: f32| -> i64 { (e as f64 * 1_000_000.0).round() as i64 };

    for t in 0..n {
        if elevations[t] <= 0.0 {
            visited[t] = true;
            heap.push(Reverse((elev_key(elevations[t]), t)));
        }
    }

    // Priority-Flood from the sea inward: each unvisited land tile `nb` is reached first from its
    // lowest-elevation spill path `u`, establishing a unique monotonic drainage tree to the ocean.
    while let Some(Reverse((_, u))) = heap.pop() {
        for &nb in &adjacency[u] {
            if !visited[nb] {
                visited[nb] = true;
                // If `nb` has an already-visited neighbor with strictly lower elevation than `u`,
                // prefer the steepest-descent visited neighbor so hillslopes follow steepest gradient.
                let best_down = adjacency[nb]
                    .iter()
                    .copied()
                    .filter(|&cand| visited[cand])
                    .min_by(|&a, &b| {
                        elevations[a]
                            .partial_cmp(&elevations[b])
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| a.cmp(&b))
                    })
                    .unwrap_or(u);
                downstream[nb] = Some(best_down);
                heap.push(Reverse((elev_key(elevations[nb]), nb)));
            }
        }
    }

    downstream
}

/// Trace coarse rivers from local elevation peaks (`elevation >= hmin`) down to the sea,
/// starting each active river where cumulative normalized precipitation reaches `pmin`
/// on an unglaciated (`glacier_cover < 0.50`) land tile.
pub fn trace_coarse_rivers(
    elevations: &[f32],
    precip_norm: &[f32],
    glacier_cover: &[f32],
    adjacency: &[Vec<usize>],
    hmin: f32,
    pmin: f32,
) -> Vec<Vec<usize>> {
    let n = elevations.len();
    let downstream = compute_downstream_targets(elevations, adjacency);
    let mut rivers = Vec::new();

    for t in 0..n {
        let elev_t = elevations[t];
        if elev_t > 0.0
            && elev_t >= hmin
            && adjacency[t].iter().all(|&nb| elev_t >= elevations[nb])
        {
            let mut ts = Vec::new();
            let mut cur_opt = Some(t);
            let mut steps = 0usize;
            while let Some(u) = cur_opt {
                ts.push(u);
                if elevations[u] <= 0.0 || steps > n {
                    break;
                }
                cur_opt = downstream[u];
                steps += 1;
            }
            if ts.last().is_some_and(|&last| elevations[last] <= 0.0) {
                let mut p_cum = 0.0f32;
                for i in 0..ts.len() {
                    let u = ts[i];
                    if elevations[u] > 0.0 {
                        p_cum += precip_norm[u];
                    }
                    let is_glaciated = glacier_cover.get(u).copied().unwrap_or(0.0) >= 0.50;
                    if elevations[u] > 0.0 && !is_glaciated && p_cum >= pmin {
                        rivers.push(ts[i..].to_vec());
                        break;
                    }
                }
            }
        }
    }

    rivers
}


/// Standard minimum mountain elevation (km) for coarse river headwaters in terrain & tides views.
pub const TERRAIN_MIN_RIVER_ELEV: f32 = 5.0;
/// Standard minimum normalized precipitation for coarse river headwaters in terrain & tides views.
pub const TERRAIN_MIN_RIVER_PRECIP: f32 = 0.5;
