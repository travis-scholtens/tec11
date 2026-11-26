use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tec11_math_grid::{dot, SphericalGrid, Vec3};

/// Simple union-find (`disjoint.py`) over indices `0..n`.
#[derive(Clone, Debug)]
pub struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    pub fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            let p = self.parent[x];
            self.parent[x] = self.parent[p];
            x = p;
        }
        x
    }

    pub fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        if self.rank[rx] < self.rank[ry] {
            self.parent[rx] = ry;
        } else if self.rank[rx] > self.rank[ry] {
            self.parent[ry] = rx;
        } else {
            self.parent[ry] = rx;
            self.rank[rx] += 1;
        }
    }
}

/// Find the largest connected component of sea tiles (`elevation <= 0.0`),
/// matching `populationmethod.mainocean(seatiles, adj)` (`populationmethod.py:10-22`).
pub fn main_ocean(elevations: &[f32], adjacency: &[Vec<usize>]) -> BTreeSet<usize> {
    let n = elevations.len();
    let sea_tiles: Vec<usize> = (0..n).filter(|&t| elevations[t] <= 0.0).collect();
    if sea_tiles.is_empty() {
        return BTreeSet::new();
    }

    let mut dsu = DisjointSet::new(n);
    for &t in &sea_tiles {
        for &nb in &adjacency[t] {
            if elevations[nb] <= 0.0 {
                dsu.union(t, nb);
            }
        }
    }

    let mut bodies: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for &t in &sea_tiles {
        let root = dsu.find(t);
        bodies.entry(root).or_default().insert(t);
    }

    bodies
        .into_values()
        .max_by_key(|b| b.len())
        .unwrap_or_default()
}

/// Metadata for a contiguous continental realm or isolated oceanic archipelago.
#[derive(Clone, Debug, PartialEq)]
pub struct ContinentInfo {
    pub id: usize,
    pub name: String,
    pub adjectival_prefix: String,
    pub tile_count: usize,
    pub centroid: Vec3,
    pub is_island_group: bool,
}

/// Metadata for a major ocean basin or enclosed sea.
#[derive(Clone, Debug, PartialEq)]
pub struct OceanBasinInfo {
    pub id: usize,
    pub name: String,
    pub tile_count: usize,
    pub centroid: Vec3,
}

/// Metadata for a regional coastal sea, marginal bay, or enclosed inland sea.
#[derive(Clone, Debug, PartialEq)]
pub struct SeaInfo {
    pub id: usize,
    pub name: String,
    pub tile_count: usize,
    pub centroid: Vec3,
}

/// Metadata for a contiguous mountain range / cordillera on land.
#[derive(Clone, Debug, PartialEq)]
pub struct MountainRangeInfo {
    pub id: usize,
    pub name: String,
    pub tile_count: usize,
    pub max_elevation_km: f32,
    pub centroid: Vec3,
}

/// Partition of a planet's surface into named continents/island realms, ocean basins, coastal seas, and mountain ranges.
#[derive(Clone, Debug, Default)]
pub struct PlanetaryGeography {
    pub continents: Vec<ContinentInfo>,
    pub oceans: Vec<OceanBasinInfo>,
    pub seas: Vec<SeaInfo>,
    pub mountain_ranges: Vec<MountainRangeInfo>,
    /// Per-tile continent index (`Some(id)` for land tiles `elevation > 0.0`, `None` for sea tiles).
    pub tile_continent: Vec<Option<usize>>,
    /// Per-tile ocean basin index (`Some(id)` for sea tiles `elevation <= 0.0`, `None` for land tiles).
    pub tile_ocean: Vec<Option<usize>>,
    /// Per-tile regional coastal/enclosed sea index (`Some(id)` for near-shore or enclosed sea tiles).
    pub tile_sea: Vec<Option<usize>>,
    /// Per-tile mountain range index (`Some(id)` for highland/mountain land tiles).
    pub tile_mountain_range: Vec<Option<usize>>,
}

impl PlanetaryGeography {
    pub fn continent_name(&self, tile: usize) -> Option<&str> {
        self.tile_continent
            .get(tile)
            .copied()
            .flatten()
            .and_then(|cid| self.continents.get(cid))
            .map(|c| c.name.as_str())
    }

    pub fn ocean_name(&self, tile: usize) -> Option<&str> {
        self.tile_ocean
            .get(tile)
            .copied()
            .flatten()
            .and_then(|oid| self.oceans.get(oid))
            .map(|o| o.name.as_str())
    }

