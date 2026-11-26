use std::collections::{BTreeMap, BTreeSet};

use tec11_climate::{ClimateModel, EARTH_GLACIATION, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use tec11_rivers::trace_coarse_rivers;
use tec11_geology::rock::{Rock, RockKind, RockLayer};
use tec11_geology::tile::{tiles_from_snapshot, TileGeology};


pub use tec11_topography::{
    identify_continents_and_oceans, identify_continents_and_oceans_with_adj, main_ocean,
    ContinentInfo, DisjointSet, MountainRangeInfo, OceanBasinInfo, PlanetaryGeography, SeaInfo,
};

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

/// Habitable by seafaring peoples on emergent sub-grid volcanic, arc, or atoll islands
/// (`elevation <= 0.0` on the coarse grid with an emergent `> 0.0 km` fine-grid summit).
/// Maritime fisher-foragers (`!is_agricultural`) can inhabit wet/monsoon/savanna tropical (`Af`, `Am`, `Aw`)
/// and temperate/subpolar (`C*`, `D*`) islands; agricultural islanders can also farm semi-arid (`BS`) islands.
#[inline]
pub fn is_subgrid_island_habitable(island_koeppen: u8, is_agricultural: bool) -> bool {
    if is_agricultural {
        matches!(island_koeppen, 1..=4 | 6..=11)
    } else {
        matches!(island_koeppen, 1..=3 | 6..=11)
    }
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
    expand_population_impl(
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
        true,
        None,
        cmemo,
    )
}

/// Expand `populated` map by one migration step taking into account `agricultural_heritages`,
/// `rivergoing_heritages` (extended range along river corridors), and `seafaring_heritages`
/// (required to traverse open-ocean tiles `elevation <= 0.0`, with extended coastal & sea range).
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
    expand_population_impl(
        elevations,
        koeppen_ids,
        adjacency,
        river_tiles,
        populated,
        agricultural_heritages,
        rivergoing_heritages,
        seafaring_heritages,
        travel_range,
        coastal_proximity,
        false,
        None,
        cmemo,
    )
}

/// Expand `populated` map by one migration step with technology-gated ocean navigation and
/// settleable emergent sub-grid volcanic/arc/atoll islands (`subgrid_island_koeppen[t] != 0` on
/// `elevations[t] <= 0.0`), enabling seafaring heritages to use oceanic islands and archipelagos
/// as permanent settlements and stepping stones across wide oceans.
pub fn expand_population_with_technologies_and_islands(
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
    subgrid_island_koeppen: Option<&[u8]>,
    cmemo: &mut BTreeMap<(usize, bool), bool>,
) -> bool {
    expand_population_impl(
        elevations,
        koeppen_ids,
        adjacency,
        river_tiles,
        populated,
        agricultural_heritages,
        rivergoing_heritages,
        seafaring_heritages,
        travel_range,
        coastal_proximity,
        false,
        subgrid_island_koeppen,
        cmemo,
    )
}

