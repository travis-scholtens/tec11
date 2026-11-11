use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use crate::pangea::PangeaWorld;
use crate::population::{
    eden_tile, expand_population, is_habitable, tile_population_thousands, DisjointSet,
};
use crate::racination::{racinate, racination_shades, Heritage};
use crate::rivers::trace_coarse_rivers;
use crate::rock::{Rock, RockKind, RockLayer};
use crate::tectonics::LithosphericGroup;
use crate::tile::TileGeology;

pub const PREHISTORY_COAST_PROX: usize = 2;
pub const PREHISTORY_TRAVEL_RANGE: usize = 6;
pub const PREHISTORY_MIN_RIVER_ELEV: f32 = 5.0;
pub const PREHISTORY_MIN_RIVER_PRECIP: f32 = 0.5;
pub const PREHISTORY_GLACIATION_STEP: u32 = 16;
pub const PREHISTORY_ANTHROGLACIAL_STEPS: usize = 6;

/// Agriculture discovery probability per river tile by Köppen climate code (`prehistorysimulation.py:34-43`).
#[inline]
pub fn agriculture_spark_prob(koeppen: u8) -> f64 {
    match koeppen {
        4 | 5 => 0.05,         // BS, BW
        6 | 7 | 8 => 0.005,    // Cf, Cs, Cw
        9 | 10 | 11 => 0.001,  // Df, Ds, Dw
        3 => 0.0001,           // Aw
        _ => 0.0,
    }
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
    glaciation_tick: u32,
    glaciation: f64,
    tiles: Vec<TileGeology>,
    groups: Vec<LithosphericGroup>,
    climate_model: ClimateModel,
    tile_koeppen: Vec<u8>,
    tile_life: Vec<f32>,
    tile_precip_norm: Vec<f32>,
    rivers: Vec<Vec<usize>>,
    river_tiles: Vec<bool>,
    populated: BTreeMap<usize, u32>,
    heritages: BTreeMap<u32, Heritage>,
    agricultural_heritages: BTreeSet<u32>,
    next_heritage_id: u32,
    is_custom: bool,
    rng: Pcg64Mcg,
}

impl PrehistorySimulation {
    pub fn new_earth(seed: u128) -> Self {
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

        let groups = build_continent_groups_from_elevations(&tiles, &adjacency);
        let climate_model = ClimateModel::new_for_tectonics(grid.clone(), 3, 1.0, 23.0);
        let radius_km = PangeaWorld::planet_radius_km(6);

        let mut sim = Self {
            grid,
            adjacency,
            radius_km,
            spin: 1.0,
            tilt_deg: 23.0,
            tectonics_ticks: 0,
            prehistory_ticks: 0,
            glaciation_tick: 0,
            glaciation: 0.5,
            tiles,
            groups,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_precip_norm: vec![0.0; n],
            rivers: Vec::new(),
            river_tiles: vec![false; n],
            populated: BTreeMap::new(),
            heritages: BTreeMap::new(),
            agricultural_heritages: BTreeSet::new(),
            next_heritage_id: 0,
            is_custom: false,
            rng: Pcg64Mcg::new(seed),
        };
        sim.refresh_climate_and_rivers();
        sim
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(42);
    }

    pub fn reset(&mut self) {
        self.prehistory_ticks = 0;
        self.glaciation_tick = 0;
        self.glaciation = 0.5;
        self.populated.clear();
        self.heritages.clear();
        self.agricultural_heritages.clear();
        self.next_heritage_id = 0;
        self.climate_model.set_glaciation_only(0.5);
        self.refresh_climate_and_rivers();
    }

