use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::f64::consts::PI;

use tec11_math_grid::{
    cross, dist2, dot, length, normal, rotate, Pcg64Mcg, PointTree, SphericalGrid,
    SphericalPolygon, Vec3,
};

/// Average a slice of 3D vectors (`movemethod.py:average`).
pub fn average_vec3(vs: &[Vec3]) -> Vec3 {
    if vs.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let mut sum = [0.0, 0.0, 0.0];
    for v in vs {
        sum[0] += v[0];
        sum[1] += v[1];
        sum[2] += v[2];
    }
    let inv = 1.0 / (vs.len() as f64);
    [sum[0] * inv, sum[1] * inv, sum[2] * inv]
}

/// Find the tile in `group_tiles` closest in 3D chord distance to `target` (`movemethod.py:closest`).
pub fn closest_tile(grid: &SphericalGrid, group_tiles: &[usize], target: Vec3) -> usize {
    group_tiles
        .iter()
        .copied()
        .min_by(|&a, &b| {
            dist2(grid.faces[a].0, target).total_cmp(&dist2(grid.faces[b].0, target))
        })
        .unwrap_or(0)
}

/// Find the tile in `group_tiles` farthest in 3D chord distance from `target` (`movemethod.py:farthest`).
pub fn farthest_tile(grid: &SphericalGrid, group_tiles: &[usize], target: Vec3) -> usize {
    group_tiles
        .iter()
        .copied()
        .max_by(|&a, &b| {
            dist2(grid.faces[a].0, target).total_cmp(&dist2(grid.faces[b].0, target))
        })
        .unwrap_or(0)
}

/// Find the tile in `group_tiles` closest to the group's centroid (`movemethod.py:center`).
pub fn center_tile(grid: &SphericalGrid, group_tiles: &[usize]) -> usize {
    let vs: Vec<Vec3> = group_tiles.iter().map(|&t| grid.faces[t].0).collect();
    let avg = average_vec3(&vs);
    closest_tile(grid, group_tiles, avg)
}

/// Partition `tile_set` into connected components on the `adjacency` graph.
fn connected_components(adjacency: &[Vec<usize>], tile_set: &BTreeSet<usize>) -> Vec<Vec<usize>> {
    let mut visited = BTreeSet::new();
    let mut components = Vec::new();

    for &start in tile_set {
        if visited.contains(&start) {
            continue;
        }
        let mut comp = Vec::new();
        let mut queue = VecDeque::new();
        visited.insert(start);
        queue.push_back(start);

        while let Some(curr) = queue.pop_front() {
            comp.push(curr);
            for &nb in &adjacency[curr] {
                if tile_set.contains(&nb) && visited.insert(nb) {
                    queue.push_back(nb);
                }
            }
        }
        components.push(comp);
    }

    components
}

