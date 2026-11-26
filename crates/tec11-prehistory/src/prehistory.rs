use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use tec11_climate::{ClimateModel, DEFAULT_GLACIATION, EARTH_GLACIATION, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use crate::lifeforms::{settle_species_on_climate, SettledBiosphere, SpeciesKind};
use crate::population::{
    eden_tile, expand_population_with_technologies_and_islands, is_farmable, is_habitable,
    is_subgrid_island_habitable, tile_population_thousands_with_water_tech, DisjointSet,
};
use crate::racination::{
    branch_wide_heritages_by_distance, racinate_with_technologies, racination_shades, Heritage,
};
use tec11_rivers::trace_coarse_rivers;
use tec11_geology::rock::{Rock, RockKind, RockLayer};
use tec11_geology::tile::{planet_radius_km, LithosphericGroup, TileGeology};

pub const PREHISTORY_COAST_PROX: usize = 2;
pub const PREHISTORY_TRAVEL_RANGE: usize = 6;
/// Panmictic gene-flow range for Prehistory racination (tighter than 16 kyr migration range
/// `PREHISTORY_TRAVEL_RANGE` so deserts, mountains, and seas genuinely isolate populations).
pub const PREHISTORY_RACINATION_RANGE: usize = 2;
/// Maximum spherical angular diameter (radians, ~41° / ~4,600 km on Earth) before a single
/// wide-ranging heritage branches into regional peoples via isolation by distance.
pub const PREHISTORY_ISOLATION_BY_DISTANCE_RAD: f64 = 0.72;
pub const PREHISTORY_MIN_RIVER_ELEV: f32 = 5.0;
pub const PREHISTORY_MIN_RIVER_PRECIP: f32 = 0.5;
pub const PREHISTORY_GLACIATION_STEP: u32 = 16;
pub const PREHISTORY_ANTHROGLACIAL_STEPS: usize = 6;

/// Agriculture discovery probability per river tile by Köppen climate code.
/// Favors arid/steppe exotic rivers (`BS`/`BW`) and seasonal wet-dry temperate valleys (`Cs`/`Cw`)
/// over year-round humid (`Cf`) or cold continental (`D*`) rivers.
#[inline]
pub fn agriculture_spark_prob(koeppen: u8) -> f64 {
    match koeppen {
        4 | 5 => 0.015,        // BS, BW (Nile, Mesopotamia, Indus)
        7 | 8 => 0.005,        // Cs, Cw (Mediterranean, Yellow River / monsoon temperate)
        6 => 0.003,            // Cf (year-round humid temperate)
        9 | 10 | 11 => 0.0008, // Df, Ds, Dw (continental)
        3 => 0.0006,           // Aw (tropical wet-dry savanna)
        _ => 0.0,
    }
}

/// Glaciation multiplier (`[0.0, 1.0]`) for agriculture discovery.
/// Suppressed during deep glacial maxima (`0.0` at `glaciation = 0.0`), ramps smoothly
/// through cool/Holocene climates (`~0.78` at Earth's `0.35`), and saturates at `1.0`
/// once the planet reaches mild interglacial conditions (`glaciation >= 0.50`) so
/// hothouse peaks (`glaciation = 1.0`) do not disproportionately trigger History.
#[inline]
pub fn agriculture_glaciation_factor(glaciation: f64) -> f64 {
    let t = (glaciation / 0.50).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Seasonal flooding multiplier (`[0.5, 2.0]`) for a river based on its basin's wet-vs-dry
/// seasonal precipitation swing across the climate model's seasonal snapshots.
fn river_seasonal_flood_factor(river: &[usize], climate_model: &ClimateModel) -> f64 {
    let num_seasons = climate_model.season_count();
    if river.is_empty() || num_seasons < 2 {
        return 1.0;
    }
    let mut p_min = f64::INFINITY;
    let mut p_max = f64::NEG_INFINITY;
    let inv_len = 1.0 / (river.len() as f64);
    for s in 0..num_seasons {
        if let Some(snap) = climate_model.season_snapshot(s) {
            let mean_p: f64 = river
                .iter()
                .map(|&t| snap.precipitation.get(t).copied().unwrap_or(0.0) as f64)
                .sum::<f64>()
                * inv_len;
            p_min = p_min.min(mean_p);
            p_max = p_max.max(mean_p);
        }
    }
    if !p_min.is_finite() || !p_max.is_finite() {
        return 1.0;
    }
    let swing = (p_max - p_min).max(0.0);
    (0.5 + 2.5 * swing).clamp(0.5, 2.0)
}

/// Rivergoing (canoe/barge/freshwater navigation & fishing) discovery probability per river tile by Köppen climate code.
/// Develops independently of agriculture and seafaring, and is boosted `3.0x` if the population already has seafaring.
#[inline]
pub fn rivergoing_spark_prob(koeppen: u8) -> f64 {
    match koeppen {
        1 | 2 => 0.015,        // Af, Am (tropical rainforest / monsoon river systems)
        6 | 7 | 8 => 0.008,    // Cf, Cs, Cw (temperate forested river basins)
        3 => 0.004,            // Aw (savanna floodplains)
        9 | 10 | 11 => 0.003,  // Df, Ds, Dw (boreal / continental rivers)
        4 | 5 => 0.002,        // BS, BW (arid river corridors)
        _ => 0.0,
    }
}

/// Seafaring (outrigger/sailing/coastal & pelagic navigation & marine fishing) discovery probability per coastal tile.
/// Develops independently of agriculture and rivergoing, and is boosted `3.0x` if the population already has rivergoing.
#[inline]
pub fn seafaring_spark_prob(koeppen: u8, sea_neighbors: usize) -> f64 {
    if sea_neighbors == 0 {
        return 0.0;
    }
    let base = match koeppen {
        1 | 2 | 3 => 0.004,    // Af, Am, Aw (tropical / monsoon coasts & archipelagos)
        6 | 7 | 8 => 0.005,    // Cf, Cs, Cw (Mediterranean / temperate maritime coasts)
        9 | 10 | 11 => 0.0025, // Df, Ds, Dw (subarctic / fjords / Baltic-style coasts)
        4 | 5 => 0.0015,       // BS, BW (arid trade-wind coasts)
        13 => 0.001,           // ET (tundra maritime hunters/fishers)
        _ => 0.0,
    };
    let island_mult = if sea_neighbors >= 3 {
        2.2
    } else if sea_neighbors == 2 {
        1.4
    } else {
        1.0
    };
    base * island_mult
}

/// Build connected-component continent groups for a static elevation map (such as Earth).
fn build_continent_groups_from_elevations(
    tiles: &[TileGeology],
    adjacency: &[Vec<usize>],
) -> Vec<LithosphericGroup> {
    let n = tiles.len();
    let mut dsu = DisjointSet::new(n);
    for t in 0..n {
        if tiles[t].elevation() > 0.0 {
            for &nb in &adjacency[t] {
                if tiles[nb].elevation() > 0.0 {
                    dsu.union(t, nb);
                }
            }
        }
    }
    let mut comp_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for t in 0..n {
        if tiles[t].elevation() > 0.0 {
            comp_map.entry(dsu.find(t)).or_default().push(t);
        }
    }
    comp_map
        .into_values()
        .map(|tile_ids| LithosphericGroup {
            tiles: tile_ids,
            velocity: [0.0, 0.0, 0.0],
        })
        .collect()
}

/// Complete Prehistory simulation (`prehistorysimulation.py`, `prehistorydisplay.py`, `prehistorypresenter.py`).
pub struct PrehistorySimulation {
    grid: SphericalGrid,
    adjacency: Vec<Vec<usize>>,
    radius_km: f64,
    spin: f64,
    tilt_deg: f64,
    tectonics_ticks: u32,
    prehistory_ticks: u32,
    /// Minimum Prehistory ticks before agriculture can be invented (`cycles * 2 * PREHISTORY_GLACIATION_STEP`).
    min_ag_ticks: u32,
    glaciation_tick: u32,
    glaciation: f64,
    /// Eustatic sea-level offset in model elevation units (`<= 0.0` during glacial periods,
    /// up to `-0.12 km` on custom planets or `-120/550` on canned Earth at glacial maximum).
    eustatic_sea_level_km: f32,
    tiles: Vec<TileGeology>,
    groups: Vec<LithosphericGroup>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    subgrid_island_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    tile_ag_potential: Vec<f32>,
    rivers: Vec<Vec<usize>>,
    river_tiles: Vec<bool>,
    populated: BTreeMap<usize, u32>,
    heritages: BTreeMap<u32, Heritage>,
    agricultural_heritages: BTreeSet<u32>,
    rivergoing_heritages: BTreeSet<u32>,
    seafaring_heritages: BTreeSet<u32>,
    next_heritage_id: u32,
    is_custom: bool,
    settled_biosphere: std::cell::RefCell<Option<SettledBiosphere>>,
    saved_environment: Option<tec11_proto::PlanetaryEnvironment>,
    rng: Pcg64Mcg,
}

impl PrehistorySimulation {
    pub fn new_earth(seed: u128) -> Self {
        let grid = SphericalGrid::with_size(6);
        let adjacency = grid.adjacency();
        let n = grid.faces.len();
        let mut tiles = Vec::with_capacity(n);

        for (idx, &elev_m) in EARTH_SIZE6_ELEVATIONS_M.iter().enumerate() {
            let elev_km = (elev_m as f32) / 550.0;
            if elev_km > 0.0 {
                let p = grid.faces[idx].0;
                let lat_abs = (p[2].abs() as f32).clamp(0.0, 1.0);
                let h = (idx as u32)
                    .wrapping_mul(0x9E37_79B9)
                    ^ (p[0].to_bits() as u32).rotate_left(7)
                    ^ (p[1].to_bits() as u32).rotate_left(17);
                let u = ((h % 1000) as f32) * 0.001;

                let top_thick = 0.35f32;
                let base_thick = elev_km + 24.0 - top_thick;
                let base_rock = if elev_km >= 2.2 && u < 0.30 {
                    let mut r = Rock::intrusive(0.60);
                    r.sulfidity = Some(0.28);
                    r.quartzity = Some(0.22);
                    r
                } else if elev_km >= 0.7 && (0.30..0.37).contains(&u) {
                    let mut r = Rock::intrusive(0.78);
                    r.quartzity = Some(0.38);
                    r.sulfidity = Some(0.10);
                    r
                } else if elev_km < 1.8 && (0.37..0.52).contains(&u) {
                    Rock {
                        kind: RockKind::Metamorphic,
                        name: "greenschist",
                        felsity: 0.48,
                        toughness: 0.75,
                        clasticity: None,
                        calcity: None,
                        bogginess: None,
                        quartzity: Some(0.32),
                        sulfidity: Some(0.22),
                        salinity: None,
                    }
                } else {
                    Rock::intrusive(0.68)
                };

                let top_rock = if elev_km >= 2.4 && u < 0.22 {
                    let mut r = Rock::extrusive(0.58);
                    r.sulfidity = Some(0.28);
                    r.quartzity = Some(0.20);
                    r
                } else if elev_km >= 0.9 && (0.30..0.36).contains(&u) {
                    let mut r = Rock::intrusive(0.80);
                    r.quartzity = Some(0.38);
                    r
                } else if (0.35..1.60).contains(&elev_km) && (0.38..0.48).contains(&u) {
                    Rock {
                        kind: RockKind::Sedimentary,
                        name: "sandstone",
                        felsity: 0.50,
                        toughness: 0.5,
                        clasticity: Some(8.0),
                        calcity: None,
                        bogginess: None,
                        quartzity: Some(0.30),
                        sulfidity: Some(0.22),
                        salinity: None,
                    }
                } else if elev_km < 1.2
                    && (lat_abs < 0.18 || (0.42..0.68).contains(&lat_abs))
                    && (0.52..0.63).contains(&u)
                {
                    Rock {
                        kind: RockKind::Sedimentary,
                        name: "siltstone",
                        felsity: 0.55,
                        toughness: 0.35,
                        clasticity: Some(20.0),
                        calcity: None,
                        bogginess: Some(0.45),
                        quartzity: None,
                        sulfidity: None,
                        salinity: None,
                    }
                } else if (0.22..0.40).contains(&lat_abs)
                    && elev_km < 1.4
                    && (0.64..0.72).contains(&u)
                {
                    Rock {
                        kind: RockKind::Sedimentary,
                        name: "limestone",
                        felsity: 0.50,
                        toughness: 0.45,
                        clasticity: Some(8.0),
                        calcity: Some(0.80),
                        bogginess: None,
                        quartzity: None,
                        sulfidity: None,
                        salinity: Some(0.36),
                    }
                } else if lat_abs < 0.48 && elev_km < 1.1 && u >= 0.78 {
                    Rock {
                        kind: RockKind::Sedimentary,
                        name: "limestone",
                        felsity: 0.52,
                        toughness: 0.48,
                        clasticity: Some(4.0),
                        calcity: Some(0.85),
                        bogginess: None,
                        quartzity: None,
                        sulfidity: None,
                        salinity: None,
                    }
                } else {
                    Rock {
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
                    }
                };

                let mut t = TileGeology {
                    bottom: -24.0,
                    mountainosity: (elev_km * 2.0).clamp(0.0, 20.0),
                    subduction: 0.0,
                    intrusion: None,
                    impact_scar: 0.0,
                    soil_depth_m: 2.0,
                    soil_fertility: 1.0,
                    layers: vec![
                        RockLayer {
                            rock: base_rock,
                            thickness: base_thick,
                        },
                        RockLayer {
                            rock: top_rock,
                            thickness: top_thick,
                        },
                    ],
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

        let groups = build_continent_groups_from_elevations(&tiles, &adjacency);
        let mut climate_model = ClimateModel::new_for_tectonics_with_adjacency(
            grid.clone(),
            adjacency.clone(),
            3,
            1.0,
            23.0,
        );
        climate_model.set_glaciation_only(EARTH_GLACIATION);
        let radius_km = planet_radius_km(6);

        let mut sim = Self {
            grid,
            adjacency,
            radius_km,
            spin: 1.0,
            tilt_deg: 23.0,
            tectonics_ticks: 0,
            prehistory_ticks: 0,
            min_ag_ticks: 0,
            glaciation_tick: 0,
            glaciation: EARTH_GLACIATION,
            eustatic_sea_level_km: Self::compute_eustatic_sea_level(EARTH_GLACIATION, false),
            tiles,
            groups,
            climate_model,
            tile_koeppen: vec![0; n],
            subgrid_island_koeppen: vec![0; n],
            tile_life: vec![0; n].into_iter().map(|_| 0.0).collect(),
            tile_precip_norm: vec![0.0; n],
            tile_ag_potential: vec![1.0; n],
            rivers: Vec::new(),
            river_tiles: vec![false; n],
            populated: BTreeMap::new(),
            heritages: BTreeMap::new(),
            agricultural_heritages: BTreeSet::new(),
            rivergoing_heritages: BTreeSet::new(),
            seafaring_heritages: BTreeSet::new(),
            next_heritage_id: 0,
            is_custom: false,
            settled_biosphere: std::cell::RefCell::new(None),
            saved_environment: None,
            rng: Pcg64Mcg::new(seed),
        };
        sim.refresh_climate_and_rivers();
        sim
    }

    /// Transition directly from Tectonics outputs in memory without serializing to protobuf
    /// or reconstructing the `SphericalGrid`, adjacency graph, or `OceanGridGeometry`.
    pub fn from_tectonics_parts(
        mut saved_environment: Option<tec11_proto::PlanetaryEnvironment>,
        parts: (
            SphericalGrid,
            Vec<Vec<usize>>,
            Vec<TileGeology>,
            Vec<LithosphericGroup>,
            ClimateModel,
            f64,
            f64,
            f64,
            u32,
            Pcg64Mcg,
        ),
    ) -> Self {
        if let Some(ref mut env) = saved_environment {
            env.has_atmosphere = true;
            env.has_life = true;
            env.current_moon_dist_radii = env.target_moon_dist_radii as f32;
        }
        let (
            grid,
            adjacency,
            mut tiles,
            mut groups,
            mut climate_model,
            radius_km,
            spin,
            tilt_deg,
            tectonics_ticks,
            mut rng,
        ) = parts;
        for t in &mut tiles {
            t.reset_surficial_soil_from_bedrock();
        }
        let n = tiles.len();
        if groups.is_empty() {
            groups = build_continent_groups_from_elevations(&tiles, &adjacency);
        }
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        // Start Prehistory at a random phase in the ongoing 32-tick glacial cycle
        // (`2 * PREHISTORY_GLACIATION_STEP`), since the cycle was conceptually running
        // even while abstracted away during Tectonics.
        let initial_glaciation_tick =
            (rng.next_u64() % ((2 * PREHISTORY_GLACIATION_STEP) as u64)) as u32;
        let initial_glaciation =
            0.5 - ((initial_glaciation_tick as f64) * PI / (PREHISTORY_GLACIATION_STEP as f64))
                .cos()
                / 2.0;
        climate_model.reset_for_stage(cells, spin, tilt_deg, initial_glaciation);
        let mut sim = Self {
            grid,
            adjacency,
            radius_km,
            spin,
            tilt_deg,
            tectonics_ticks,
            prehistory_ticks: 0,
            min_ag_ticks: 0,
            glaciation_tick: initial_glaciation_tick,
            glaciation: initial_glaciation,
            eustatic_sea_level_km: Self::compute_eustatic_sea_level(initial_glaciation, true),
            tiles,
            groups,
            climate_model,
            tile_koeppen: vec![0; n],
            subgrid_island_koeppen: vec![0; n],
            tile_life: vec![0; n].into_iter().map(|_| 0.0).collect(),
            tile_precip_norm: vec![0.0; n],
            tile_ag_potential: vec![1.0; n],
            rivers: Vec::new(),
            river_tiles: vec![false; n],
            populated: BTreeMap::new(),
            heritages: BTreeMap::new(),
            agricultural_heritages: BTreeSet::new(),
            rivergoing_heritages: BTreeSet::new(),
            seafaring_heritages: BTreeSet::new(),
            next_heritage_id: 0,
            is_custom: true,
            settled_biosphere: std::cell::RefCell::new(None),
            saved_environment,
            rng,
        };
        sim.refresh_climate_and_rivers();
        sim
    }

    /// Take ownership of this `PrehistorySimulation` for transitioning directly into `HistorySimulation`
    /// without serializing/deserializing a `PlanetSnapshot` or running `settle_species_on_climate` twice.
    pub fn take_for_history(&mut self) -> Self {
        let grid_size = self.grid.size;
        let grid = std::mem::replace(&mut self.grid, SphericalGrid::new_empty(grid_size));
        let adjacency = std::mem::take(&mut self.adjacency);
        let mut tiles = std::mem::take(&mut self.tiles);
        for t in &mut tiles {
            t.normalize_for_snapshot();
        }
        let n = tiles.len();
        let mut groups = std::mem::take(&mut self.groups);
        for g in &mut groups {
            g.tiles.retain(|&id| id < n);
            g.velocity = [
                (g.velocity[0] as f32) as f64,
                (g.velocity[1] as f32) as f64,
                (g.velocity[2] as f32) as f64,
            ];
        }
        if groups.is_empty() {
            groups = build_continent_groups_from_elevations(&tiles, &adjacency);
        }
        let r_f32 = self.radius_km as f32;
        let radius_km = if r_f32 > 0.0 {
            r_f32 as f64
        } else {
            planet_radius_km(grid_size)
        };
        let spin_f32 = self.spin as f32;
        let spin = if spin_f32 > 0.0 { spin_f32 as f64 } else { 1.0 };
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        let tilt_f32 = self.tilt_deg as f32;
        let tilt_deg = if tilt_f32 > 0.0 {
            tilt_f32 as f64
        } else {
            23.0
        };
        let glaciation = if self.glaciation_tick > 0 {
            0.5 - (((self.glaciation_tick - 1) as f64) * PI / (PREHISTORY_GLACIATION_STEP as f64))
                .cos()
                / 2.0
        } else {
            self.glaciation
        };
        let mut climate_model = std::mem::replace(&mut self.climate_model, ClimateModel::empty());
        climate_model.reset_for_stage(cells, spin, tilt_deg, glaciation);
        let rng = Pcg64Mcg::from_bytes(self.rng.to_bytes());
        let mut sim = Self {
            grid,
            adjacency,
            radius_km,
            spin,
            tilt_deg,
            tectonics_ticks: self.tectonics_ticks,
            prehistory_ticks: self.prehistory_ticks,
            min_ag_ticks: self.min_ag_ticks,
            glaciation_tick: self.glaciation_tick,
            glaciation,
            eustatic_sea_level_km: Self::compute_eustatic_sea_level(glaciation, self.is_custom),
            tiles,
            groups,
            climate_model,
            tile_koeppen: std::mem::take(&mut self.tile_koeppen),
            subgrid_island_koeppen: std::mem::take(&mut self.subgrid_island_koeppen),
            tile_life: std::mem::take(&mut self.tile_life),
            tile_precip_norm: std::mem::take(&mut self.tile_precip_norm),
            tile_ag_potential: std::mem::take(&mut self.tile_ag_potential),
            rivers: std::mem::take(&mut self.rivers),
            river_tiles: std::mem::take(&mut self.river_tiles),
            populated: std::mem::take(&mut self.populated),
            heritages: std::mem::take(&mut self.heritages),
            agricultural_heritages: std::mem::take(&mut self.agricultural_heritages),
            rivergoing_heritages: std::mem::take(&mut self.rivergoing_heritages),
            seafaring_heritages: std::mem::take(&mut self.seafaring_heritages),
            next_heritage_id: self.next_heritage_id,
            is_custom: true,
            settled_biosphere: std::cell::RefCell::new(None),
            saved_environment: self.saved_environment.clone(),
            rng,
        };
        sim.refresh_climate_and_rivers_without_biosphere();
        sim
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(42);
    }

    pub fn reset(&mut self) {
        let initial_glaciation = if self.is_custom {
            DEFAULT_GLACIATION
        } else {
            EARTH_GLACIATION
        };
        self.prehistory_ticks = 0;
        self.glaciation_tick = 0;
        self.glaciation = initial_glaciation;
        self.eustatic_sea_level_km =
            Self::compute_eustatic_sea_level(initial_glaciation, self.is_custom);
        for t in &mut self.tiles {
            t.reset_surficial_soil_from_bedrock();
        }
        self.populated.clear();
        self.heritages.clear();
        self.agricultural_heritages.clear();
        self.rivergoing_heritages.clear();
        self.seafaring_heritages.clear();
        self.next_heritage_id = 0;
        self.climate_model.set_glaciation_only(initial_glaciation);
        self.refresh_climate_and_rivers();
    }

    /// Advance the prehistory simulation by 1 tick (`4,000` years, matching `PrehistorySimulation.update`
    /// in `prehistorysimulation.py:103-146`).
    pub fn step(&mut self) {
        *self.settled_biosphere.borrow_mut() = None;
        let _span_glac = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::PreGlaciationClimate);
        // 1. Count initial glacial tiles (`EF = 12`, `ET = 13`) per continent group (`prehistorysimulation.py:104-106`).
        let old_glaciers: Vec<usize> = self
            .groups
            .iter()
            .map(|g| {
                g.tiles
                    .iter()
                    .filter(|&&t| matches!(self.tile_koeppen[t], 12 | 13))
                    .count()
            })
            .collect();

        // 2. Sinusoidal Pleistocene-style glaciation cycle (`prehistorysimulation.py:109`)
        //    plus eustatic sea-level drop (~120 m at glacial maximum) exposing shallow continental shelves.
        let glaciation =
            0.5 - ((self.glaciation_tick as f64) * PI / (PREHISTORY_GLACIATION_STEP as f64)).cos() / 2.0;
        self.glaciation = glaciation;
        self.eustatic_sea_level_km = Self::compute_eustatic_sea_level(glaciation, self.is_custom);
        self.climate_model.set_glaciation_only(glaciation);

        // 3. Recompute climate and update physical surficial soil state, then remove uninhabitable tiles.
        self.refresh_climate();
        self.update_surficial_soil();
        let ag_heritages = &self.agricultural_heritages;
        let subgrid_koeppen = &self.subgrid_island_koeppen;
        let tile_koeppen = &self.tile_koeppen;
        let tiles_ref = &self.tiles;
        let sea_level = self.eustatic_sea_level_km;
        self.populated.retain(|&t, &mut hid| {
            let e = tiles_ref[t].elevation() - sea_level;
            let is_ag = ag_heritages.contains(&hid);
            if e > 0.0 {
                is_habitable(e, tile_koeppen[t]) || (is_ag && is_farmable(e, tile_koeppen[t]))
            } else {
                is_subgrid_island_habitable(subgrid_koeppen[t], is_ag)
            }
        });

        // 4. Glacio-isostasy per continent group (`prehistorysimulation.py:117-121`).
        for (g_idx, g) in self.groups.iter().enumerate() {
            if g.tiles.is_empty() {
                continue;
            }
            let new_glaciers = g
                .tiles
                .iter()
                .filter(|&&t| matches!(self.tile_koeppen[t], 12 | 13))
                .count();
            let dg = (new_glaciers as f32) - (old_glaciers[g_idx] as f32);
            let dh = 0.6 * dg / (g.tiles.len() as f32);
            if dh.abs() > 1e-6 {
                for &t in &g.tiles {
                    self.tiles[t].isostasize(dh);
                }
            }
        }

        // 5. Advance glaciation & prehistory tick counters (`prehistorysimulation.py:123`).
        self.glaciation_tick += 1;
        self.prehistory_ticks += 1;

        drop(_span_glac);
        let _span_riv = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::PreRiversAndTechSparks);
        // 6. Seed Eden if currently unpopulated (`prehistorysimulation.py:126-127`).
        let elevations: Vec<f32> = self.tile_elevations();
        if self.populated.is_empty() {
            if let Some(eden) = eden_tile(
                &elevations,
                &self.tile_koeppen,
                &self.adjacency,
                &mut self.rng,
            ) {
                let h = self.alloc_heritage(Vec::new());
                self.populated.insert(eden, h.id);
            }
        }

        // 7. Trace coarse rivers (`prehistorysimulation.py:130`).
        self.refresh_rivers(&elevations);

        // 8. Spark agriculture and rivergoing along unique river tiles (`prehistorysimulation.py:133-137`),
        //    weighted by each river basin's seasonal flooding swing, and seafaring along coastal tiles.
        //    Rivergoing and seafaring develop independently of agriculture and of each other, though
        //    having one boosts the probability of developing the other (`3.0x`).
        //    Agriculture is gated until `min_ag_ticks` (configured glaciation cycles) have elapsed.
        let can_spark_ag = self.prehistory_ticks >= self.min_ag_ticks;
        let gfactor = agriculture_glaciation_factor(glaciation);
        let mut river_tile_flood: BTreeMap<usize, f64> = BTreeMap::new();
        for r in &self.rivers {
            let flood_mult = river_seasonal_flood_factor(r, &self.climate_model);
            for &t in r {
                river_tile_flood
                    .entry(t)
                    .and_modify(|cur| {
                        if flood_mult > *cur {
                            *cur = flood_mult;
                        }
                    })
                    .or_insert(flood_mult);
            }
        }
        for (t, flood_mult) in river_tile_flood {
            if let Some(&hid) = self.populated.get(&t) {
                if can_spark_ag {
                    let ag_mult = self.tile_ag_potential.get(t).copied().unwrap_or(1.0) as f64;
                    let prob =
                        gfactor * agriculture_spark_prob(self.tile_koeppen[t]) * ag_mult * flood_mult;
                    if prob > 0.0 && self.rng.uniform(0.0, 1.0) < prob {
                        self.agricultural_heritages.insert(hid);
                    }
                }
                if !self.rivergoing_heritages.contains(&hid) {
                    let sea_synergy = if self.seafaring_heritages.contains(&hid) {
                        3.0
                    } else {
                        1.0
                    };
                    let r_prob = (0.25 + 0.75 * gfactor)
                        * rivergoing_spark_prob(self.tile_koeppen[t])
                        * sea_synergy;
                    if r_prob > 0.0 && self.rng.uniform(0.0, 1.0) < r_prob {
                        self.rivergoing_heritages.insert(hid);
                    }
                }
            }
        }
        let populated_snapshot: Vec<(usize, u32)> =
            self.populated.iter().map(|(&t, &h)| (t, h)).collect();
        for (t, hid) in populated_snapshot {
            let is_subgrid_island = elevations[t] <= 0.0 && self.subgrid_island_koeppen[t] != 0;
            let eff_koeppen = if is_subgrid_island {
                self.subgrid_island_koeppen[t]
            } else {
                self.tile_koeppen[t]
            };
            let can_rainfed_farm = if is_subgrid_island {
                is_subgrid_island_habitable(eff_koeppen, true)
            } else {
                is_farmable(elevations[t], eff_koeppen)
            };
            // Low-probability rainfed crop domestication on humid farmable non-river tiles so low-relief
            // cratons or archipelagos without major mountain-fed rivers can still invent agriculture.
            if can_spark_ag
                && !self.agricultural_heritages.contains(&hid)
                && !self.river_tiles.get(t).copied().unwrap_or(false)
                && can_rainfed_farm
                && self.tile_precip_norm.get(t).copied().unwrap_or(0.0) >= 0.30
            {
                let ag_mult = self.tile_ag_potential.get(t).copied().unwrap_or(1.0) as f64;
                let rainfed_prob = gfactor * 0.0003 * ag_mult;
                if rainfed_prob > 0.0 && self.rng.uniform(0.0, 1.0) < rainfed_prob {
                    self.agricultural_heritages.insert(hid);
                }
            }
            if !self.seafaring_heritages.contains(&hid) {
                let sea_nbs = self.adjacency[t]
                    .iter()
                    .filter(|&&nb| elevations[nb] <= 0.0)
                    .count();
                if sea_nbs > 0 {
                    let riv_synergy = if self.rivergoing_heritages.contains(&hid) {
                        3.0
                    } else {
                        1.0
                    };
                    let s_prob = (0.25 + 0.75 * gfactor)
                        * seafaring_spark_prob(eff_koeppen, sea_nbs)
                        * riv_synergy;
                    if s_prob > 0.0 && self.rng.uniform(0.0, 1.0) < s_prob {
                        self.seafaring_heritages.insert(hid);
                    }
                }
            }
        }

        drop(_span_riv);
        let _span_exp = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::PrePopulationExpansion);
        // 9. Run up to `PREHISTORY_ANTHROGLACIAL_STEPS` (6) migration steps (`prehistorysimulation.py:140-143`).
        let mut popcache: BTreeMap<(usize, bool), bool> = BTreeMap::new();
        for _ in 0..PREHISTORY_ANTHROGLACIAL_STEPS {
            if !expand_population_with_technologies_and_islands(
                &elevations,
                &self.tile_koeppen,
                &self.adjacency,
                &self.river_tiles,
                &mut self.populated,
                &self.agricultural_heritages,
                &self.rivergoing_heritages,
                &self.seafaring_heritages,
                PREHISTORY_TRAVEL_RANGE,
                PREHISTORY_COAST_PROX,
                Some(&self.subgrid_island_koeppen),
                &mut popcache,
            ) {
                break;
            }
        }

        drop(_span_exp);
        let _span_rac = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::PreRacination);
        // 10. Speciate / racinate isolated or admixed populations (`prehistorysimulation.py:146`).
        let mut next_id = self.next_heritage_id;
        racinate_with_technologies(
            &elevations,
            &self.tile_koeppen,
            &self.adjacency,
            &mut self.populated,
            &mut self.agricultural_heritages,
            &mut self.rivergoing_heritages,
            &mut self.seafaring_heritages,
            &mut self.heritages,
            |ancestry| {
                let id = next_id;
                next_id += 1;
                Heritage {
                    id,
                    name: format!("people_{}", id + 1),
                    ancestry,
                }
            },
            PREHISTORY_RACINATION_RANGE,
        );
        let tile_centers: Vec<[f64; 3]> = self.grid.faces.iter().map(|(c, _)| *c).collect();
        branch_wide_heritages_by_distance(
            &tile_centers,
            &self.adjacency,
            &mut self.populated,
            &mut self.agricultural_heritages,
            &mut self.rivergoing_heritages,
            &mut self.seafaring_heritages,
            &mut self.heritages,
            |ancestry| {
                let id = next_id;
                next_id += 1;
                Heritage {
                    id,
                    name: format!("people_{}", id + 1),
                    ancestry,
                }
            },
            PREHISTORY_ISOLATION_BY_DISTANCE_RAD,
        );
        self.next_heritage_id = next_id;
    }

    fn alloc_heritage(&mut self, ancestry: Vec<u32>) -> Heritage {
        let id = self.next_heritage_id;
        self.next_heritage_id += 1;
        let h = Heritage {
            id,
            name: format!("people_{}", id + 1),
            ancestry,
        };
        self.heritages.insert(id, h.clone());
        h
    }

    fn sync_classification_arrays(&mut self) {
        let n = self.tiles.len();
        self.tile_koeppen.resize(n, 0);
        self.subgrid_island_koeppen.resize(n, 0);
        self.tile_life.resize(n, 0.0);
        self.tile_precip_norm.resize(n, 0.0);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }
        if self.is_custom {
            for v in 0..n {
                let eff_e = self.tiles[v].elevation() - self.eustatic_sea_level_km;
                if eff_e <= 0.0 {
                    if let Some(pk) = tec11_terrain::compute_subgrid_volcanic_peak_km(
                        eff_e,
                        self.tiles[v].thickness(),
                        self.tiles[v].mountainosity,
                    ) {
                        let isl_c = self.climate_model.classify_emergent_island(v, pk, true);
                        self.subgrid_island_koeppen[v] = isl_c.koeppen as u8;
                        self.tile_precip_norm[v] = isl_c.mean_precipitation as f32;
                    }
                }
            }
        }
    }

    fn refresh_climate(&mut self) {
        let sea_level = self.eustatic_sea_level_km;
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| (t.elevation() - sea_level).max(0.0))
            .collect();
        self.climate_model.update_for_glaciation_step(&land_elevs, true);
        self.sync_classification_arrays();
    }

    /// Update per-tile physical surficial soil state (`soil_depth_m` and `soil_fertility`)
    /// each 4,000-year Prehistory step:
    /// - Active glacier / ice sheet (`climate_model.is_tile_glaciated(t)`): strips soil depth
    ///   toward bare bedrock and relaxes fertility toward underlying bedrock baseline.
    /// - Periglacial margin (ice-free land within 2 hops of an active glacier): deposits
    ///   mineral-rich windblown loess & glacial outwash till, deepening and enriching soil.
    /// - Ice-free non-periglacial land: climate-driven weathering slowly rebuilds soil depth
    ///   toward precipitation-dependent equilibrium and relaxes fertility toward bedrock.
    fn update_surficial_soil(&mut self) {
        let n = self.tiles.len();
        let sea_level = self.eustatic_sea_level_km;
        let mut is_ice = vec![false; n];
        for t in 0..n {
            let eff_e = self.tiles[t].elevation() - sea_level;
            if eff_e > 0.0 && self.climate_model.is_tile_glaciated(t) {
                is_ice[t] = true;
            }
        }
        // Mark ice-free tiles adjacent (1 hop) to an active glacier for 2-hop periglacial lookup.
        let mut hop1 = vec![false; n];
        for t in 0..n {
            if is_ice[t] {
                for &nb in &self.adjacency[t] {
                    if !is_ice[nb] {
                        hop1[nb] = true;
                    }
                }
            }
        }
        for t in 0..n {
            let eff_e = self.tiles[t].elevation() - sea_level;
            if eff_e <= 0.0 {
                continue;
            }
            let old_eff = self.tiles[t].effective_soil_fertility().max(0.1);
            let bedrock_fert = self.tiles[t].surface_soil_fertility();
            if is_ice[t] {
                // Active glacier scouring strips regolith toward bare bedrock.
                let tile = &mut self.tiles[t];
                tile.soil_depth_m = (tile.soil_depth_m * 0.65 - 0.35).max(0.0);
                tile.soil_fertility =
                    (tile.soil_fertility + 0.25 * (bedrock_fert - tile.soil_fertility))
                        .clamp(0.45, 1.50);
            } else {
                let near_ice = hop1[t] || self.adjacency[t].iter().any(|&nb| hop1[nb]);
                if near_ice {
                    // Periglacial margin: loess & glacial outwash/till deposition.
                    let tile = &mut self.tiles[t];
                    let added = 0.65f32;
                    let loess_fert = 1.38f32;
                    let old_depth = tile.soil_depth_m.max(0.0);
                    let new_depth = (old_depth + added).min(20.0);
                    if new_depth > 1e-6 {
                        let eff_added = (new_depth - old_depth).max(0.15);
                        tile.soil_fertility = ((old_depth * tile.soil_fertility
                            + eff_added * loess_fert)
                            / (old_depth + eff_added))
                            .clamp(0.45, 1.50);
                    }
                    tile.soil_depth_m = new_depth;
                } else {
                    // Ice-free non-periglacial land: slow climate-driven weathering.
                    let precip_norm = self
                        .tile_precip_norm
                        .get(t)
                        .copied()
                        .unwrap_or(0.5)
                        .clamp(0.0, 1.0);
                    let target_depth = (0.6 + 2.0 * precip_norm).clamp(0.5, 2.6);
                    let tile = &mut self.tiles[t];
                    if tile.soil_depth_m < target_depth {
                        let weather_rate = 0.04 + 0.14 * precip_norm;
                        tile.soil_depth_m = (tile.soil_depth_m + weather_rate).min(target_depth);
                    } else if tile.soil_depth_m > target_depth {
                        tile.soil_depth_m = (tile.soil_depth_m - 0.03).max(target_depth);
                    }
                    tile.soil_fertility =
                        (tile.soil_fertility + 0.04 * (bedrock_fert - tile.soil_fertility))
                            .clamp(0.45, 1.50);
                }
            }
            let new_eff = self.tiles[t].effective_soil_fertility();
            if self.tile_ag_potential.len() == n && self.tile_ag_potential[t] > 0.0 {
                self.tile_ag_potential[t] =
                    (self.tile_ag_potential[t] * (new_eff / old_eff)).clamp(0.3, 2.8);
            }
        }
    }

    fn refresh_rivers(&mut self, elevations: &[f32]) {
        // Custom planets store true physical km elevations (typical ranges ~2.0..4.8 km, matching History's
        // `2.0 km` / `0.25` threshold), whereas canned Earth scales `EARTH_SIZE6_ELEVATIONS_M / 550.0`.
        let (hmin, pmin) = if self.is_custom {
            (2.0, 0.25)
        } else {
            (PREHISTORY_MIN_RIVER_ELEV, PREHISTORY_MIN_RIVER_PRECIP)
        };
        self.rivers = trace_coarse_rivers(
            elevations,
            &self.tile_precip_norm,
            self.climate_model.tile_glacier_cover(),
            &self.adjacency,
            hmin,
            pmin,
        );
        self.river_tiles.resize(elevations.len(), false);
        self.river_tiles.fill(false);
        for r in &self.rivers {
            for &t in r {
                self.river_tiles[t] = true;
            }
        }
    }

    fn update_ag_potential_from_biosphere(&mut self, biosphere: &SettledBiosphere) {
        let n = self.tiles.len();
        self.tile_ag_potential.resize(n, 0.0);
        for t in 0..n {
            let mut plant_count = 0u32;
            let mut herbivore_count = 0u32;
            for &sp_id in &biosphere.tile_all_species[t] {
                let sp = &biosphere.species[sp_id];
                match sp.kind {
                    SpeciesKind::Plant => plant_count += 1,
                    SpeciesKind::Animal if sp.trophic_level == 2 => herbivore_count += 1,
                    _ => {}
                }
            }
            let soil_fert = self.tiles[t].effective_soil_fertility();
            self.tile_ag_potential[t] = if plant_count == 0 {
                if self.subgrid_island_koeppen.get(t).copied().unwrap_or(0) != 0 {
                    soil_fert.clamp(0.6, 1.5)
                } else {
                    0.0
                }
            } else {
                let bio_factor = (0.5
                    + (plant_count as f32) / 30.0
                    + (herbivore_count as f32) / 20.0)
                    .clamp(0.5, 1.8);
                (bio_factor * soil_fert).clamp(0.3, 2.5)
            };
        }
    }

    pub fn take_or_settle_biosphere(&mut self, elevations: &[f32]) -> SettledBiosphere {
        if let Some(bio) = self.settled_biosphere.get_mut().take() {
            bio
        } else {
            let bio = settle_species_on_climate(elevations, &self.climate_model);
            self.update_ag_potential_from_biosphere(&bio);
            bio
        }
    }

    fn refresh_climate_and_rivers_without_biosphere(&mut self) -> Vec<f32> {
        let sea_level = self.eustatic_sea_level_km;
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| (t.elevation() - sea_level).max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);
        self.sync_classification_arrays();
        let elevations: Vec<f32> = self.tile_elevations();
        self.refresh_rivers(&elevations);
        *self.settled_biosphere.get_mut() = None;
        elevations
    }

    fn refresh_climate_and_rivers(&mut self) {
        let elevations = self.refresh_climate_and_rivers_without_biosphere();
        let biosphere = settle_species_on_climate(&elevations, &self.climate_model);
        self.update_ag_potential_from_biosphere(&biosphere);
        *self.settled_biosphere.get_mut() = Some(biosphere);
    }

    /// Serialize this simulation state as a `PrehistoryStage` inside a `tec11_proto::PlanetSnapshot`.
    pub fn to_proto_prehistory_stage(&self) -> tec11_proto::PrehistoryStage {
        let proto_tiles: Vec<tec11_proto::PrehistoryTile> = self
            .tiles
            .iter()
            .enumerate()
            .map(|(idx, t)| tec11_proto::PrehistoryTile {
                geology: Some(t.to_proto_geological_tile()),
                heritage_id: self.populated.get(&idx).copied(),
            })
            .collect();

        let proto_groups: Vec<tec11_proto::LithosphericGroup> = self
            .groups
            .iter()
            .map(|g| tec11_proto::LithosphericGroup {
                tile_ids: g.tiles.iter().map(|&id| id as u32).collect(),
                velocity: Some(tec11_proto::Vec3 {
                    x: g.velocity[0] as f32,
                    y: g.velocity[1] as f32,
                    z: g.velocity[2] as f32,
                }),
            })
            .collect();

        let mut proto_heritages = Vec::with_capacity(self.next_heritage_id as usize);
        for id in 0..self.next_heritage_id {
            if let Some(h) = self.heritages.get(&id) {
                proto_heritages.push(tec11_proto::Heritage {
                    name: h.name.clone(),
                    parent_heritage_ids: h.ancestry.clone(),
                    agricultural: self.agricultural_heritages.contains(&id),
                    rivergoing: self.rivergoing_heritages.contains(&id),
                    seafaring: self.seafaring_heritages.contains(&id),
                });
            } else {
                proto_heritages.push(tec11_proto::Heritage {
                    name: format!("people_{}", id + 1),
                    parent_heritage_ids: Vec::new(),
                    agricultural: false,
                    rivergoing: false,
                    seafaring: false,
                });
            }
        }

        tec11_proto::PrehistoryStage {
            tectonics_ticks: self.tectonics_ticks,
            prehistory_ticks: self.prehistory_ticks,
            glaciation_tick: self.glaciation_tick,
            tiles: proto_tiles,
            groups: proto_groups,
            heritages: proto_heritages,
        }
    }

    pub fn saved_environment(&self) -> Option<&tec11_proto::PlanetaryEnvironment> {
        self.saved_environment.as_ref()
    }

    pub fn set_saved_environment(&mut self, env: Option<tec11_proto::PlanetaryEnvironment>) {
        self.saved_environment = env;
    }

    pub fn to_snapshot(&self) -> tec11_proto::PlanetSnapshot {
        tec11_proto::PlanetSnapshot {
            rng_state_16b: self.rng.to_bytes().to_vec(),
            grid_size: self.grid.size,
            radius_km: self.radius_km as f32,
            spin: self.spin as f32,
            tilt_deg: self.tilt_deg as f32,
            stage: Some(tec11_proto::PlanetStageOneof::Prehistory(
                self.to_proto_prehistory_stage(),
            )),
            environment: self.saved_environment.clone(),
        }
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.to_snapshot().encode_bytes()
    }

    pub fn from_snapshot(snap: &tec11_proto::PlanetSnapshot, fallback_seed: u64) -> Option<Self> {
        let grid_size = snap.grid_size;
        if !(2..=7).contains(&grid_size) {
            return None;
        }
        let grid = SphericalGrid::with_size(grid_size);
        let n = grid.faces.len();
        let adjacency = grid.adjacency();

        let radius_km = if snap.radius_km > 0.0 {
            snap.radius_km as f64
        } else {
            planet_radius_km(grid_size)
        };
        let spin = if snap.spin > 0.0 {
            snap.spin as f64
        } else {
            1.0
        };
        let cells = tec11_climate::nominal_cells_for_spin(spin);
        let tilt_deg = if snap.tilt_deg > 0.0 {
            snap.tilt_deg as f64
        } else {
            23.0
        };

        let rng = if let Ok(bytes_16) = <[u8; 16]>::try_from(snap.rng_state_16b.as_slice()) {
            Pcg64Mcg::from_bytes(bytes_16)
        } else {
            Pcg64Mcg::from_seed_u64(fallback_seed)
        };

        match snap.stage.as_ref()? {
            tec11_proto::PlanetStageOneof::Tectonics(tec) => {
                if tec.tiles.len() != n {
                    return None;
                }
                let mut tiles = Vec::with_capacity(n);
                for pt in &tec.tiles {
                    tiles.push(TileGeology::from_proto_geological_tile(pt)?);
                }
                let mut groups: Vec<LithosphericGroup> = tec
                    .groups
                    .iter()
                    .map(|pg| {
                        let v = pg.velocity.unwrap_or_default();
                        LithosphericGroup {
                            tiles: pg
                                .tile_ids
                                .iter()
                                .map(|&id| id as usize)
                                .filter(|&id| id < n)
                                .collect(),
                            velocity: [v.x as f64, v.y as f64, v.z as f64],
                        }
                    })
                    .collect();
                if groups.is_empty() {
                    groups = build_continent_groups_from_elevations(&tiles, &adjacency);
                }

                let climate_model = ClimateModel::new_for_tectonics_with_adjacency(
                    grid.clone(),
                    adjacency.clone(),
                    cells,
                    spin,
                    tilt_deg,
                );
                let mut sim = Self {
                    grid,
                    adjacency,
                    radius_km,
                    spin,
                    tilt_deg,
                    tectonics_ticks: tec.tectonics_ticks,
                    prehistory_ticks: 0,
                    min_ag_ticks: 0,
                    glaciation_tick: 0,
                    glaciation: 0.5,
                    eustatic_sea_level_km: Self::compute_eustatic_sea_level(0.5, true),
                    tiles,
                    groups,
                    climate_model,
                    tile_koeppen: vec![0; n],
                    subgrid_island_koeppen: vec![0; n],
                    tile_life: vec![0.0; n],
                    tile_precip_norm: vec![0.0; n],
                    tile_ag_potential: vec![1.0; n],
                    rivers: Vec::new(),
                    river_tiles: vec![false; n],
                    populated: BTreeMap::new(),
                    heritages: BTreeMap::new(),
                    agricultural_heritages: BTreeSet::new(),
                    rivergoing_heritages: BTreeSet::new(),
                    seafaring_heritages: BTreeSet::new(),
                    next_heritage_id: 0,
                    is_custom: true,
                    settled_biosphere: std::cell::RefCell::new(None),
                    saved_environment: snap.environment.clone(),
                    rng,
                };
                sim.refresh_climate_and_rivers();
                Some(sim)
            }
            tec11_proto::PlanetStageOneof::Prehistory(pre) => {
                let mut sim = Self::from_proto_prehistory_stage(
                    pre,
                    grid,
                    adjacency,
                    radius_km,
                    spin,
                    cells,
                    tilt_deg,
                    rng,
                )?;
                sim.saved_environment = snap.environment.clone();
                Some(sim)
            }
            tec11_proto::PlanetStageOneof::History(hist) => {
                let pre = hist.prehistory_base.as_ref()?;
                let mut sim = Self::from_proto_prehistory_stage(
                    pre,
                    grid,
                    adjacency,
                    radius_km,
                    spin,
                    cells,
                    tilt_deg,
                    rng,
                )?;
                sim.saved_environment = snap.environment.clone();
                Some(sim)
            }
        }
    }

    fn from_proto_prehistory_stage(
        pre: &tec11_proto::PrehistoryStage,
        grid: SphericalGrid,
        adjacency: Vec<Vec<usize>>,
        radius_km: f64,
        spin: f64,
        cells: u32,
        tilt_deg: f64,
        rng: Pcg64Mcg,
    ) -> Option<Self> {
        let n = grid.faces.len();
        if pre.tiles.len() != n {
            return None;
        }
        let mut tiles = Vec::with_capacity(n);
        let mut populated = BTreeMap::new();
        for (idx, pt) in pre.tiles.iter().enumerate() {
            let geo = pt.geology.as_ref()?;
            tiles.push(TileGeology::from_proto_geological_tile(geo)?);
            if let Some(hid) = pt.heritage_id {
                populated.insert(idx, hid);
            }
        }

        let mut groups: Vec<LithosphericGroup> = pre
            .groups
            .iter()
            .map(|pg| {
                let v = pg.velocity.unwrap_or_default();
                LithosphericGroup {
                    tiles: pg
                        .tile_ids
                        .iter()
                        .map(|&id| id as usize)
                        .filter(|&id| id < n)
                        .collect(),
                    velocity: [v.x as f64, v.y as f64, v.z as f64],
                }
            })
            .collect();
        if groups.is_empty() {
            groups = build_continent_groups_from_elevations(&tiles, &adjacency);
        }

        let mut heritages = BTreeMap::new();
        let mut agricultural_heritages = BTreeSet::new();
        let mut rivergoing_heritages = BTreeSet::new();
        let mut seafaring_heritages = BTreeSet::new();
        for (idx, ph) in pre.heritages.iter().enumerate() {
            let id = idx as u32;
            let name = if ph.name.is_empty() {
                format!("people_{}", id + 1)
            } else {
                ph.name.clone()
            };
            heritages.insert(
                id,
                Heritage {
                    id,
                    name,
                    ancestry: ph.parent_heritage_ids.clone(),
                },
            );
            if ph.agricultural {
                agricultural_heritages.insert(id);
            }
            if ph.rivergoing {
                rivergoing_heritages.insert(id);
            }
            if ph.seafaring {
                seafaring_heritages.insert(id);
            }
        }
        let next_heritage_id = (pre.heritages.len() as u32).max(
            populated
                .values()
                .copied()
                .max()
                .map(|m| m + 1)
                .unwrap_or(0),
        );

        let glaciation = if pre.glaciation_tick > 0 {
            0.5 - (((pre.glaciation_tick - 1) as f64) * PI / (PREHISTORY_GLACIATION_STEP as f64)).cos()
                / 2.0
        } else {
            0.5
        };

        let mut climate_model = ClimateModel::new_for_tectonics_with_adjacency(
            grid.clone(),
            adjacency.clone(),
            cells,
            spin,
            tilt_deg,
        );
        climate_model.set_glaciation_only(glaciation);

        let mut sim = Self {
            grid,
            adjacency,
            radius_km,
            spin,
            tilt_deg,
            tectonics_ticks: pre.tectonics_ticks,
            prehistory_ticks: pre.prehistory_ticks,
            min_ag_ticks: 0,
            glaciation_tick: pre.glaciation_tick,
            glaciation,
            eustatic_sea_level_km: Self::compute_eustatic_sea_level(glaciation, true),
            tiles,
            groups,
            climate_model,
            tile_koeppen: vec![0; n],
            subgrid_island_koeppen: vec![0; n],
            tile_life: vec![0; n].into_iter().map(|_| 0.0).collect(),
            tile_precip_norm: vec![0.0; n],
            tile_ag_potential: vec![1.0; n],
            rivers: Vec::new(),
            river_tiles: vec![false; n],
            populated,
            heritages,
            agricultural_heritages,
            rivergoing_heritages,
            seafaring_heritages,
            next_heritage_id,
            is_custom: true,
            settled_biosphere: std::cell::RefCell::new(None),
            saved_environment: None,
            rng,
        };
        sim.refresh_climate_and_rivers();
        Some(sim)
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(sim) = Self::from_snapshot(&snap, 42) else {
            return false;
        };
        *self = sim;
        true
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn climate_model(&self) -> &ClimateModel {
        &self.climate_model
    }

    pub fn adjacency(&self) -> &[Vec<usize>] {
        &self.adjacency
    }

    pub fn tiles(&self) -> &[TileGeology] {
        &self.tiles
    }

    pub fn tiles_mut(&mut self) -> &mut [TileGeology] {
        &mut self.tiles
    }

    pub fn groups(&self) -> &[LithosphericGroup] {
        &self.groups
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn tectonics_ticks(&self) -> u32 {
        self.tectonics_ticks
    }

    pub fn prehistory_ticks(&self) -> u32 {
        self.prehistory_ticks
    }

    pub fn glaciation_tick(&self) -> u32 {
        self.glaciation_tick
    }

    pub fn glaciation(&self) -> f64 {
        self.glaciation
    }

    pub fn radius_km(&self) -> f64 {
        self.radius_km
    }

    pub fn spin(&self) -> f64 {
        self.spin
    }

    pub fn tilt_deg(&self) -> f64 {
        self.tilt_deg
    }

    pub fn is_custom_planet(&self) -> bool {
        self.is_custom
    }

    pub fn land_percent(&self) -> u32 {
        let n = self.tiles.len().max(1);
        let land = self.tiles.iter().filter(|t| t.elevation() > 0.0).count();
        ((land * 100) / n) as u32
    }

    pub fn populated(&self) -> &BTreeMap<usize, u32> {
        &self.populated
    }

    pub fn heritages(&self) -> &BTreeMap<u32, Heritage> {
        &self.heritages
    }

    pub fn agricultural_heritages(&self) -> &BTreeSet<u32> {
        &self.agricultural_heritages
    }

    pub fn rivergoing_heritages(&self) -> &BTreeSet<u32> {
        &self.rivergoing_heritages
    }

    pub fn seafaring_heritages(&self) -> &BTreeSet<u32> {
        &self.seafaring_heritages
    }

    pub fn river_tiles(&self) -> &[bool] {
        &self.river_tiles
    }

    pub fn populated_count(&self) -> usize {
        self.populated.len()
    }

    /// Number of distinct races currently inhabiting at least one tile (`prehistorysimulation.py:85`).
    pub fn active_race_count(&self) -> usize {
        let active: BTreeSet<u32> = self.populated.values().copied().collect();
        active.len()
    }

    /// Number of distinct agricultural races currently inhabiting at least one tile (`prehistorysimulation.py:89`).
    pub fn active_agricultural_race_count(&self) -> usize {
        let active_ag: BTreeSet<u32> = self
            .populated
            .values()
            .copied()
            .filter(|h| self.agricultural_heritages.contains(h))
            .collect();
        active_ag.len()
    }

    pub fn active_rivergoing_race_count(&self) -> usize {
        let active_riv: BTreeSet<u32> = self
            .populated
            .values()
            .copied()
            .filter(|h| self.rivergoing_heritages.contains(h))
            .collect();
        active_riv.len()
    }

    pub fn active_seafaring_race_count(&self) -> usize {
        let active_sea: BTreeSet<u32> = self
            .populated
            .values()
            .copied()
            .filter(|h| self.seafaring_heritages.contains(h))
            .collect();
        active_sea.len()
    }

    /// Population in thousands on coarse tile `t` for `heritage_id` (scaling emergent sub-grid
    /// volcanic/arc/atoll islands by `1/6` of a full coarse tile).
    pub fn tile_population_for_heritage_thousands(&self, t: usize, heritage_id: u32) -> f32 {
        let is_ag = self.agricultural_heritages.contains(&heritage_id);
        let is_riv_fish = self.rivergoing_heritages.contains(&heritage_id)
            && self.river_tiles.get(t).copied().unwrap_or(false);
        let is_sea_fish = self.seafaring_heritages.contains(&heritage_id)
            && self.adjacency.get(t).is_some_and(|nbs| {
                self.tiles[t].elevation() <= 0.0
                    || nbs.iter().any(|&nb| self.tiles[nb].elevation() <= 0.0)
            });
        let is_subgrid_island = self.tiles[t].elevation() <= 0.0
            && self.subgrid_island_koeppen.get(t).copied().unwrap_or(0) != 0;
        let eff_koeppen = if is_subgrid_island {
            self.subgrid_island_koeppen[t]
        } else {
            self.tile_koeppen[t]
        };
        let raw = tile_population_thousands_with_water_tech(
            eff_koeppen,
            is_ag,
            is_riv_fish,
            is_sea_fish,
        );
        if is_subgrid_island {
            raw * (1.0 / 6.0)
        } else {
            raw
        }
    }

    /// Total planet population in thousands (`prehistorysimulation.py:93`), including freshwater
    /// and coastal fishing carrying capacity bonuses from rivergoing and seafaring technologies.
    pub fn total_population_thousands(&self) -> f32 {
        self.populated
            .iter()
            .map(|(&t, &h)| self.tile_population_for_heritage_thousands(t, h))
            .sum()
    }

    /// Compute eustatic sea-level offset (`<= 0.0`) from the current glaciation index (`0.0` at
    /// glacial maximum → `-120 m`, `1.0` at full interglacial → `0 m`).
    #[inline]
    pub fn compute_eustatic_sea_level(glaciation: f64, is_custom: bool) -> f32 {
        let max_drop = if is_custom { 0.12f32 } else { 120.0 / 550.0 };
        -max_drop * (1.0 - glaciation as f32).clamp(0.0, 1.0)
    }

    /// Require at least `cycles` full Pleistocene glaciation cycles (`2 * PREHISTORY_GLACIATION_STEP`
    /// ticks per cycle) before agriculture can be invented.
    pub fn set_min_glaciation_cycles(&mut self, cycles: u32) {
        self.min_ag_ticks = cycles.saturating_mul(2 * PREHISTORY_GLACIATION_STEP);
    }

    pub fn min_ag_ticks(&self) -> u32 {
        self.min_ag_ticks
    }

    /// Current eustatic sea-level offset in physical meters (`-120.0` at glacial maximum to `0.0`
    /// at full interglacial).
    pub fn eustatic_sea_level_m(&self) -> f32 {
        -120.0 * (1.0 - self.glaciation as f32).clamp(0.0, 1.0)
    }

    pub fn tile_elevations(&self) -> Vec<f32> {
        let sea_level = self.eustatic_sea_level_km;
        self.tiles
            .iter()
            .map(|t| t.elevation() - sea_level)
            .collect()
    }

    pub fn tile_felsities(&self) -> Vec<f32> {
        self.tiles.iter().map(|t| t.top_felsity()).collect()
    }

    pub fn tile_koeppen_ids(&self) -> &[u8] {
        &self.tile_koeppen
    }

    pub fn subgrid_island_koeppen_ids(&self) -> &[u8] {
        &self.subgrid_island_koeppen
    }

    pub fn tile_life(&self) -> &[f32] {
        &self.tile_life
    }

    pub fn tile_precip_norm(&self) -> &[f32] {
        &self.tile_precip_norm
    }

    pub fn river_mask(&self) -> Vec<u8> {
        self.river_tiles.iter().map(|&b| if b { 1 } else { 0 }).collect()
    }

    /// Per-tile prehistoric population overlay colors `[r0, g0, b0, ...]` for populated tiles,
    /// and `[-1.0, -1.0, -1.0]` for unpopulated tiles (so JS can render natural terrain colors
    /// on unpopulated tiles), matching `prehistorydisplay.py:21-29`.
    /// Agricultural tiles are red `[1.0, 0.0, 0.0]`; paleolithic tiles use `racination_shades`.
    pub fn population_overlay_colors(&self) -> Vec<f32> {
        let active_paleo: Vec<u32> = {
            let set: BTreeSet<u32> = self
                .populated
                .values()
                .copied()
                .filter(|h| !self.agricultural_heritages.contains(h))
                .collect();
            set.into_iter().collect()
        };
        let shades = racination_shades(active_paleo.len());
        let mut shade_map = BTreeMap::new();
        for (idx, hid) in active_paleo.into_iter().enumerate() {
            shade_map.insert(hid, shades[idx]);
        }

        let n = self.tiles.len();
        let mut out = Vec::with_capacity(n * 3);
        for t in 0..n {
            if let Some(&hid) = self.populated.get(&t) {
                if self.agricultural_heritages.contains(&hid) {
                    out.extend_from_slice(&[1.0, 0.0, 0.0]);
                } else {
                    let rgb = shade_map.get(&hid).copied().unwrap_or([1.0, 0.5, 0.0]);
                    out.extend_from_slice(&rgb);
                }
            } else {
                out.extend_from_slice(&[-1.0, -1.0, -1.0]);
            }
        }
        out
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        let Some(tile) = self.tiles.get(tile_idx) else {
            return String::new();
        };
        let elev = tile.elevation();
        let isl_k = self.subgrid_island_koeppen.get(tile_idx).copied().unwrap_or(0);
        let is_subgrid_island = elev <= 0.0 && isl_k != 0;
        let koeppen = if is_subgrid_island {
            isl_k
        } else {
            self.tile_koeppen.get(tile_idx).copied().unwrap_or(0)
        };
        let mut lines = vec![if is_subgrid_island {
            format!(
                "Coarse Tile #{tile_idx} (Sub-grid Island) | Bathymetry: {elev:+.2} km | Island Climate: {}",
                tec11_koeppen::koeppen_climate_name(koeppen)
            )
        } else {
            format!(
                "Coarse Tile #{tile_idx} | Elevation: {elev:+.2} km | Climate: {}",
                tec11_koeppen::koeppen_climate_name(koeppen)
            )
        }];
        if let Some(top) = tile.layers.last() {
            lines.push(format!(
                "Surface Rock: {} ({}, {:.1} km)",
                top.rock.name,
                top.rock.kind.code(),
                top.thickness
            ));
        }
        if let Some(&hid) = self.populated.get(&tile_idx) {
            let is_ag = self.agricultural_heritages.contains(&hid);
            let is_riv = self.rivergoing_heritages.contains(&hid);
            let is_sea = self.seafaring_heritages.contains(&hid);
            let pop_k = self.tile_population_for_heritage_thousands(tile_idx, hid);
            let mut tech = Vec::new();
            if is_ag {
                tech.push("Agricultural");
            }
            if is_riv {
                tech.push("Rivergoing");
            }
            if is_sea {
                tech.push("Seafaring");
            }
            if tech.is_empty() {
                tech.push("Paleolithic");
            }
            lines.push(format!(
                "Population: {:.1}K ({}) | Heritage #{hid}",
                pop_k,
                tech.join(" · ")
            ));
        } else if elev > 0.0 || is_subgrid_island {
            lines.push("Population: Uninhabited Land".to_string());
        } else {
            lines.push("Marine Basin".to_string());
        }
        lines.join("\n")
    }

    pub fn inspect_tile_stratigraphy_json(&self, tile_idx: usize) -> String {
        let Some(tile) = self.tiles.get(tile_idx) else {
            return "{\"hasColumn\":false}".to_string();
        };
        tile.to_stratigraphy_json(tile_idx, None)
    }

    pub fn subgrid_island_koeppen(&self) -> &[u8] {
        &self.subgrid_island_koeppen
    }

    pub fn salient_flora_and_fauna_for_tile(
        &self,
        tile_idx: usize,
        max_flora: usize,
        max_fauna: usize,
    ) -> (Vec<String>, Vec<String>) {
        let mut guard = self.settled_biosphere.borrow_mut();
        if guard.is_none() {
            let elevations: Vec<f32> = self.tiles.iter().map(|t| t.elevation()).collect();
            *guard = Some(settle_species_on_climate(&elevations, &self.climate_model));
        }
        match guard.as_ref() {
            Some(bio) => bio.select_salient_flora_and_fauna_names(tile_idx, max_flora, max_fauna),
            None => (Vec::new(), Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prehistory_simulation_and_snapshot_roundtrip() {
        let mut sim = PrehistorySimulation::new_earth(42);
        assert_eq!(sim.tile_count(), 7292);
        assert_eq!(sim.populated_count(), 0);

        for _ in 0..4 {
            sim.step();
        }
        assert_eq!(sim.prehistory_ticks(), 4);
        assert!(sim.populated_count() > 0);
        assert!(sim.active_race_count() >= 1);
        assert!(sim.total_population_thousands() > 0.0);

        let bytes = sim.encode_snapshot();
        assert!(!bytes.is_empty());

        let mut restored = PrehistorySimulation::new_earth(99);
        assert!(restored.load_snapshot_bytes(&bytes));
        assert_eq!(restored.prehistory_ticks(), 4);
        assert_eq!(restored.populated_count(), sim.populated_count());
        assert_eq!(restored.active_race_count(), sim.active_race_count());
    }

    #[test]
    fn test_custom_planet_prehistory_rivers_use_physical_km_threshold() {
        let mut tec = tec11_tectonics::TectonicsSimulation::new(4, 30.0, 2026);
        for _ in 0..35 {
            tec.step();
        }
        let snap = tec.to_snapshot();
        let mut pre = PrehistorySimulation::from_snapshot(&snap, 42).unwrap();
        // Cap all land elevations at 3.8 km (< 5.0 km scaled Earth threshold, >= 2.0 km physical range)
        for t in &mut pre.tiles {
            let e = t.elevation();
            if e > 3.8 {
                t.bottom -= e - 3.8;
            }
        }
        let elevs = pre.tile_elevations();
        pre.refresh_rivers(&elevs);
        assert!(pre.is_custom_planet());
        assert!(
            pre.river_tiles().iter().any(|&b| b),
            "Custom Prehistory planets with 2.0..3.8 km mountain ranges should trace coarse rivers"
        );
    }

    #[test]
    fn test_seafaring_settles_and_hops_across_subgrid_islands_in_prehistory() {
        // Synthetic 1D ring/corridor of 19 tiles:
        //   0 (Continent A) -- [1..5 sea] -- 6 (Sub-grid Island 1, 6 hops from 0)
        //   -- [7..11 sea] -- 12 (Sub-grid Island 2, 6 hops from 6)
        //   -- [13..17 sea] -- 18 (Continent B, 6 hops from 12, 18 hops from 0!)
        let n = 19usize;
        let mut adj = vec![Vec::new(); n];
        for i in 0..(n - 1) {
            adj[i].push(i + 1);
            adj[i + 1].push(i);
        }
        let mut elevs = vec![-3.0f32; n];
        elevs[0] = 0.5;
        elevs[6] = -0.35; // submerged coarse tile with emergent sub-grid island
        elevs[12] = -0.25; // submerged coarse tile with emergent sub-grid island
        elevs[18] = 0.5;

        let mut koeppen = vec![0u8; n];
        koeppen[0] = 6; // Cf
        koeppen[18] = 6; // Cf

        let mut subgrid_isl_k = vec![0u8; n];
        subgrid_isl_k[6] = 1; // Af tropical wet volcanic island
        subgrid_isl_k[12] = 2; // Am tropical monsoon island

        let rivers = vec![false; n];
        let ag = BTreeSet::new();
        let riv = BTreeSet::new();
        let no_sea = BTreeSet::new();
        let mut with_sea = BTreeSet::new();
        with_sea.insert(0u32);

        let mut pop = BTreeMap::new();
        pop.insert(0usize, 0u32);
        let mut memo = BTreeMap::new();

        // Non-seafaring heritage cannot settle sub-grid islands or cross open water
        assert!(!expand_population_with_technologies_and_islands(
            &elevs,
            &koeppen,
            &adj,
            &rivers,
            &mut pop,
            &ag,
            &riv,
            &no_sea,
            6,
            2,
            Some(&subgrid_isl_k),
            &mut memo,
        ));
        assert_eq!(pop.len(), 1);

        // Seafaring heritage hops 0 -> 6 (Island 1) -> 12 (Island 2) -> 18 (Continent B) across 18 sea tiles!
        for _ in 0..3 {
            assert!(expand_population_with_technologies_and_islands(
                &elevs,
                &koeppen,
                &adj,
                &rivers,
                &mut pop,
                &ag,
                &riv,
                &with_sea,
                6,
                2,
                Some(&subgrid_isl_k),
                &mut memo,
            ));
        }
        assert!(pop.contains_key(&6), "Sub-grid Island 1 should be settled");
        assert!(pop.contains_key(&12), "Sub-grid Island 2 should be settled via Island 1");
        assert!(
            pop.contains_key(&18),
            "Distant Continent B (18 hops away) should be reached via sub-grid island stepping stones"
        );
    }

    #[test]
    fn test_land_ag_potential_incorporates_surface_soil_fertility() {
        let mut sim = PrehistorySimulation::new_earth(42);
        let land_tiles: Vec<usize> = (0..sim.tiles.len())
            .filter(|&t| sim.tiles[t].elevation() > 0.0 && sim.tile_ag_potential[t] > 0.0)
            .collect();
        assert!(land_tiles.len() >= 2);
        let t_a = land_tiles[0];
        let t_b = land_tiles[1];

        // Give t_a fertile basaltic/volcanic topsoil and t_b nutrient-poor granite topsoil,
        // and settle a shared biosphere with identical plant/herbivore counts on both tiles.
        sim.tiles[t_a] = TileGeology::empty_land(tec11_geology::rock::Rock::extrusive(0.48), 1.0);
        sim.tiles[t_b] = TileGeology::empty_land(tec11_geology::rock::Rock::intrusive(0.74), 1.0);
        assert!(
            sim.tiles[t_a].surface_soil_fertility() > sim.tiles[t_b].surface_soil_fertility(),
            "Basaltic topsoil should have higher fertility than granitic topsoil"
        );

        let mut bio = sim.take_or_settle_biosphere(&sim.tile_elevations());
        let shared_species = bio.tile_all_species[t_a].clone();
        bio.tile_all_species[t_b] = shared_species;
        sim.update_ag_potential_from_biosphere(&bio);

        assert!(
            sim.tile_ag_potential[t_a] > sim.tile_ag_potential[t_b],
            "Fertile topsoil tile ({}) should have higher ag_potential than infertile tile ({})",
            sim.tile_ag_potential[t_a],
            sim.tile_ag_potential[t_b]
        );
    }

    #[test]
    fn test_river_seasonal_flood_factor_rewards_seasonal_precipitation_swing() {
        let sim = PrehistorySimulation::new_earth(42);
        let cm = sim.climate_model();
        let num_seasons = cm.season_count();
        assert!(num_seasons >= 2);

        // Find one land tile with a large seasonal precipitation swing and one with a small swing.
        let mut best_seasonal = (0usize, -1.0f64);
        let mut best_steady = (0usize, f64::INFINITY);
        for t in 0..sim.tiles.len() {
            if sim.tiles[t].elevation() <= 0.0 {
                continue;
            }
            let mut p_min = f64::INFINITY;
            let mut p_max = f64::NEG_INFINITY;
            for s in 0..num_seasons {
                if let Some(snap) = cm.season_snapshot(s) {
                    let p = snap.precipitation[t] as f64;
                    p_min = p_min.min(p);
                    p_max = p_max.max(p);
                }
            }
            let swing = p_max - p_min;
            if swing > best_seasonal.1 {
                best_seasonal = (t, swing);
            }
            if swing < best_steady.1 {
                best_steady = (t, swing);
            }
        }

        let f_seasonal = river_seasonal_flood_factor(&[best_seasonal.0], cm);
        let f_steady = river_seasonal_flood_factor(&[best_steady.0], cm);
        assert!(
            f_seasonal > f_steady + 0.15,
            "Seasonal river basin (swing={:.3}, mult={:.3}) should outscore steady basin (swing={:.3}, mult={:.3})",
            best_seasonal.1,
            f_seasonal,
            best_steady.1,
            f_steady
        );
        assert!(agriculture_spark_prob(7) > agriculture_spark_prob(6)); // Cs > Cf
        assert!(agriculture_spark_prob(8) > agriculture_spark_prob(6)); // Cw > Cf
    }

    #[test]
    fn test_agriculture_glaciation_factor_saturates_at_interglacial() {
        assert!((agriculture_glaciation_factor(0.0) - 0.0).abs() < 1e-12);
        assert!(agriculture_glaciation_factor(0.25) > 0.4 && agriculture_glaciation_factor(0.25) < 0.6);
        assert!(agriculture_glaciation_factor(EARTH_GLACIATION) > 0.7);
        assert!((agriculture_glaciation_factor(0.50) - 1.0).abs() < 1e-12);
        assert!((agriculture_glaciation_factor(1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_eustatic_sea_level_exposes_and_submerges_glacial_land_bridges() {
        let mut sim = PrehistorySimulation::new_earth(42);
        // Step at glacial maximum (glaciation_tick = 0 -> glaciation = 0.0 -> -120 m sea level)
        sim.glaciation_tick = 0;
        sim.step();
        assert!(
            sim.eustatic_sea_level_m() <= -119.0,
            "Expected ~-120 m eustatic drop at glacial maximum, got {}",
            sim.eustatic_sea_level_m()
        );
        let lgm_land = sim.tile_elevations().iter().filter(|&&e| e > 0.0).count();

        // Step at full interglacial (glaciation_tick = PREHISTORY_GLACIATION_STEP -> glaciation = 1.0 -> 0 m)
        sim.glaciation_tick = PREHISTORY_GLACIATION_STEP;
        sim.step();
        assert!(
            sim.eustatic_sea_level_m().abs() < 1e-3,
            "Expected 0 m eustatic offset at full interglacial, got {}",
            sim.eustatic_sea_level_m()
        );
        let interglacial_land = sim.tile_elevations().iter().filter(|&&e| e > 0.0).count();

        assert!(
            lgm_land > interglacial_land,
            "Expected eustatic sea-level drop at LGM to expose shallow continental shelves ({lgm_land} vs {interglacial_land} land tiles)"
        );
    }

    #[test]
    fn test_glacial_scouring_vs_periglacial_loess_fertility() {
        let mut sim = PrehistorySimulation::new_earth(42);
        sim.glaciation_tick = 0;
        // Run 6 ticks near glacial maximum so ice sheets scour interiors and deposit loess on margins
        for _ in 0..6 {
            sim.step();
        }
        let mut scoured_count = 0usize;
        let mut loess_count = 0usize;
        let mut min_depth = f32::INFINITY;
        let mut max_depth = 0.0f32;
        let mut min_eff = f32::INFINITY;
        let mut max_eff = 0.0f32;
        for t in 0..sim.tiles.len() {
            if sim.tiles[t].elevation() <= 0.0 {
                continue;
            }
            let d = sim.tiles[t].soil_depth_m;
            let eff = sim.tiles[t].effective_soil_fertility();
            let bedrock = sim.tiles[t].surface_soil_fertility();
            if d < 0.5 && eff < bedrock * 0.85 {
                scoured_count += 1;
            }
            if d > 4.0 && sim.tiles[t].soil_fertility > 1.15 && eff > bedrock * 1.10 {
                loess_count += 1;
            }
            min_depth = min_depth.min(d);
            max_depth = max_depth.max(d);
            min_eff = min_eff.min(eff);
            max_eff = max_eff.max(eff);
        }
        assert!(
            scoured_count > 10 && min_depth < 0.1,
            "Expected ice-scoured shield interiors stripped to bare bedrock (count={scoured_count}, min_depth={min_depth:.3}, min_eff={min_eff:.3})"
        );
        assert!(
            loess_count > 10 && max_depth > 4.5 && max_eff > 1.20,
            "Expected periglacial loess/till margins with deepened, enriched soil (count={loess_count}, max_depth={max_depth:.3}, max_eff={max_eff:.3})"
        );

        // Verify physical surficial soil state survives full PlanetSnapshot encode/decode round-trip.
        use tec11_proto::Message as _;
        let bytes = sim.to_snapshot().encode_to_vec();
        let decoded_snap = tec11_proto::PlanetSnapshot::decode(bytes.as_slice())
            .expect("Prehistory PlanetSnapshot decode should succeed");
        let reloaded = PrehistorySimulation::from_snapshot(&decoded_snap, 42)
            .expect("Prehistory PlanetSnapshot round-trip should succeed");
        assert_eq!(reloaded.tiles.len(), sim.tiles.len());
        for t in 0..sim.tiles.len() {
            assert!(
                (reloaded.tiles[t].soil_depth_m - sim.tiles[t].soil_depth_m).abs() < 1e-5,
                "Tile {t} soil_depth_m drifted across snapshot round-trip"
            );
            assert!(
                (reloaded.tiles[t].soil_fertility - sim.tiles[t].soil_fertility).abs() < 1e-5,
                "Tile {t} soil_fertility drifted across snapshot round-trip"
            );
            assert!(
                (reloaded.tiles[t].effective_soil_fertility()
                    - sim.tiles[t].effective_soil_fertility())
                .abs()
                    < 1e-5,
                "Tile {t} effective_soil_fertility drifted across snapshot round-trip"
            );
        }
    }
}