fn expand_population_impl(
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
    allow_unassisted_sea: bool,
    subgrid_island_koeppen: Option<&[u8]>,
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
        let can_cross_sea = seas_tech || allow_unassisted_sea;
        let eff_range = travel_range
            + usize::from(rivers_tech && river_tiles.get(t).copied().unwrap_or(false))
            + usize::from(seas_tech);
        let can_hop_through = |tile: usize| -> bool {
            if elevations[tile] <= 0.0 {
                can_cross_sea
            } else {
                (rivers_tech && river_tiles.get(tile).copied().unwrap_or(false))
                    || is_passable(elevations[tile], koeppen_ids[tile], farms)
            }
        };

        stamp = stamp.wrapping_add(1);
        if stamp == 0 {
            visited_stamp.fill(0);
            stamp = 1;
        }
        visited_stamp[t] = stamp;

        curr_frontier.clear();
        for &nb in &adjacency[t] {
            if visited_stamp[nb] != stamp && (elevations[nb] > 0.0 || can_cross_sea) {
                visited_stamp[nb] = stamp;
                curr_frontier.push(nb);
            }
        }

        let mut distance = 0usize;
        while distance < eff_range && !curr_frontier.is_empty() {
            let mut added = false;
            for &n in &curr_frontier {
                if !is_pop[n] {
                    let cand = if elevations[n] <= 0.0 {
                        seas_tech
                            && subgrid_island_koeppen
                                .and_then(|s| s.get(n))
                                .copied()
                                .is_some_and(|isl_k| is_subgrid_island_habitable(isl_k, farms))
                    } else if farms {
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
                if can_hop_through(u) {
                    for &a in &adjacency[u] {
                        if visited_stamp[a] != stamp && can_hop_through(a) {
                            visited_stamp[a] = stamp;
                            next_frontier.push(a);
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
                    soil_depth_m: 2.0,
                    soil_fertility: 1.0,
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
                t.reset_surficial_soil_from_bedrock();
                tiles.push(t);
            } else {
                let bathy_km = ((elev_m as f32) / 1000.0).clamp(-7.0, -0.2);
                tiles.push(TileGeology {
                    bottom: bathy_km - 5.0,
                    mountainosity: 0.0,
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    soil_depth_m: 0.0,
                    soil_fertility: 1.0,
                    layers: vec![RockLayer {
                        rock: Rock::extrusive(0.5),
                        thickness: 5.0,
                    }],
                });
            }
        }

        let mut climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);
        climate_model.set_glaciation_only(EARTH_GLACIATION);
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
        let Some((grid, tiles, geoticks)) = tiles_from_snapshot(&snap) else {
            return false;
        };
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let land_cnt = tiles.iter().filter(|t| t.elevation() > 0.0).count();
        let land_pct = ((land_cnt as f64) * 100.0 / (n.max(1) as f64)).round() as u32;
        self.grid = grid.clone();
        self.adjacency = adjacency;
        self.tiles = tiles;
        self.climate_model = ClimateModel::new_for_tectonics(grid, 3, 1.0, 23.0);
        self.tile_koeppen = vec![0; n];
        self.tile_life = vec![0.0; n];
        self.tile_precip_norm = vec![0.0; n];
        self.river_tiles = vec![false; n];
        self.populated.clear();
        self.popcache.clear();
        self.ticks = 0;
        self.custom_geoticks = Some(geoticks);
        self.custom_land_percent = Some(land_pct);
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
            self.climate_model.tile_glacier_cover(),
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

    #[test]
    fn test_open_ocean_hopping_requires_seafaring() {
        // Linear 5-tile chain: [Land 0] -- [Sea 1] -- [Sea 2] -- [Land 3] -- [Land 4]
        let elevations = vec![0.5, -3.2, -3.5, 0.4, 0.6];
        let koeppen_ids = vec![3, 0, 0, 3, 6];
        let adjacency = vec![vec![1], vec![0, 2], vec![1, 3], vec![2, 4], vec![3]];
        let river_tiles = vec![false; 5];
        let mut populated = BTreeMap::from([(0usize, 0u32)]);
        let ag = BTreeSet::from([0u32]);
        let riv = BTreeSet::from([0u32]);
        let no_sea = BTreeSet::new();
        let mut memo = BTreeMap::new();

        // Without seafaring, even agricultural + rivergoing populations cannot cross open ocean:
        let moved_without_sea = expand_population_with_technologies(
            &elevations,
            &koeppen_ids,
            &adjacency,
            &river_tiles,
            &mut populated,
            &ag,
            &riv,
            &no_sea,
            6,
            2,
            &mut memo,
        );
        assert!(!moved_without_sea);
        assert_eq!(populated.len(), 1);

        // Once seafaring is acquired, the population crosses the sea tiles to settle Land 3:
        let with_sea = BTreeSet::from([0u32]);
        let moved_with_sea = expand_population_with_technologies(
            &elevations,
            &koeppen_ids,
            &adjacency,
            &river_tiles,
            &mut populated,
            &ag,
            &riv,
            &with_sea,
            6,
            2,
            &mut memo,
        );
        assert!(moved_with_sea);
        assert_eq!(populated.get(&3), Some(&0));
    }
}