/// Detect and fill small enclosed non-group pockets (`<= max_hole_tiles` tiles) completely
/// surrounded by `new_map` (inspired by `holemethod.py` in `Tec-Nine`).
fn fill_enclosed_holes(
    adjacency: &[Vec<usize>],
    new_map: &mut BTreeMap<usize, Vec<usize>>,
    max_hole_tiles: usize,
    close_four_neighbor_cracks: bool,
) {
    let mut candidates = BTreeSet::new();
    for &t in new_map.keys() {
        for &nb in &adjacency[t] {
            if !new_map.contains_key(&nb) {
                candidates.insert(nb);
            }
        }
    }

    let mut visited_non_group = BTreeSet::new();
    let mut enclosed_holes: Vec<Vec<usize>> = Vec::new();

    for start in candidates {
        if visited_non_group.contains(&start) || new_map.contains_key(&start) {
            continue;
        }

        let mut comp = Vec::new();
        let mut queue = VecDeque::new();
        let mut exceeded = false;
        visited_non_group.insert(start);
        queue.push_back(start);

        while let Some(curr) = queue.pop_front() {
            comp.push(curr);
            if comp.len() > max_hole_tiles {
                exceeded = true;
                break;
            }
            for &nb in &adjacency[curr] {
                if !new_map.contains_key(&nb) && visited_non_group.insert(nb) {
                    queue.push_back(nb);
                }
            }
        }

        if !exceeded {
            enclosed_holes.push(comp);
        }
    }

    // Fill each enclosed hole from its surrounding `new_map` neighbors inward.
    for mut hole in enclosed_holes {
        while !hole.is_empty() {
            let mut filled_any = false;
            let mut remaining = Vec::new();
            for h in hole {
                let mut srcs = Vec::new();
                for &nb in &adjacency[h] {
                    if let Some(nb_srcs) = new_map.get(&nb) {
                        if let Some(&first_src) = nb_srcs.first() {
                            if !srcs.contains(&first_src) {
                                srcs.push(first_src);
                            }
                        }
                    }
                }
                if !srcs.is_empty() {
                    new_map.insert(h, srcs);
                    filled_any = true;
                } else {
                    remaining.push(h);
                }
            }
            if !filled_any {
                break;
            }
            hole = remaining;
        }
    }

    // Morphological closing pass:
    // - When `close_four_neighbor_cracks` is true, iteratively close any concave notch or
    //   1-tile-wide crack surrounded on >= 4 sides by `new_map` until convergence.
    //   On a hex grid, straight and convex coastlines have at most 2 or 3 neighbors in `new_map`,
    //   so >= 4 strictly targets concave notches and 1-tile fissures without advancing straight coasts.
    // - Always close any 1-tile pinhole surrounded on >= (deg - 1) sides (>= 5 of 6).
    let max_passes = if close_four_neighbor_cracks { 24 } else { 6 };
    for _pass in 0..max_passes {
        let mut notch_fills: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut border_candidates = BTreeSet::new();
        for &t in new_map.keys() {
            for &nb in &adjacency[t] {
                if !new_map.contains_key(&nb) {
                    border_candidates.insert(nb);
                }
            }
        }
        for c in border_candidates {
            let total_nbs = adjacency[c].len();
            let mut srcs = Vec::new();
            let mut in_count = 0usize;
            for &nb in &adjacency[c] {
                if let Some(nb_srcs) = new_map.get(&nb) {
                    in_count += 1;
                    if let Some(&first_src) = nb_srcs.first() {
                        if !srcs.contains(&first_src) {
                            srcs.push(first_src);
                        }
                    }
                }
            }
            let threshold = if close_four_neighbor_cracks {
                4
            } else {
                total_nbs.saturating_sub(1).max(4)
            };
            if in_count >= threshold && !srcs.is_empty() {
                notch_fills.push((c, srcs));
            }
        }
        if notch_fills.is_empty() {
            break;
        }
        for (c, srcs) in notch_fills {
            new_map.insert(c, srcs);
        }
    }
}

/// Returns `true` if `t` is a "simple convex boundary point" of `new_map`:
/// 1. `t` has `1 <= k <= 3` neighbors in `new_map` (`k < 4`, so removing `t` never leaves `t` as a
///    `>= 4`-neighbor concave notch).
/// 2. Those `k` neighbors of `t` in `new_map` form a single connected arc in `adjacency`, and every
///    such neighbor retains `>= 2` neighbors in `new_map` after `t` is removed (so removing `t`
///    cannot disconnect `new_map` or create a 1-tile spike).
fn is_simple_convex_boundary_tile(
    adjacency: &[Vec<usize>],
    new_map: &BTreeMap<usize, Vec<usize>>,
    t: usize,
) -> bool {
    let in_nbs: Vec<usize> = adjacency[t]
        .iter()
        .copied()
        .filter(|&nb| new_map.contains_key(&nb))
        .collect();
    let k = in_nbs.len();
    if k == 0 || k > 3 {
        return false;
    }

    // Check that `in_nbs` is connected within `adjacency`:
    // - k == 1: trivially connected (a 1-neighbor spike).
    // - k == 2: the two neighbors must be adjacent to each other.
    // - k == 3: at least one of the three neighbors must be adjacent to both of the other two
    //           (forming a contiguous 3-tile arc around `t`).
    let arc_connected = match k {
        1 => true,
        2 => adjacency[in_nbs[0]].contains(&in_nbs[1]),
        3 => {
            let a01 = adjacency[in_nbs[0]].contains(&in_nbs[1]);
            let a12 = adjacency[in_nbs[1]].contains(&in_nbs[2]);
            let a02 = adjacency[in_nbs[0]].contains(&in_nbs[2]);
            (a01 as u8) + (a12 as u8) + (a02 as u8) >= 2
        }
        _ => false,
    };
    if !arc_connected {
        return false;
    }

    // Ensure every neighbor in `in_nbs` still has >= 2 neighbors in `new_map` after `t` is removed
    // (unless `new_map` is tiny).
    if new_map.len() > 6 {
        for &nb in &in_nbs {
            let rem_nbs = adjacency[nb]
                .iter()
                .filter(|&&n2| n2 != t && new_map.contains_key(&n2))
                .count();
            if rem_nbs < 2 {
                return false;
            }
        }
    }

    true
}