    /// Advance the prehistory simulation by 1 tick (`4,000` years, matching `PrehistorySimulation.update`
    /// in `prehistorysimulation.py:103-146`).
    pub fn step(&mut self) {
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

        // 2. Sinusoidal Pleistocene-style glaciation cycle (`prehistorysimulation.py:109`).
        let glaciation =
            0.5 - ((self.glaciation_tick as f64) * PI / (PREHISTORY_GLACIATION_STEP as f64)).cos() / 2.0;
        self.glaciation = glaciation;
        self.climate_model.set_glaciation_only(glaciation);

        // 3. Recompute climate and remove populated tiles that became uninhabitable (`prehistorysimulation.py:110-114`).
        self.refresh_climate();
        self.populated.retain(|&t, _| {
            is_habitable(self.tiles[t].elevation(), self.tile_koeppen[t])
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

        // 6. Seed Eden if currently unpopulated (`prehistorysimulation.py:126-127`).
        let elevations: Vec<f32> = self.tiles.iter().map(|t| t.elevation()).collect();
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

        // 8. Spark agriculture along river tiles (`prehistorysimulation.py:133-137`).
        let gfactor = glaciation * glaciation;
        for r in &self.rivers {
            for &t in r {
                if let Some(&hid) = self.populated.get(&t) {
                    let prob = gfactor * agriculture_spark_prob(self.tile_koeppen[t]);
                    if prob > 0.0 && self.rng.uniform(0.0, 1.0) < prob {
                        self.agricultural_heritages.insert(hid);
                    }
                }
            }
        }

        // 9. Run up to `PREHISTORY_ANTHROGLACIAL_STEPS` (6) migration steps (`prehistorysimulation.py:140-143`).
        let mut popcache: BTreeMap<(usize, bool), bool> = BTreeMap::new();
        for _ in 0..PREHISTORY_ANTHROGLACIAL_STEPS {
            if !expand_population(
                &elevations,
                &self.tile_koeppen,
                &self.adjacency,
                &self.river_tiles,
                &mut self.populated,
                &self.agricultural_heritages,
                PREHISTORY_TRAVEL_RANGE,
                PREHISTORY_COAST_PROX,
                &mut popcache,
            ) {
                break;
            }
        }

        // 10. Speciate / racinate isolated or admixed populations (`prehistorysimulation.py:146`).
        let mut next_id = self.next_heritage_id;
        racinate(
            &elevations,
            &self.tile_koeppen,
            &self.adjacency,
            &mut self.populated,
            &mut self.agricultural_heritages,
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
            PREHISTORY_TRAVEL_RANGE,
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

    fn refresh_climate(&mut self) {
        let n = self.tiles.len();
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_glaciation_step(&land_elevs, true);
        self.tile_koeppen.resize(n, 0);
        self.tile_life.resize(n, 0.0);
        self.tile_precip_norm.resize(n, 0.0);

        for (v, c) in self.climate_model.classifications().iter().enumerate() {
            self.tile_koeppen[v] = c.koeppen as u8;
            self.tile_life[v] = if c.life { 1.0 } else { 0.0 };
            self.tile_precip_norm[v] = c.mean_precipitation as f32;
        }
    }

    fn refresh_rivers(&mut self, elevations: &[f32]) {
        self.rivers = trace_coarse_rivers(
            elevations,
            &self.tile_precip_norm,
            &self.tile_koeppen,
            &self.adjacency,
            PREHISTORY_MIN_RIVER_ELEV,
            PREHISTORY_MIN_RIVER_PRECIP,
        );
        self.river_tiles.resize(elevations.len(), false);
        self.river_tiles.fill(false);
        for r in &self.rivers {
            for &t in r {
                self.river_tiles[t] = true;
            }
        }
    }

    fn refresh_climate_and_rivers(&mut self) {
        let land_elevs: Vec<f32> = self
            .tiles
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        self.climate_model.update_for_tectonics(&land_elevs, true);
        self.refresh_climate();
        let elevations: Vec<f32> = self.tiles.iter().map(|t| t.elevation()).collect();
        self.refresh_rivers(&elevations);
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
                });
            } else {
                proto_heritages.push(tec11_proto::Heritage {
                    name: format!("people_{}", id + 1),
                    parent_heritage_ids: Vec::new(),
                    agricultural: false,
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
        }
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.to_snapshot().encode_bytes()
    }

    pub fn from_snapshot(snap: &tec11_proto::PlanetSnapshot, fallback_seed: u64) -> Option<Self> {
        let grid_size = snap.grid_size;
        if !(4..=7).contains(&grid_size) {
            return None;
        }
        let grid = SphericalGrid::with_size(grid_size);
        let n = grid.faces.len();
        let adjacency = grid.adjacency();

        let radius_km = if snap.radius_km > 0.0 {
            snap.radius_km as f64
        } else {
            PangeaWorld::planet_radius_km(grid_size)
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

                let climate_model =
                    ClimateModel::new_for_tectonics(grid.clone(), cells, spin, tilt_deg);
                let mut sim = Self {
                    grid,
                    adjacency,
                    radius_km,
                    spin,
                    tilt_deg,
                    tectonics_ticks: tec.tectonics_ticks,
                    prehistory_ticks: 0,
                    glaciation_tick: 0,
                    glaciation: 0.5,
                    tiles,
                    groups,
                    climate_model,
                    tile_koeppen: vec![0; n],
                    tile_life: vec![0.0; n],
                    tile_precip_norm: vec![0.0; n],
                    rivers: Vec::new(),
                    river_tiles: vec![false; n],
                    populated: BTreeMap::new(),
                    heritages: BTreeMap::new(),
                    agricultural_heritages: BTreeSet::new(),
                    next_heritage_id: 0,
                    is_custom: true,
                    rng,
                };
                sim.refresh_climate_and_rivers();
                Some(sim)
            }
            tec11_proto::PlanetStageOneof::Prehistory(pre) => {
                Self::from_proto_prehistory_stage(
                    pre,
                    grid,
                    adjacency,
                    radius_km,
                    spin,
                    cells,
                    tilt_deg,
                    rng,
                )
            }
            tec11_proto::PlanetStageOneof::History(hist) => {
                let pre = hist.prehistory_base.as_ref()?;
                Self::from_proto_prehistory_stage(
                    pre,
                    grid,
                    adjacency,
                    radius_km,
                    spin,
                    cells,
                    tilt_deg,
                    rng,
                )
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

        let mut climate_model =
            ClimateModel::new_for_tectonics(grid.clone(), cells, spin, tilt_deg);
        climate_model.set_glaciation_only(glaciation);

        let mut sim = Self {
            grid,
            adjacency,
            radius_km,
            spin,
            tilt_deg,
            tectonics_ticks: pre.tectonics_ticks,
            prehistory_ticks: pre.prehistory_ticks,
            glaciation_tick: pre.glaciation_tick,
            glaciation,
            tiles,
            groups,
            climate_model,
            tile_koeppen: vec![0; n],
            tile_life: vec![0.0; n],
            tile_precip_norm: vec![0.0; n],
            rivers: Vec::new(),
            river_tiles: vec![false; n],
            populated,
            heritages,
            agricultural_heritages,
            next_heritage_id,
            is_custom: true,
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

    /// Total planet population in thousands (`prehistorysimulation.py:93`).
    pub fn total_population_thousands(&self) -> f32 {
        self.populated
            .iter()
            .map(|(&t, &h)| {
                let is_ag = self.agricultural_heritages.contains(&h);
                tile_population_thousands(self.tile_koeppen[t], is_ag)
            })
            .sum()
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
}
