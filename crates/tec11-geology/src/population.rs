use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{dot, Pcg64Mcg, SphericalGrid, Vec3};
use tec11_proto::PlanetSnapshotExt;

use crate::language::Language;
use crate::rivers::trace_coarse_rivers;
use crate::rock::{Rock, RockKind, RockLayer};
use crate::tectonics::TectonicsSimulation;
use crate::tile::TileGeology;

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

fn format_adjectival_prefix(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return "continental".to_string();
    }
    if lower.ends_with('n') || lower.ends_with("ic") || lower.ends_with("sh") {
        lower
    } else if lower.ends_with('a') || lower.ends_with('e') || lower.ends_with('o') || lower.ends_with('u') {
        format!("{lower}n")
    } else if lower.ends_with('i') {
        format!("{lower}an")
    } else {
        format!("{lower}an")
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

fn generate_geographic_name(
    seed_tag: u64,
    rank: usize,
    centroid: Vec3,
    used_names: &mut BTreeSet<String>,
) -> String {
    let qx = ((centroid[0] + 1.0) * 500.0).round() as u64;
    let qy = ((centroid[1] + 1.0) * 500.0).round() as u64;
    let qz = ((centroid[2] + 1.0) * 500.0).round() as u64;
    let seed = seed_tag
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((rank as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(qx << 20)
        .wrapping_add(qy << 10)
        .wrapping_add(qz);
    let mut rng = Pcg64Mcg::new(seed as u128 | 1);
    let lang = Language::generate(&mut rng);
    let mut seen_keys = BTreeSet::new();
    let mut seen_words = BTreeSet::new();
    for _ in 0..32 {
        let mut word = lang.generate_word_unique(&mut seen_keys, &seen_words, &mut rng);
        if word.syllables.len() < 2 {
            let extra = lang.generate_syllable(&mut rng);
            word.syllables.push(extra);
        }
        seen_words.insert(word.clone());
        let written = word.write(true);
        if written.chars().count() >= 3 && !used_names.contains(&written) {
            used_names.insert(written.clone());
            return written;
        }
    }
    let fallback = format!("Terra{}", rank + 1);
    used_names.insert(fallback.clone());
    fallback
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

    let mut used_names = BTreeSet::new();

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
            let name = generate_geographic_name(0x434F_4E54, cid, centroid, &mut used_names);
            let adjectival_prefix = format_adjectival_prefix(&name);
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
            let name = generate_geographic_name(0x4F43_4541, oid, centroid, &mut used_names);
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
            let name = generate_geographic_name(0x5345_4153, sid, centroid, &mut used_names);
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
            let name = generate_geographic_name(0x4D54_4E53, mid, centroid, &mut used_names);
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

/// Select a random Eden tile (`koeppen == Aw == 3` bordering `main_ocean`),
/// matching `populationmethod.eden(tiles, adj, name)` (`populationmethod.py:24-29`).
/// If no `Aw` coastal tile exists on a custom planet, falls back to any habitable coastal tile.
pub fn eden_tile(
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    rng: &mut Pcg64Mcg,
) -> Option<usize> {
    let ocean = main_ocean(elevations, adjacency);
    let mut candidates: Vec<usize> = (0..elevations.len())
        .filter(|&t| {
            elevations[t] > 0.0
                && koeppen_ids[t] == 3 // Aw
                && adjacency[t].iter().any(|nb| ocean.contains(nb))
        })
        .collect();

    if candidates.is_empty() {
        candidates = (0..elevations.len())
            .filter(|&t| {
                is_habitable(elevations[t], koeppen_ids[t])
                    && adjacency[t].iter().any(|nb| ocean.contains(nb))
            })
            .collect();
    }

    if candidates.is_empty() {
        None
    } else {
        let idx = (rng.next_u64() as usize) % candidates.len();
        Some(candidates[idx])
    }
}

/// Passability on land (`passability.passable(t, agricultural)`, `passability.py:1-4`):
/// Pre-agricultural people cannot pass through desert (`BW = 5`) or polar (`EF = 12`, `ET = 13`);
/// agricultural people are only blocked by ice cap (`EF = 12`).
#[inline]
pub fn is_passable(elevation: f32, koeppen: u8, agricultural: bool) -> bool {
    if elevation <= 0.0 || koeppen == 0 {
        return false;
    }
    if agricultural {
        koeppen != 12 // EF
    } else {
        koeppen != 5 && koeppen != 12 && koeppen != 13 // BW, EF, ET
    }
}

/// Hoppability across sea or passable land (`passability.hoppable(t, agricultural)`, `passability.py:6-7`).
#[inline]
pub fn is_hoppable(elevation: f32, koeppen: u8, agricultural: bool) -> bool {
    elevation <= 0.0 || is_passable(elevation, koeppen, agricultural)
}

/// Expand a frontier set of tiles by 1 step under `condition`,
/// matching `passability.expand(ts, adj, agricultural, condition)` (`passability.py:9-14`).
pub fn expand_within<F>(
    ts: &BTreeSet<usize>,
    adjacency: &[Vec<usize>],
    condition: F,
) -> BTreeSet<usize>
where
    F: Fn(usize) -> bool,
{
    let mut next_adjs = BTreeSet::new();
    for &n in ts {
        if condition(n) {
            for &a in &adjacency[n] {
                if condition(a) {
                    next_adjs.insert(a);
                }
            }
        }
    }
    next_adjs
}

/// Tiles reachable in exact `d`-step expansion under `condition`,
/// matching `passability.within(t, adj, d, agricultural, condition)` (`passability.py:16-20`).
pub fn within_steps<F>(
    start: usize,
    adjacency: &[Vec<usize>],
    d: usize,
    condition: F,
) -> BTreeSet<usize>
where
    F: Fn(usize) -> bool,
{
    let mut adjs = BTreeSet::new();
    adjs.insert(start);
    for _ in 0..d {
        adjs = expand_within(&adjs, adjacency, &condition);
    }
    adjs
}

/// Check if land tile `t` is within `d` hoppable steps of sea (`populationmethod.nearcoast`, `populationmethod.py:31-32`).
pub fn near_coast(
    t: usize,
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    d: usize,
) -> bool {
    if elevations[t] <= 0.0 || d == 0 {
        return false;
    }
    if !is_hoppable(elevations[t], koeppen_ids[t], false) {
        return false;
    }
    // Fast allocation-free bounded BFS for small coastal_proximity (typically d = 2 or 3)
    let mut visited = [usize::MAX; 64];
    let mut v_len = 1usize;
    visited[0] = t;
    let mut curr_start = 0usize;
    let mut curr_end = 1usize;

    for _step in 0..d {
        let next_start = v_len;
        for idx in curr_start..curr_end {
            let u = visited[idx];
            for &nb in &adjacency[u] {
                if elevations[nb] <= 0.0 {
                    return true;
                }
                if is_hoppable(elevations[nb], koeppen_ids[nb], false)
                    && !visited[..v_len].contains(&nb)
                {
                    if v_len < visited.len() {
                        visited[v_len] = nb;
                        v_len += 1;
                    }
                }
            }
        }
        if v_len == next_start {
            break;
        }
        curr_start = next_start;
        curr_end = v_len;
    }
    false
}

/// Habitable by pre-agricultural people (`Aw` or temperate `C*` / cold `D*`),
/// matching `populationmethod.habitable(t)` (`populationmethod.py:34-35`).
#[inline]
pub fn is_habitable(elevation: f32, koeppen: u8) -> bool {
    elevation > 0.0 && (koeppen == 3 || (6..=11).contains(&koeppen))
}

/// Farmable by agricultural people (`Am`, `Aw`, `BS`, or `C*`/`D*`),
/// matching `populationmethod.farmable(t)` (`populationmethod.py:37-39`).
#[inline]
pub fn is_farmable(elevation: f32, koeppen: u8) -> bool {
    elevation > 0.0 && (matches!(koeppen, 2..=4) || (6..=11).contains(&koeppen))
}

/// Squattable by pre-agricultural people (coastal habitat or on a river),
/// matching `populationmethod.squattable(t, adj, coastalproximity, rivers)` (`populationmethod.py:41-43`).
pub fn is_squattable(
    t: usize,
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    coastal_proximity: usize,
    river_tiles: &[bool],
) -> bool {
    (near_coast(t, elevations, koeppen_ids, adjacency, coastal_proximity)
        && is_habitable(elevations[t], koeppen_ids[t]))
        || river_tiles[t]
}

/// Paleolithic population density in thousands per coarse tile (`populationlevel.paleolithic`, `populationlevel.py:1-4`).
#[inline]
pub fn paleolithic_density(koeppen: u8) -> f32 {
    if (1..=3).contains(&koeppen) {
        35.0
    } else {
        5.0
    }
}

/// Agricultural population density in thousands per coarse tile (`populationlevel.withagriculture`, `populationlevel.py:6-11`).
#[inline]
pub fn agricultural_density(koeppen: u8) -> f32 {
    if (1..=3).contains(&koeppen) || (6..=8).contains(&koeppen) {
        150.0
    } else if (9..=11).contains(&koeppen) {
        100.0
    } else {
        50.0
    }
}

/// Count coarse tile population in thousands (`populationlevel.count`, `populationlevel.py:13-14`).
#[inline]
pub fn tile_population_thousands(koeppen: u8, is_agricultural: bool) -> f32 {
    if is_agricultural {
        agricultural_density(koeppen)
    } else {
        paleolithic_density(koeppen)
    }
}

/// Count coarse tile population in thousands including freshwater fishing (`is_river_fishing`)
/// and coastal/marine fishing (`is_sea_fishing`) carrying capacity bonuses.
#[inline]
pub fn tile_population_thousands_with_water_tech(
    koeppen: u8,
    is_agricultural: bool,
    is_river_fishing: bool,
    is_sea_fishing: bool,
) -> f32 {
    let base = tile_population_thousands(koeppen, is_agricultural);
    let river_bonus = if is_river_fishing {
        if is_agricultural { 35.0 } else { 25.0 }
    } else {
        0.0
    };
    let sea_bonus = if is_sea_fishing {
        if is_agricultural { 45.0 } else { 30.0 }
    } else {
        0.0
    };
    base + river_bonus + sea_bonus
}

/// Expand `populated` map (`tile_idx -> heritage_id`) by one migration step,
/// matching `populationmethod.expandpopulation` (`populationmethod.py:45-72`).
pub fn expand_population(
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    river_tiles: &[bool],
    populated: &mut BTreeMap<usize, u32>,
    agricultural_heritages: &BTreeSet<u32>,
    travel_range: usize,
    coastal_proximity: usize,
    cmemo: &mut BTreeMap<(usize, bool), bool>,
) -> bool {
    let empty_set = BTreeSet::new();
    expand_population_with_technologies(
        elevations,
        koeppen_ids,
        adjacency,
        river_tiles,
        populated,
        agricultural_heritages,
        &empty_set,
        &empty_set,
        travel_range,
        coastal_proximity,
        cmemo,
    )
}

/// Expand `populated` map by one migration step taking into account `agricultural_heritages`,
/// `rivergoing_heritages` (extended range along river corridors), and `seafaring_heritages`
/// (extended range across coastal and sea tiles).
pub fn expand_population_with_technologies(
    elevations: &[f32],
    koeppen_ids: &[u8],
    adjacency: &[Vec<usize>],
    river_tiles: &[bool],
    populated: &mut BTreeMap<usize, u32>,
    agricultural_heritages: &BTreeSet<u32>,
    rivergoing_heritages: &BTreeSet<u32>,
    seafaring_heritages: &BTreeSet<u32>,
    travel_range: usize,
    coastal_proximity: usize,
    cmemo: &mut BTreeMap<(usize, bool), bool>,
) -> bool {
    if populated.is_empty() {
        return false;
    }
    let _ = cmemo;
    let num_tiles = elevations.len();

    // Flat O(1) lookup table for currently populated tiles
    let mut is_pop = vec![false; num_tiles];
    for &t in populated.keys() {
        if t < num_tiles {
            is_pop[t] = true;
        }
    }

    // Lazily memoized candidate suitability per tile for the 3 technology modes:
    //   0 = uncomputed, 1 = false, 2 = true
    let mut farm_memo = vec![0u8; num_tiles];
    let mut sea_memo = vec![0u8; num_tiles];
    let mut paleo_memo = vec![0u8; num_tiles];
    let eff_sea_coast_prox = coastal_proximity + 1;

    // Reusable BFS buffers and visited generation stamps to avoid all per-tile BTreeSet allocations
    let mut visited_stamp = vec![0u32; num_tiles];
    let mut stamp = 0u32;
    let mut curr_frontier: Vec<usize> = Vec::with_capacity(64);
    let mut next_frontier: Vec<usize> = Vec::with_capacity(64);

    let mut frontier: BTreeMap<usize, u32> = BTreeMap::new();

    for (&t, &heritage_id) in populated.iter() {
        let farms = agricultural_heritages.contains(&heritage_id);
        let rivers_tech = rivergoing_heritages.contains(&heritage_id);
        let seas_tech = seafaring_heritages.contains(&heritage_id);
        let eff_range = travel_range
            + usize::from(rivers_tech && river_tiles.get(t).copied().unwrap_or(false))
            + usize::from(seas_tech);

        stamp = stamp.wrapping_add(1);
        if stamp == 0 {
            visited_stamp.fill(0);
            stamp = 1;
        }
        visited_stamp[t] = stamp;

        curr_frontier.clear();
        for &nb in &adjacency[t] {
            if visited_stamp[nb] != stamp {
                visited_stamp[nb] = stamp;
                curr_frontier.push(nb);
            }
        }

        let mut distance = 0usize;
        while distance < eff_range && !curr_frontier.is_empty() {
            let mut added = false;
            for &n in &curr_frontier {
                if !is_pop[n] {
                    let cand = if farms {
                        let cached = farm_memo[n];
                        if cached != 0 {
                            cached == 2
                        } else {
                            let val = is_farmable(elevations[n], koeppen_ids[n]);
                            farm_memo[n] = if val { 2 } else { 1 };
                            val
                        }
                    } else if seas_tech {
                        let cached = sea_memo[n];
                        if cached != 0 {
                            cached == 2
                        } else {
                            let val = is_squattable(
                                n,
                                elevations,
                                koeppen_ids,
                                adjacency,
                                eff_sea_coast_prox,
                                river_tiles,
                            );
                            sea_memo[n] = if val { 2 } else { 1 };
                            val
                        }
                    } else {
                        let cached = paleo_memo[n];
                        if cached != 0 {
                            cached == 2
                        } else {
                            let val = is_squattable(
                                n,
                                elevations,
                                koeppen_ids,
                                adjacency,
                                coastal_proximity,
                                river_tiles,
                            );
                            paleo_memo[n] = if val { 2 } else { 1 };
                            val
                        }
                    };
                    if cand {
                        frontier.insert(n, heritage_id);
                        added = true;
                    }
                }
            }
            if added || distance + 1 >= eff_range {
                break;
            }

            next_frontier.clear();
            for &u in &curr_frontier {
                let u_ok = (rivers_tech && river_tiles.get(u).copied().unwrap_or(false))
                    || is_hoppable(elevations[u], koeppen_ids[u], farms);
                if u_ok {
                    for &a in &adjacency[u] {
                        if visited_stamp[a] != stamp {
                            let a_ok = (rivers_tech
                                && river_tiles.get(a).copied().unwrap_or(false))
                                || is_hoppable(elevations[a], koeppen_ids[a], farms);
                            if a_ok {
                                visited_stamp[a] = stamp;
                                next_frontier.push(a);
                            }
                        }
                    }
                }
            }
            std::mem::swap(&mut curr_frontier, &mut next_frontier);
            distance += 1;
        }
    }

    if frontier.is_empty() {
        return false;
    }
    for (k, v) in frontier {
        populated.insert(k, v);
    }
    true
}

/// Standalone Population sandbox (`populationsimulation.py`, `populationdisplay.py`, `populationpresenter.py`).
pub struct PopulationSandbox {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    tiles: Vec<TileGeology>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    river_tiles: Vec<bool>,
    populated: BTreeMap<usize, u32>,
    popcache: BTreeMap<(usize, bool), bool>,
    coast_prox: usize,
    travel_range: usize,
    ticks: u32,
    rng: Pcg64Mcg,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
}

impl PopulationSandbox {
    pub fn new_earth(coast_prox: usize, travel_range: usize, seed: u128) -> Self {
        let grid = SphericalGrid::with_size(6);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let mut tiles = Vec::with_capacity(n);

        for &elev_m in EARTH_SIZE6_ELEVATIONS_M.iter() {
            let elev_km = (elev_m as f32) / 550.0;
            if elev_km > 0.0 {
                let mut t = TileGeology {
                    bottom: -1.0,
                    mountainosity: (elev_km * 2.0).clamp(0.0, 20.0),
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    layers: vec![RockLayer {
                        rock: Rock {
                            kind: RockKind::Sedimentary,
                            name: "sandstone",
                            felsity: 0.55,
                            toughness: 0.5,
                            clasticity: Some(1.0),
                            calcity: None,
                            bogginess: None,
                            quartzity: None,
                            sulfidity: None,
                            salinity: None,
                        },
                        thickness: elev_km + 1.0,
                    }],
                };
                t.limit();
                tiles.push(t);
            } else {
                let bathy_km = ((elev_m as f32) / 1000.0).clamp(-7.0, -0.2);
                tiles.push(TileGeology {
                    bottom: bathy_km - 5.0,
                    mountainosity: 0.0,
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    layers: vec![RockLayer {
                        rock: Rock::extrusive(0.5),
                        thickness: 5.0,
                    }],
                });
            }
        }

        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);
        let mut s = Self {
            grid,
            adjacency,
            tiles,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_precip_norm: vec![0.0; n],
            river_tiles: vec![false; n],
            populated: BTreeMap::new(),
            popcache: BTreeMap::new(),
            coast_prox,
            travel_range,
            ticks: 0,
            rng: Pcg64Mcg::new(seed),
            custom_geoticks: None,
            custom_land_percent: None,
        };
        s.recompute_climate_and_rivers();
        s
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(self.coast_prox, self.travel_range, 42);
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(tec_sim) = TectonicsSimulation::from_snapshot(&snap, 42) else {
            return false;
        };
        let grid = tec_sim.grid().clone();
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        self.grid = grid.clone();
        self.adjacency = adjacency;
        self.tiles = tec_sim.tiles().to_vec();
        self.climate_model = ClimateModel::new_for_tectonics(grid, 3, 1.0, 23.0);
        self.tile_koeppen = vec![0; n];
        self.tile_life = vec![0.0; n];
        self.tile_precip_norm = vec![0.0; n];
        self.river_tiles = vec![false; n];
        self.populated.clear();
        self.popcache.clear();
        self.ticks = 0;
        self.custom_geoticks = Some(tec_sim.geoticks());
        self.custom_land_percent = Some(tec_sim.land_percent());
        self.recompute_climate_and_rivers();
        true
    }

    pub fn reset(&mut self) {
        self.populated.clear();
        self.popcache.clear();
        self.ticks = 0;
    }

    pub fn set_coast_prox(&mut self, coast_prox: usize) {
        if self.coast_prox != coast_prox {
            self.coast_prox = coast_prox;
            self.popcache.clear();
        }
    }

    pub fn set_travel_range(&mut self, travel_range: usize) {
        self.travel_range = travel_range;
    }

    /// Perform one population expansion step (`populationsimulation.py:45-50`).
    /// Returns `true` if population expanded, `false` if expansion is complete (no further frontier).
    pub fn step(&mut self) -> bool {
        let elevations: Vec<f32> = self.tiles.iter().map(|t| t.elevation()).collect();
        if self.populated.is_empty() {
            if let Some(eden) = eden_tile(
                &elevations,
                &self.tile_koeppen,
                &self.adjacency,
                &mut self.rng,
            ) {
                self.populated.insert(eden, 1);
            } else {
                return false;
            }
        }

        let empty_ag = BTreeSet::new();
        let expanded = expand_population(
            &elevations,
            &self.tile_koeppen,
            &self.adjacency,
            &self.river_tiles,
            &mut self.populated,
            &empty_ag,
            self.travel_range,
            self.coast_prox,
            &mut self.popcache,
        );
        if expanded {
            self.ticks += 1;
        }
        expanded
    }

    fn recompute_climate_and_rivers(&mut self) {
        let n = self.tiles.len();
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);
        self.tile_koeppen.resize(n, 0);
        self.tile_life.resize(n, 0.0);
        self.tile_precip_norm.resize(n, 0.0);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }

        let elevations: Vec<f32> = self.tiles.iter().map(|t| t.elevation()).collect();
        let rivers = trace_coarse_rivers(
            &elevations,
            &self.tile_precip_norm,
            &self.tile_koeppen,
            &self.adjacency,
            5.0,
            0.5,
        );
        self.river_tiles.fill(false);
        for r in rivers {
            for t in r {
                self.river_tiles[t] = true;
            }
        }
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn populated_count(&self) -> usize {
        self.populated.len()
    }

    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    pub fn is_custom_planet(&self) -> bool {
        self.custom_geoticks.is_some()
    }

    pub fn loaded_geoticks(&self) -> u32 {
        self.custom_geoticks.unwrap_or(0)
    }

    pub fn loaded_land_percent(&self) -> u32 {
        self.custom_land_percent.unwrap_or(29)
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        self.tiles.iter().map(|t| t.elevation()).collect()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.tiles.iter().map(|t| t.top_felsity()).collect()
    }

    pub fn tile_koeppen_ids(&self) -> &[u8] {
        &self.tile_koeppen
    }

    pub fn tile_life(&self) -> &[f32] {
        &self.tile_life
    }

    pub fn populated_mask(&self) -> Vec<u8> {
        let mut mask = vec![0u8; self.tiles.len()];
        for &t in self.populated.keys() {
            if t < mask.len() {
                mask[t] = 1;
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_population_sandbox_earth() {
        let mut sim = PopulationSandbox::new_earth(2, 6, 42);
        assert_eq!(sim.tile_count(), 7292);
        assert_eq!(sim.populated_count(), 0);
        let expanded = sim.step();
        assert!(expanded);
        assert!(sim.populated_count() > 1);
    }

    #[test]
    fn test_identify_continents_and_oceans_earth() {
        let sim = PopulationSandbox::new_earth(2, 6, 42);
        let elevs = sim.tile_elevations();
        let geo = identify_continents_and_oceans(sim.grid(), &elevs);
        assert!(
            geo.continents.len() >= 3,
            "Expected multiple continents on Earth, got {}",
            geo.continents.len()
        );
        assert!(
            geo.oceans.len() >= 2,
            "Expected multiple ocean basins on Earth, got {}",
            geo.oceans.len()
        );
        for (t, &e) in elevs.iter().enumerate() {
            if e > 0.0 {
                assert!(geo.continent_name(t).is_some());
                assert!(geo.ocean_name(t).is_none());
            } else {
                assert!(geo.ocean_name(t).is_some());
                assert!(geo.continent_name(t).is_none());
            }
        }
    }
}