/// Remove detached coastal specks and 1-tile spikes (`<= 1` neighbor in `new_map`) by keeping only
/// the single largest connected component of `new_map` with a clean boundary.
fn prune_detached_specks(adjacency: &[Vec<usize>], new_map: &mut BTreeMap<usize, Vec<usize>>) {
    if new_map.len() <= 3 {
        return;
    }
    // Shave any 1-tile peninsula tips that only touch `new_map` by <= 1 neighbor.
    loop {
        let spikes: Vec<usize> = new_map
            .keys()
            .copied()
            .filter(|&t| {
                adjacency[t]
                    .iter()
                    .filter(|&&nb| new_map.contains_key(&nb))
                    .count()
                    <= 1
            })
            .collect();
        if spikes.is_empty() || new_map.len() <= spikes.len() + 2 {
            break;
        }
        for s in spikes {
            new_map.remove(&s);
        }
    }

    let tile_set: BTreeSet<usize> = new_map.keys().copied().collect();
    let comps = connected_components(adjacency, &tile_set);
    if comps.len() <= 1 {
        return;
    }
    let largest_idx = comps
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| c.len())
        .map(|(i, _)| i)
        .unwrap_or(0);

    for (idx, comp) in comps.into_iter().enumerate() {
        if idx != largest_idx {
            for t in comp {
                new_map.remove(&t);
            }
        }
    }
}