    pub fn sea_name(&self, tile: usize) -> Option<&str> {
        self.tile_sea
            .get(tile)
            .copied()
            .flatten()
            .and_then(|sid| self.seas.get(sid))
            .map(|s| s.name.as_str())
    }

    pub fn mountain_range_name(&self, tile: usize) -> Option<&str> {
        self.tile_mountain_range
            .get(tile)
            .copied()
            .flatten()
            .and_then(|mid| self.mountain_ranges.get(mid))
            .map(|m| m.name.as_str())
    }
}

fn compute_tile_set_centroid(tiles: &[usize], grid: &SphericalGrid) -> Vec3 {
    if tiles.is_empty() {
        return [1.0, 0.0, 0.0];
    }
    let sum = tiles.iter().fold([0.0f64; 3], |acc, &t| {
        let p = grid.faces[t].0;
        [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
    });
    let len = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    if len > 1e-9 {
        [sum[0] / len, sum[1] / len, sum[2] / len]
    } else {
        grid.faces[tiles[0]].0
    }
}

/// Identify all continents (connected landmasses + associated coastal islands) and ocean basins
/// on `grid` given `elevations` (`> 0.0` is land, `<= 0.0` is ocean/sea).
pub fn identify_continents_and_oceans(
    grid: &SphericalGrid,
    elevations: &[f32],
) -> PlanetaryGeography {
    let adj = grid.adjacency();
    identify_continents_and_oceans_with_adj(grid, &adj, elevations)
}

/// Identify all continents and ocean basins on `grid` reusing a precomputed `adj` table.
pub fn identify_continents_and_oceans_with_adj(
    grid: &SphericalGrid,
    adj: &[Vec<usize>],
    elevations: &[f32],
) -> PlanetaryGeography {
    let n = elevations.len().min(grid.faces.len());
    if n == 0 {
        return PlanetaryGeography::default();
    }

    // --- 1. Continents & Island Realms ---
    let mut land_dsu = DisjointSet::new(n);
    for t in 0..n {
        if elevations[t] > 0.0 {
            for &nb in &adj[t] {
                if nb < n && elevations[nb] > 0.0 {
                    land_dsu.union(t, nb);
                }
            }
        }
    }

    let mut land_bodies: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for t in 0..n {
        if elevations[t] > 0.0 {
            land_bodies.entry(land_dsu.find(t)).or_default().push(t);
        }
    }

    let mut sorted_land: Vec<Vec<usize>> = land_bodies.into_values().collect();
    sorted_land.sort_by_key(|tiles| (Reverse(tiles.len()), tiles.iter().copied().min().unwrap_or(0)));

    let min_continent_tiles = (n / 240).max(8);
    let mut major_realms: Vec<(Vec<usize>, bool)> = Vec::new();
    let mut minor_islands: Vec<Vec<usize>> = Vec::new();

    for (idx, tiles) in sorted_land.into_iter().enumerate() {
        if tiles.len() >= min_continent_tiles || (idx == 0 && major_realms.is_empty()) {
            major_realms.push((tiles, false));
        } else {
            minor_islands.push(tiles);
        }
    }

    // Associate coastal islands (within ~25 deg / cos >= 0.90 of a major continent tile) with that continent;
    // group isolated oceanic islands into archipelago realms.
    let mut oceanic_groups: Vec<(Vec<usize>, Vec3)> = Vec::new();
    for island_tiles in minor_islands {
        let island_center = compute_tile_set_centroid(&island_tiles, grid);
        let nearest_major = major_realms
            .iter()
            .enumerate()
            .filter_map(|(m_idx, (m_tiles, _))| {
                let best_dot = m_tiles
                    .iter()
                    .map(|&mt| dot(island_center, grid.faces[mt].0))
                    .fold(f64::NEG_INFINITY, f64::max);
                (best_dot >= 0.90).then_some((m_idx, best_dot))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((m_idx, _)) = nearest_major {
            major_realms[m_idx].0.extend(island_tiles);
        } else {
            let nearest_arch = oceanic_groups
                .iter()
                .enumerate()
                .filter_map(|(a_idx, (_, a_center))| {
                    let d = dot(island_center, *a_center);
                    (d >= 0.88).then_some((a_idx, d))
                })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((a_idx, _)) = nearest_arch {
                oceanic_groups[a_idx].0.extend(island_tiles);
                oceanic_groups[a_idx].1 = compute_tile_set_centroid(&oceanic_groups[a_idx].0, grid);
            } else {
                oceanic_groups.push((island_tiles, island_center));
            }
        }
    }

    for (arch_tiles, _) in oceanic_groups {
        major_realms.push((arch_tiles, true));
    }

    major_realms.sort_by_key(|(tiles, is_island)| {
        (
            *is_island,
            Reverse(tiles.len()),
            tiles.iter().copied().min().unwrap_or(0),
        )
    });

    let mut tile_continent = vec![None; n];
    let continents: Vec<ContinentInfo> = major_realms
        .into_iter()
        .enumerate()
        .map(|(cid, (tiles, is_island_group))| {
            for &t in &tiles {
                tile_continent[t] = Some(cid);
            }
            let centroid = compute_tile_set_centroid(&tiles, grid);
            let name = if is_island_group {
                format!("Archipelago {}", cid + 1)
            } else {
                format!("Continent {}", cid + 1)
            };
            let adjectival_prefix = name.clone();
            ContinentInfo {
                id: cid,
                name,
                adjectival_prefix,
                tile_count: tiles.len(),
                centroid,
                is_island_group,
            }
        })
        .collect();

    // --- 2. Ocean Basins & Enclosed Seas ---
    let mut sea_dsu = DisjointSet::new(n);
    for t in 0..n {
        if elevations[t] <= 0.0 {
            for &nb in &adj[t] {
                if nb < n && elevations[nb] <= 0.0 {
                    sea_dsu.union(t, nb);
                }
            }
        }
    }

    let mut sea_bodies: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for t in 0..n {
        if elevations[t] <= 0.0 {
            sea_bodies.entry(sea_dsu.find(t)).or_default().push(t);
        }
    }

    let mut sorted_seas: Vec<Vec<usize>> = sea_bodies.into_values().collect();
    sorted_seas.sort_by_key(|tiles| (Reverse(tiles.len()), tiles.iter().copied().min().unwrap_or(0)));

    // Compute BFS distance from coast for all sea tiles so large interconnected oceans
    // can be partitioned into 3-4 major ocean basins around oceanic poles of inaccessibility.
    let mut coast_dist = vec![u16::MAX; n];
    let mut bfs_q = VecDeque::new();
    for t in 0..n {
        if elevations[t] <= 0.0 && adj[t].iter().any(|&nb| nb < n && elevations[nb] > 0.0)
        {
            coast_dist[t] = 1;
            bfs_q.push_back(t);
        }
    }
    while let Some(curr) = bfs_q.pop_front() {
        let next_d = coast_dist[curr].saturating_add(1);
        for &nb in &adj[curr] {
            if nb < n && elevations[nb] <= 0.0 && coast_dist[nb] == u16::MAX {
                coast_dist[nb] = next_d;
                bfs_q.push_back(nb);
            }
        }
    }

    let mut basin_tiles_list: Vec<Vec<usize>> = Vec::new();
    for sea_comp in sorted_seas {
        if sea_comp.len() >= (n / 3).max(60) {
            let target_basins = ((sea_comp.len() * 4) / n.max(1)).clamp(2, 4);
            let mut ranked_poles = sea_comp.clone();
            ranked_poles.sort_by_key(|&t| (Reverse(coast_dist[t]), t));
            let mut seeds: Vec<usize> = Vec::new();
            for &cand in &ranked_poles {
                if seeds
                    .iter()
                    .all(|&s| dot(grid.faces[cand].0, grid.faces[s].0) < 0.35)
                {
                    seeds.push(cand);
                    if seeds.len() >= target_basins {
                        break;
                    }
                }
            }
            if seeds.len() <= 1 {
                basin_tiles_list.push(sea_comp);
            } else {
                let mut local_owner = BTreeMap::new();
                let mut q = VecDeque::new();
                for (b_idx, &s_tile) in seeds.iter().enumerate() {
                    local_owner.insert(s_tile, b_idx);
                    q.push_back(s_tile);
                }
                while let Some(curr) = q.pop_front() {
                    let owner = local_owner[&curr];
                    for &nb in &adj[curr] {
                        if nb < n && elevations[nb] <= 0.0 && !local_owner.contains_key(&nb) {
                            local_owner.insert(nb, owner);
                            q.push_back(nb);
                        }
                    }
                }
                let mut split_basins = vec![Vec::new(); seeds.len()];
                for t in sea_comp {
                    let b_idx = local_owner.get(&t).copied().unwrap_or(0);
                    split_basins[b_idx].push(t);
                }
                for b in split_basins {
                    if !b.is_empty() {
                        basin_tiles_list.push(b);
                    }
                }
            }
        } else {
            basin_tiles_list.push(sea_comp);
        }
    }

    basin_tiles_list
        .sort_by_key(|tiles| (Reverse(tiles.len()), tiles.iter().copied().min().unwrap_or(0)));

    let mut tile_ocean = vec![None; n];
    let oceans: Vec<OceanBasinInfo> = basin_tiles_list
        .into_iter()
        .enumerate()
        .map(|(oid, tiles)| {
            for &t in &tiles {
                tile_ocean[t] = Some(oid);
            }
            let centroid = compute_tile_set_centroid(&tiles, grid);
            let name = format!("Ocean {}", oid + 1);
            OceanBasinInfo {
                id: oid,
                name,
                tile_count: tiles.len(),
                centroid,
            }
        })
        .collect();

    // --- 3. Regional Coastal & Enclosed Seas (`coast_dist <= 2`) ---
    let mut coastal_sea_tiles: Vec<usize> = (0..n)
        .filter(|&t| elevations[t] <= 0.0 && coast_dist[t] <= 2)
        .collect();
    coastal_sea_tiles.sort_unstable();
    let mut sea_seeds: Vec<usize> = Vec::new();
    for &cand in &coastal_sea_tiles {
        if sea_seeds
            .iter()
            .all(|&s| dot(grid.faces[cand].0, grid.faces[s].0) < 0.88)
        {
            sea_seeds.push(cand);
            if sea_seeds.len() >= 18 {
                break;
            }
        }
    }
    let mut tile_sea = vec![None; n];
    let mut sea_groups: Vec<Vec<usize>> = vec![Vec::new(); sea_seeds.len()];
    if !sea_seeds.is_empty() {
        for &t in &coastal_sea_tiles {
            let best_s = sea_seeds
                .iter()
                .enumerate()
                .max_by(|&(_, &sa), &(_, &sb)| {
                    dot(grid.faces[t].0, grid.faces[sa].0)
                        .partial_cmp(&dot(grid.faces[t].0, grid.faces[sb].0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            sea_groups[best_s].push(t);
        }
    }
    sea_groups.retain(|g| !g.is_empty());
    sea_groups.sort_by_key(|tiles| (Reverse(tiles.len()), tiles.iter().copied().min().unwrap_or(0)));
    let seas: Vec<SeaInfo> = sea_groups
        .into_iter()
        .enumerate()
        .map(|(sid, tiles)| {
            for &t in &tiles {
                tile_sea[t] = Some(sid);
            }
            let centroid = compute_tile_set_centroid(&tiles, grid);
            let name = format!("Sea {}", sid + 1);
            SeaInfo {
                id: sid,
                name,
                tile_count: tiles.len(),
                centroid,
            }
        })
        .collect();

    // --- 4. Mountain Ranges (`elevations[t] >= 1.65 km`) ---
    let mut mtn_dsu = DisjointSet::new(n);
    for t in 0..n {
        if elevations[t] >= 1.65 {
            for &nb in &adj[t] {
                if nb < n && elevations[nb] >= 1.65 {
                    mtn_dsu.union(t, nb);
                }
            }
        }
    }
    let mut mtn_bodies: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for t in 0..n {
        if elevations[t] >= 1.65 {
            mtn_bodies.entry(mtn_dsu.find(t)).or_default().push(t);
        }
    }
    let mut sorted_mtns: Vec<Vec<usize>> = mtn_bodies
        .into_values()
        .filter(|tiles| tiles.len() >= 2)
        .collect();
    sorted_mtns.sort_by_key(|tiles| (Reverse(tiles.len()), tiles.iter().copied().min().unwrap_or(0)));
    sorted_mtns.truncate(24);
    let mut tile_mountain_range = vec![None; n];
    let mountain_ranges: Vec<MountainRangeInfo> = sorted_mtns
        .into_iter()
        .enumerate()
        .map(|(mid, tiles)| {
            let mut max_elev = 0.0f32;
            for &t in &tiles {
                tile_mountain_range[t] = Some(mid);
                if elevations[t] > max_elev {
                    max_elev = elevations[t];
                }
            }
            let centroid = compute_tile_set_centroid(&tiles, grid);
            let name = format!("Mountain Range {}", mid + 1);
            MountainRangeInfo {
                id: mid,
                name,
                tile_count: tiles.len(),
                max_elevation_km: max_elev,
                centroid,
            }
        })
        .collect();

    PlanetaryGeography {
        continents,
        oceans,
        seas,
        mountain_ranges,
        tile_continent,
        tile_ocean,
        tile_sea,
        tile_mountain_range,
    }
}
