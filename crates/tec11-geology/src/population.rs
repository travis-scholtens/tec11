use std::collections::{BTreeMap, BTreeSet};

use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

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
    if elevations[t] <= 0.0 {
        return false;
    }
    let reachable = within_steps(t, adjacency, d, |u| {
        is_hoppable(elevations[u], koeppen_ids[u], false)
    });
    reachable.into_iter().any(|n| elevations[n] <= 0.0)
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
    let mut frontier: BTreeMap<usize, u32> = BTreeMap::new();

    for (&t, &heritage_id) in populated.iter() {
        let farms = agricultural_heritages.contains(&heritage_id);
        let mut distance = 0usize;
        let mut adjs: Vec<usize> = adjacency[t].clone();

        while distance < travel_range {
            let mut added = false;
            for &n in &adjs {
                if !populated.contains_key(&n) {
                    let cand = *cmemo.entry((n, farms)).or_insert_with(|| {
                        if farms {
                            is_farmable(elevations[n], koeppen_ids[n])
                        } else {
                            is_squattable(
                                n,
                                elevations,
                                koeppen_ids,
                                adjacency,
                                coastal_proximity,
                                river_tiles,
                            )
                        }
                    });
                    if cand {
                        frontier.insert(n, heritage_id);
                        added = true;
                    }
                }
            }
            if added {
                break;
            }
            let curr_set: BTreeSet<usize> = adjs.into_iter().collect();
            let next_set = expand_within(&curr_set, adjacency, |u| {
                is_hoppable(elevations[u], koeppen_ids[u], farms)
            });
            adjs = next_set.into_iter().collect();
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
                    layers: vec![RockLayer {
                        rock: Rock {
                            kind: RockKind::Sedimentary,
                            name: "sandstone",
                            felsity: 0.55,
                            toughness: 0.5,
                            clasticity: Some(1.0),
                            calcity: None,
                            bogginess: None,
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
}