/// Advect a lithospheric group of tiles across the sphere at tangent velocity `velocity`
/// (`movemethod.py:move` + topological hole-filling and despeckling).
///
/// Returns `(dest_to_sources, next_velocity)` where `dest_to_sources` maps each destination
/// tile index to the source tile indices from `group_tiles` that moved into it.
pub fn move_group(
    grid: &SphericalGrid,
    adjacency: &[Vec<usize>],
    tree: &PointTree,
    group_tiles: &[usize],
    velocity: Vec3,
) -> (BTreeMap<usize, Vec<usize>>, Vec3) {
    if group_tiles.is_empty() {
        return (BTreeMap::new(), velocity);
    }

    let vs: Vec<Vec3> = group_tiles.iter().map(|&t| grid.faces[t].0).collect();
    let avg = average_vec3(&vs);
    let a = if length(avg) > 1e-12 {
        normal(avg)
    } else {
        grid.faces[group_tiles[0]].0
    };

    let speed = length(velocity);
    let raw_axis = cross(a, velocity);
    let axis_len = length(raw_axis);

    if speed <= 1e-12 || axis_len <= 1e-12 {
        let mut map = BTreeMap::new();
        for &t in group_tiles {
            map.insert(t, vec![t]);
        }
        return (map, velocity);
    }

    let axis = [
        raw_axis[0] / axis_len,
        raw_axis[1] / axis_len,
        raw_axis[2] / axis_len,
    ];

    let target_len = group_tiles.len();
    let old_set: BTreeSet<usize> = group_tiles.iter().copied().collect();
    let mut new_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

    for (i, &src_tile) in group_tiles.iter().enumerate() {
        let loc = rotate(vs[i], axis, speed);
        let dest = tree.nearest(loc);
        new_map.entry(dest).or_default().push(src_tile);
    }

    // 1. Unrestricted inverse-rotation hole-filling loop (`movemethod.py:80-90`)
    let mut fill: Vec<usize> = new_map.keys().copied().collect();
    while !fill.is_empty() {
        let mut holes = Vec::new();
        for f in fill {
            for &h in &adjacency[f] {
                if !new_map.contains_key(&h) {
                    let loc = rotate(grid.faces[h].0, axis, -speed);
                    let n = tree.nearest(loc);
                    if old_set.contains(&n) {
                        new_map.insert(h, vec![n]);
                        holes.push(h);
                    }
                }
            }
        }
        fill = holes;
    }

    // 2. Fill any remaining small enclosed interior holes (`<= 32` tiles), close 1-tile-wide cracks,
    //    and prune detached specks before enforcing area conservation.
    fill_enclosed_holes(adjacency, &mut new_map, 32, true);
    prune_detached_specks(adjacency, &mut new_map);

    // 3. Maintain tile count by shaving simple convex boundary tiles (`is_simple_convex_boundary_tile`),
    //    prioritizing newly-added 1-, 2-, and 3-neighbor protrusions farthest from the new centroid.
    for _round in 0..4 {
        if new_map.len() <= target_len {
            break;
        }
        let new_vs: Vec<Vec3> = new_map.keys().map(|&t| grid.faces[t].0).collect();
        let na = average_vec3(&new_vs);

        let mut edge: Vec<(usize, bool, usize, f64)> = new_map
            .keys()
            .copied()
            .filter(|&t| is_simple_convex_boundary_tile(adjacency, &new_map, t))
            .map(|t| {
                let is_new = !old_set.contains(&t);
                let nb_count = adjacency[t]
                    .iter()
                    .filter(|&&nb| new_map.contains_key(&nb))
                    .count();
                (t, is_new, nb_count, dist2(grid.faces[t].0, na))
            })
            .collect();

        if edge.is_empty() {
            break;
        }

        // Sort so `.pop()` removes newly-added (`is_new == true`) protruding tiles with the fewest
        // neighbors in `new_map` first, breaking ties by farthest distance from `na`.
        edge.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.3.total_cmp(&b.3))
        });

        let mut removed_any = false;
        while new_map.len() > target_len {
            let Some((t, _, _, _)) = edge.pop() else {
                break;
            };
            if is_simple_convex_boundary_tile(adjacency, &new_map, t) {
                new_map.remove(&t);
                removed_any = true;
            }
        }
        if !removed_any {
            break;
        }
    }
    prune_detached_specks(adjacency, &mut new_map);

    let vp = rotate(velocity, axis, speed);
    let a_new = rotate(a, axis, speed);
    let vp_dot_a = dot(vp, a_new);
    let tangent = [
        vp[0] - vp_dot_a * a_new[0],
        vp[1] - vp_dot_a * a_new[1],
        vp[2] - vp_dot_a * a_new[2],
    ];
    let tangent_len = length(tangent);
    let next_v = if tangent_len > 1e-12 {
        [
            tangent[0] * (speed / tangent_len),
            tangent[1] * (speed / tangent_len),
            tangent[2] * (speed / tangent_len),
        ]
    } else {
        vp
    };

    (new_map, next_v)
}

/// Locate a corner of a spherical square encompassing a shape with center `c` and farthest point `f`
/// (`splitmethod.py:firstcorner`).
fn first_corner(c: Vec3, f: Vec3) -> Vec3 {
    let cp = cross(c, f);
    let ncp = length(cp);
    if ncp <= 1e-12 {
        return f;
    }
    let d = ncp.atan2(dot(c, f));
    let axis = [cp[0] / ncp, cp[1] / ncp, cp[2] / ncp];
    let f2 = rotate(c, axis, d * 2.0_f64.sqrt());
    rotate(f2, c, PI / 4.0)
}

/// Reassign any small disconnected fragments clipped by the splitting wedge back to the contiguous
/// half they touch so `split_group` always produces two clean, contiguous plates.
fn clean_split_components(
    adjacency: &[Vec<usize>],
    a_set: &mut BTreeSet<usize>,
    b_set: &mut BTreeSet<usize>,
) {
    // 1. Move disconnected fragments of `a_set` into `b_set`.
    let a_comps = connected_components(adjacency, a_set);
    if a_comps.len() > 1 {
        let largest_a = a_comps
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.len())
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        for (idx, comp) in a_comps.into_iter().enumerate() {
            if idx != largest_a {
                for t in comp {
                    a_set.remove(&t);
                    b_set.insert(t);
                }
            }
        }
    }

    // 2. Move disconnected fragments of `b_set` into `a_set`.
    let b_comps = connected_components(adjacency, b_set);
    if b_comps.len() > 1 {
        let largest_b = b_comps
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.len())
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        for (idx, comp) in b_comps.into_iter().enumerate() {
            if idx != largest_b {
                for t in comp {
                    b_set.remove(&t);
                    a_set.insert(t);
                }
            }
        }
    }

    // 3. Smooth interlocking 1-tile zipper teeth/notches along the split boundary between `a_set` and `b_set`.
    for _pass in 0..4 {
        let mut to_b = Vec::new();
        for &t in a_set.iter() {
            let n_a = adjacency[t].iter().filter(|&&nb| a_set.contains(&nb)).count();
            let n_b = adjacency[t].iter().filter(|&&nb| b_set.contains(&nb)).count();
            if n_b > 0 && (n_a <= 1 || (n_a <= 2 && n_b >= 3)) {
                to_b.push(t);
            }
        }
        for t in &to_b {
            a_set.remove(t);
            b_set.insert(*t);
        }

        let mut to_a = Vec::new();
        for &t in b_set.iter() {
            let n_b = adjacency[t].iter().filter(|&&nb| b_set.contains(&nb)).count();
            let n_a = adjacency[t].iter().filter(|&&nb| a_set.contains(&nb)).count();
            if n_a > 0 && (n_b <= 1 || (n_b <= 2 && n_a >= 3)) {
                to_a.push(t);
            }
        }
        for t in &to_a {
            b_set.remove(t);
            a_set.insert(*t);
        }

        if to_b.is_empty() && to_a.is_empty() {
            break;
        }
    }

    // 4. Final connectedness passes on `a_set` and `b_set` after boundary smoothing.
    let a_comps_2 = connected_components(adjacency, a_set);
    if a_comps_2.len() > 1 {
        let largest_a = a_comps_2
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.len())
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        for (idx, comp) in a_comps_2.into_iter().enumerate() {
            if idx != largest_a {
                for t in comp {
                    a_set.remove(&t);
                    b_set.insert(t);
                }
            }
        }
    }
    let b_comps_2 = connected_components(adjacency, b_set);
    if b_comps_2.len() > 1 {
        let largest_b = b_comps_2
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.len())
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        for (idx, comp) in b_comps_2.into_iter().enumerate() {
            if idx != largest_b {
                for t in comp {
                    b_set.remove(&t);
                    a_set.insert(t);
                }
            }
        }
    }
    let a_comps_final = connected_components(adjacency, a_set);
    if a_comps_final.len() > 1 {
        let largest_a = a_comps_final
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.len())
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        for (idx, comp) in a_comps_final.into_iter().enumerate() {
            if idx != largest_a {
                for t in comp {
                    a_set.remove(&t);
                }
            }
        }
    }
}

/// Split a continent group along a random 120-degree wedge (`splitmethod.py:split`).
///
/// Returns `Some(((tiles_a, vel_dir_a), (tiles_b, vel_dir_b)))` if both halves are non-empty,
/// or `None` if the split produced an empty half.
pub fn split_group(
    grid: &SphericalGrid,
    adjacency: &[Vec<usize>],
    group_tiles: &[usize],
    rng: &mut Pcg64Mcg,
) -> Option<((Vec<usize>, Vec3), (Vec<usize>, Vec3))> {
    if group_tiles.len() < 2 {
        return None;
    }

    let c_idx = center_tile(grid, group_tiles);
    let c_vec = grid.faces[c_idx].0;
    let f_idx = farthest_tile(grid, group_tiles, c_vec);
    let f_vec = grid.faces[f_idx].0;

    let b1 = rng.next_f64() * 2.0 * PI;
    let b2 = b1 + 2.0 * PI / 3.0;

    let mut ths = vec![b1];
    for i in 1..=4 {
        let cn = (i as f64) * PI / 2.0;
        if b1 < cn && cn < b2 {
            ths.push(cn);
        }
    }
    ths.push(b2);

    let fc = first_corner(c_vec, f_vec);
    let mut poly_vs: Vec<Vec3> = ths.iter().map(|&th| rotate(fc, c_vec, th)).collect();
    poly_vs.push(c_vec);

    let mid_angle = 0.5 * (b1 + b2);
    let va_raw = rotate(fc, c_vec, mid_angle);
    let va_dot_c = dot(va_raw, c_vec);
    let va_tangent = [
        va_raw[0] - va_dot_c * c_vec[0],
        va_raw[1] - va_dot_c * c_vec[1],
        va_raw[2] - va_dot_c * c_vec[2],
    ];
    let va = if length(va_tangent) > 1e-12 {
        normal(va_tangent)
    } else {
        normal(va_raw)
    };
    let vb = [-va[0], -va[1], -va[2]];

    let poly = SphericalPolygon::new(&poly_vs);
    let mut a_set = BTreeSet::new();
    let mut b_set = BTreeSet::new();
    for &t in group_tiles {
        if poly.contains(grid.faces[t].0) {
            a_set.insert(t);
        } else {
            b_set.insert(t);
        }
    }

    if a_set.is_empty() || b_set.is_empty() {
        return None;
    }

    clean_split_components(adjacency, &mut a_set, &mut b_set);

    let min_half = 12.min(group_tiles.len() / 4).max(4);
    if a_set.len() < min_half || b_set.len() < min_half {
        None
    } else {
        Some(((a_set.into_iter().collect(), va), (b_set.into_iter().collect(), vb)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_and_split_group() {
        let grid = SphericalGrid::with_size(4);
        let adj = grid.adjacency();
        let pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&pts);

        // Take a cap of tiles in the +X hemisphere
        let initial_tiles: Vec<usize> = (0..grid.faces.len())
            .filter(|&i| grid.faces[i].0[0] > 0.65)
            .collect();
        assert!(initial_tiles.len() > 20);

        let mut rng = Pcg64Mcg::from_seed_u64(123);
        let ((a, va), (b, vb)) =
            split_group(&grid, &adj, &initial_tiles, &mut rng).expect("split should produce two non-empty groups");
        assert!(!a.is_empty() && !b.is_empty());
        assert_eq!(a.len() + b.len(), initial_tiles.len());
        assert!((dot(va, vb) + 1.0).abs() < 1e-9);

        // Both halves must be single connected components (no detached specks)
        let a_set: BTreeSet<usize> = a.iter().copied().collect();
        let b_set: BTreeSet<usize> = b.iter().copied().collect();
        assert_eq!(connected_components(&adj, &a_set).len(), 1);
        assert_eq!(connected_components(&adj, &b_set).len(), 1);

        // Advect group `a` at speed 0.08 rad
        let vel = [va[0] * 0.08, va[1] * 0.08, va[2] * 0.08];
        let (moved_map, next_v) = move_group(&grid, &adj, &tree, &a, vel);
        assert!(!moved_map.is_empty());
        let moved_set: BTreeSet<usize> = moved_map.keys().copied().collect();
        assert_eq!(connected_components(&adj, &moved_set).len(), 1);
        assert!((length(next_v) - 0.08).abs() < 1e-3);
    }
}
