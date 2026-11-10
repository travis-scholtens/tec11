use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tec11_math_grid::{
    normal, DisplayFaceSource, FineParentage, Pcg64Mcg, PointTree, SphericalGrid,
    SubdividedTerrainGrid, Vec3,
};
use tec11_proto::PlanetSnapshotExt;

use crate::language::{Language, SoundChangeKind, WordOrigin};
use crate::lifeforms::{settle_species_on_climate, SettledSpecies, SpeciesKind};
use crate::population::{
    is_farmable, is_habitable, is_passable, main_ocean, tile_population_thousands,
};
use crate::prehistory::PrehistorySimulation;
use crate::rivers::trace_coarse_rivers;

pub const HISTORY_YEARS_PER_TICK: u32 = 200;
pub const HISTORY_MIN_RIVER_ELEV: f32 = 2.0;
pub const HISTORY_MIN_RIVER_PRECIP: f32 = 0.25;

/// 6 distinct political map colors (`terrainmethod.py:colornations`).
pub const NATION_PALETTE: [[f32; 3]; 6] = [
    [0.88, 0.28, 0.28], // Crimson
    [0.92, 0.56, 0.18], // Amber
    [0.86, 0.82, 0.22], // Gold
    [0.26, 0.72, 0.58], // Teal
    [0.32, 0.56, 0.88], // Cobalt
    [0.72, 0.38, 0.86], // Amethyst
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConceptKey {
    Species(u32),
    Nation(u32),
    State(u32),
    Language(u32),
}

impl ConceptKey {
    pub fn to_proto(self) -> tec11_proto::Concept {
        match self {
            Self::Species(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Species as i32,
                id,
            },
            Self::Nation(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Nation as i32,
                id,
            },
            Self::State(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::State as i32,
                id,
            },
            Self::Language(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Language as i32,
                id,
            },
        }
    }

    pub fn from_proto(c: &tec11_proto::Concept) -> Self {
        match tec11_proto::ConceptKind::try_from(c.kind)
            .unwrap_or(tec11_proto::ConceptKind::Species)
        {
            tec11_proto::ConceptKind::Species => Self::Species(c.id),
            tec11_proto::ConceptKind::Nation => Self::Nation(c.id),
            tec11_proto::ConceptKind::State => Self::State(c.id),
            tec11_proto::ConceptKind::Language => Self::Language(c.id),
        }
    }
}

/// Demographic community on a tile (`people/community.py`).
#[derive(Clone, Debug)]
pub struct Community {
    pub thousands: f32,
    pub nationality_id: u32,
    pub language_id: u32,
    pub agriculture: bool,
    pub racial_mix: BTreeMap<u32, f32>,
}

impl Community {
    pub fn normalize_racial_mix(&mut self) {
        let sum: f32 = self.racial_mix.values().copied().sum();
        if sum > 1e-6 {
            for v in self.racial_mix.values_mut() {
                *v /= sum;
            }
        }
    }

    pub fn merge_from(&mut self, other: &Community) {
        let total = (self.thousands + other.thousands).max(1e-6);
        let w_self = self.thousands / total;
        let w_other = other.thousands / total;
        let mut new_mix = BTreeMap::new();
        for (&h, &f) in &self.racial_mix {
            *new_mix.entry(h).or_insert(0.0) += f * w_self;
        }
        for (&h, &f) in &other.racial_mix {
            *new_mix.entry(h).or_insert(0.0) += f * w_other;
        }
        self.thousands += other.thousands;
        self.agriculture = self.agriculture || other.agriculture;
        self.racial_mix = new_mix;
        self.normalize_racial_mix();
    }
}

#[derive(Clone, Debug)]
pub struct LanguageRecord {
    pub id: u32,
    pub phonology: Language,
    pub lexicon: BTreeMap<ConceptKey, WordOrigin>,
    pub changes_per_tick: Vec<Vec<u32>>,
}

impl LanguageRecord {
    pub fn new(id: u32, rng: &mut Pcg64Mcg) -> Self {
        let phonology = Language::generate(rng);
        let mut rec = Self {
            id,
            phonology,
            lexicon: BTreeMap::new(),
            changes_per_tick: Vec::new(),
        };
        rec.coin_concept(ConceptKey::Language(id), rng);
        rec
    }

    pub fn name(&self) -> String {
        self.lexicon
            .get(&ConceptKey::Language(self.id))
            .map(|o| o.word.write(true))
            .unwrap_or_else(|| format!("Lang_{}", self.id + 1))
    }

    pub fn coin_concept(&mut self, key: ConceptKey, rng: &mut Pcg64Mcg) -> WordOrigin {
        if let Some(existing) = self.lexicon.get(&key) {
            return existing.clone();
        }
        let lang_label = self
            .lexicon
            .get(&ConceptKey::Language(self.id))
            .map(|o| o.word.write(true))
            .unwrap_or_else(|| format!("Proto-{}", self.id + 1));
        let word = self.phonology.generate_word(rng);
        let origin = WordOrigin::new(word, lang_label, None);
        self.lexicon.insert(key, origin.clone());
        origin
    }

    pub fn borrow_concept(&mut self, key: ConceptKey, donor: &LanguageRecord) {
        if self.lexicon.contains_key(&key) {
            return;
        }
        if let Some(donor_origin) = donor.lexicon.get(&key) {
            let borrowed_word = self.phonology.borrow_word(&donor_origin.word);
            let my_name = self.name();
            let origin = WordOrigin::new(borrowed_word, my_name, Some(donor_origin.clone()));
            self.lexicon.insert(key, origin);
        }
    }

    pub fn mutate_tick(&mut self, rng: &mut Pcg64Mcg) {
        let change_id = (rng.next_u64() as u32) % 9;
        let change = SoundChangeKind::from_u32(change_id);
        let my_name = self.name();
        for origin in self.lexicon.values_mut() {
            let mut next = origin.mutate(change);
            next.lang_name = my_name.clone();
            *origin = next;
        }
        self.changes_per_tick.push(vec![change_id]);
    }
}

#[derive(Clone, Debug)]
pub struct StateRecord {
    pub id: u32,
    pub capital_tile: usize,
    pub color_idx: usize,
    pub active: bool,
}

/// Complete History simulation (`historysimulation.py`, `historydisplay.py`, `historypresenter.py`).
pub struct HistorySimulation {
    prehistory: PrehistorySimulation,
    terrain: SubdividedTerrainGrid,
    tree: PointTree,
    history_ticks: u32,
    settled_species: Vec<SettledSpecies>,
    tile_species: Vec<Vec<usize>>,
    rivers: Vec<Vec<usize>>,
    river_routes: Vec<Vec<usize>>,
    river_tiles: Vec<bool>,
    capacities: Vec<f32>,
    tile_communities: BTreeMap<usize, Vec<Community>>,
    tile_states: BTreeMap<usize, u32>,
    states: BTreeMap<u32, StateRecord>,
    languages: BTreeMap<u32, LanguageRecord>,
    nationalities: BTreeSet<u32>,
    trade_partners: Vec<(u32, u32)>,
    conflicts: Vec<(u32, u32)>,
    next_state_id: u32,
    next_language_id: u32,
    rng: Pcg64Mcg,
}

impl HistorySimulation {
    pub fn new_earth(seed: u128) -> Self {
        let prehistory = PrehistorySimulation::new_earth(seed);
        Self::from_prehistory(prehistory, seed)
    }

    pub fn from_prehistory(mut prehistory: PrehistorySimulation, seed: u128) -> Self {
        // Ensure prehistory has populated agricultural tiles if starting directly on Earth or an unpopulated planet.
        if prehistory.active_agricultural_race_count() == 0 {
            for _ in 0..8 {
                prehistory.step();
                if prehistory.active_agricultural_race_count() > 0 && prehistory.populated_count() > 80 {
                    break;
                }
            }
        }

        let coarse_elevs: Vec<f32> = prehistory.tiles().iter().map(|t| t.elevation()).collect();
        let coarse_mounts: Vec<f32> = prehistory.tiles().iter().map(|t| t.mountainosity).collect();
        let terrain =
            SubdividedTerrainGrid::from_coarse(prehistory.grid(), &coarse_elevs, &coarse_mounts);
        let num_fine = terrain.terrain_grid.faces.len();

        let indexed_pts: Vec<(Vec3, usize)> = terrain
            .display_faces
            .iter()
            .enumerate()
            .map(|(i, (c, _, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);

        let mut sim = Self {
            prehistory,
            terrain,
            tree,
            history_ticks: 0,
            settled_species: Vec::new(),
            tile_species: vec![Vec::new(); num_fine],
            rivers: Vec::new(),
            river_routes: Vec::new(),
            river_tiles: vec![false; num_fine],
            capacities: vec![0.0; num_fine],
            tile_communities: BTreeMap::new(),
            tile_states: BTreeMap::new(),
            states: BTreeMap::new(),
            languages: BTreeMap::new(),
            nationalities: BTreeSet::new(),
            trade_partners: Vec::new(),
            conflicts: Vec::new(),
            next_state_id: 0,
            next_language_id: 0,
            rng: Pcg64Mcg::new(seed),
        };
        sim.initialize_terrain_and_polities();
        sim
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(42);
    }

    pub fn reset(&mut self) {
        self.history_ticks = 0;
        self.initialize_terrain_and_polities();
    }

    fn initialize_terrain_and_polities(&mut self) {
        let grid = self.prehistory.grid().clone();
        let coarse_adj = self.prehistory.adjacency().to_vec();
        let coarse_elevs = self.prehistory.tile_elevations();
        let koeppen = self.prehistory.tile_koeppen_ids().to_vec();
        let precip = self.prehistory.tile_precip_norm().to_vec();
        let num_coarse = coarse_elevs.len();
        let num_fine = self.terrain.terrain_grid.faces.len();
        let fine_adj = self.terrain.terrain_adj.clone();
        let fine_elevs = self.terrain.fine_elevations.clone();

        // 1. Settle domesticable plants & animals on the planet's coarse climate,
        //    then map onto subdivided fine faces (`historysimulation.py:facespecies`).
        let climate_for_species = tec11_climate::ClimateModel::with_elevations(
            grid.clone(),
            coarse_elevs.iter().map(|&h| h.max(0.0)).collect(),
            3,
            self.prehistory.glaciation(),
        );
        let biosphere = settle_species_on_climate(&coarse_elevs, &climate_for_species);
        self.settled_species = biosphere.species;
        let mut coarse_species: Vec<Vec<usize>> = vec![Vec::new(); num_coarse];
        for (sp_idx, sp) in self.settled_species.iter().enumerate() {
            for &(t, _) in &sp.tile_seasons {
                if t < num_coarse {
                    coarse_species[t].push(sp_idx);
                }
            }
        }
        self.tile_species = Vec::with_capacity(num_fine);
        for f_idx in 0..num_fine {
            let sp_vec = match self.terrain.parentage[f_idx] {
                FineParentage::CoarseFace(c)
                | FineParentage::CoarseEdge {
                    primary_coarse: c, ..
                } => coarse_species[c].clone(),
                FineParentage::CoarseVertex([c0, c1, c2]) => {
                    let mut set = BTreeSet::new();
                    for &c in &[c0, c1, c2] {
                        for &sp in &coarse_species[c] {
                            set.insert(sp);
                        }
                    }
                    set.into_iter().collect()
                }
            };
            self.tile_species.push(sp_vec);
        }

        // 2. Trace coarse rivers (`hmin = 2.0`, `pmin = 0.25`) and route them on the subdivided terrain grid (`routerivers`).
        self.rivers = trace_coarse_rivers(
            &coarse_elevs,
            &precip,
            &koeppen,
            &coarse_adj,
            HISTORY_MIN_RIVER_ELEV,
            HISTORY_MIN_RIVER_PRECIP,
        );
        self.river_routes = self.terrain.route_rivers(&grid, &self.rivers);
        self.river_tiles = vec![false; num_fine];
        for route in &self.river_routes {
            for &f_idx in route {
                if f_idx < num_fine {
                    self.river_tiles[f_idx] = true;
                }
            }
        }

        // 3. Carrying capacity on subdivided fine faces (`historysimulation.py:capacity`, 1/9 of coarse parent).
        let ocean = main_ocean(&coarse_elevs, &coarse_adj);
        let mut coarse_caps = vec![0.0f32; num_coarse];
        for c in 0..num_coarse {
            if coarse_elevs[c] <= 0.0 || !is_habitable(coarse_elevs[c], koeppen[c]) {
                continue;
            }
            let base =
                tile_population_thousands(koeppen[c], is_farmable(coarse_elevs[c], koeppen[c]));
            let coast_mult = if coarse_adj[c].iter().any(|nb| ocean.contains(nb)) {
                1.25
            } else {
                1.0
            };
            let sp_mult = ((coarse_species[c].len() as f32).sqrt()).max(1.0);
            coarse_caps[c] = (base * coast_mult * sp_mult).max(1.8);
        }

        self.capacities = vec![0.0; num_fine];
        let mut coast_fine = Vec::new();
        for f_idx in 0..num_fine {
            if fine_elevs[f_idx] <= 0.0 {
                continue;
            }
            let river_mult = if self.river_tiles[f_idx] { 1.75 } else { 1.0 };
            match self.terrain.parentage[f_idx] {
                FineParentage::CoarseVertex(cs) => {
                    let elevated: Vec<usize> = cs
                        .iter()
                        .copied()
                        .filter(|&c| coarse_elevs[c] > 0.0)
                        .collect();
                    if !elevated.is_empty() {
                        let sum_cap: f32 = elevated.iter().map(|&c| coarse_caps[c]).sum();
                        self.capacities[f_idx] =
                            (sum_cap / (9.0 * elevated.len() as f32)) * river_mult;
                    }
                }
                FineParentage::CoarseFace(c) => {
                    self.capacities[f_idx] = (coarse_caps[c] / 9.0) * river_mult;
                }
                FineParentage::CoarseEdge { primary_coarse, .. } => {
                    if coarse_elevs[primary_coarse] <= 0.0 {
                        coast_fine.push(f_idx);
                    } else {
                        self.capacities[f_idx] = (coarse_caps[primary_coarse] / 9.0) * river_mult;
                    }
                }
            }
        }
        // Fill in coastal edge fine faces whose primary coarse tile was underwater with average of nonzero fine neighbors
        for &f_idx in &coast_fine {
            let nbs: Vec<f32> = fine_adj[f_idx]
                .iter()
                .map(|&nb| self.capacities[nb])
                .filter(|&cap| cap > 0.0)
                .collect();
            if !nbs.is_empty() {
                self.capacities[f_idx] = nbs.iter().sum::<f32>() / (nbs.len() as f32);
            }
        }

        // 4. Seed cities & initial states across farmable fine faces (`historysimulation.py:stateboundaries`)
        self.tile_communities.clear();
        self.tile_states.clear();
        self.states.clear();
        self.languages.clear();
        self.nationalities.clear();
        self.trade_partners.clear();
        self.conflicts.clear();
        self.next_state_id = 0;
        self.next_language_id = 0;

        let mut candidate_tiles: Vec<usize> = (0..num_fine)
            .filter(|&f_idx| {
                let c = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                fine_elevs[f_idx] > 0.0
                    && is_farmable(fine_elevs[f_idx], koeppen[c])
                    && self.capacities[f_idx] >= 0.5
            })
            .collect();
        candidate_tiles.sort_by(|&a, &b| {
            self.capacities[b]
                .partial_cmp(&self.capacities[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut city_tiles: Vec<usize> = Vec::new();
        let mut too_close = vec![false; num_fine];
        for &cand in &candidate_tiles {
            if too_close[cand] {
                continue;
            }
            city_tiles.push(cand);
            if city_tiles.len() >= 18 {
                break;
            }
            // Mark fine faces within 8 steps as too close to another city (3 coarse steps ~ 9 fine steps)
            let mut visited = BTreeSet::new();
            let mut q = VecDeque::new();
            visited.insert(cand);
            q.push_back((cand, 0usize));
            while let Some((curr, dist)) = q.pop_front() {
                too_close[curr] = true;
                if dist < 8 {
                    for &nb in &fine_adj[curr] {
                        if visited.insert(nb) {
                            q.push_back((nb, dist + 1));
                        }
                    }
                }
            }
        }

        // Create initial states, nationalities, and languages at each city
        let mut q = VecDeque::new();
        for &city_f in &city_tiles {
            let sid = self.next_state_id;
            self.next_state_id += 1;
            let lid = self.next_language_id;
            self.next_language_id += 1;

            let mut lang = LanguageRecord::new(lid, &mut self.rng);
            lang.coin_concept(ConceptKey::State(sid), &mut self.rng);
            lang.coin_concept(ConceptKey::Nation(sid), &mut self.rng);
            for &sp_idx in &self.tile_species[city_f] {
                lang.coin_concept(ConceptKey::Species(sp_idx as u32), &mut self.rng);
            }
            self.languages.insert(lid, lang);
            self.nationalities.insert(sid);
            self.states.insert(
                sid,
                StateRecord {
                    id: sid,
                    capital_tile: city_f,
                    color_idx: (sid as usize) % NATION_PALETTE.len(),
                    active: true,
                },
            );
            self.tile_states.insert(city_f, sid);
            q.push_back((city_f, sid, 0usize));
        }

        // Expand state boundaries up to 14 fine steps over passable fine faces (`historysimulation.py:nearestcity`)
        while let Some((curr, sid, dist)) = q.pop_front() {
            if dist >= 14 {
                continue;
            }
            for &nb in &fine_adj[curr] {
                let c = self.terrain.primary_coarse_tile(nb, &coarse_elevs);
                let step_cost = if self.river_tiles[nb] { 3 } else { 1 };
                if dist + step_cost <= 14
                    && !self.tile_states.contains_key(&nb)
                    && fine_elevs[nb] > 0.0
                    && is_passable(fine_elevs[nb], koeppen[c], true)
                {
                    self.tile_states.insert(nb, sid);
                    if let Some(lang) = self.languages.get_mut(&sid) {
                        for &sp_idx in &self.tile_species[nb] {
                            lang.coin_concept(ConceptKey::Species(sp_idx as u32), &mut self.rng);
                        }
                    }
                    q.push_back((nb, sid, dist + step_cost));
                }
            }
        }

        // Initialize communities on claimed fine faces (`historysimulation.py:population`, 1/9 or 1/27 shares)
        let pre_pop = self.prehistory.populated().clone();
        for (&f_idx, &sid) in &self.tile_states {
            let mut mix = BTreeMap::new();
            match self.terrain.parentage[f_idx] {
                FineParentage::CoarseVertex(cs) => {
                    for &c in &cs {
                        if let Some(&hid) = pre_pop.get(&c) {
                            *mix.entry(hid).or_insert(0.0) += 1.0 / 3.0;
                        }
                    }
                }
                FineParentage::CoarseFace(c)
                | FineParentage::CoarseEdge {
                    primary_coarse: c, ..
                } => {
                    if let Some(&hid) = pre_pop.get(&c) {
                        mix.insert(hid, 1.0);
                    }
                }
            }
            if mix.is_empty() {
                mix.insert(0, 1.0);
            }
            let pop = (self.capacities[f_idx] * 0.45).max(0.15);
            let mut comm = Community {
                thousands: pop,
                nationality_id: sid,
                language_id: sid,
                agriculture: true,
                racial_mix: mix,
            };
            comm.normalize_racial_mix();
            self.tile_communities.insert(f_idx, vec![comm]);
        }

        self.recolor_states(&fine_adj);
        self.update_diplomacy_and_trade(&fine_adj);
    }

    /// Greedy 6-coloring of adjacent states (`terrainmethod.py:colornations`).
    fn recolor_states(&mut self, adjacency: &[Vec<usize>]) {
        let mut state_adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for (&t, &s1) in &self.tile_states {
            state_adj.entry(s1).or_default();
            for &nb in &adjacency[t] {
                if let Some(&s2) = self.tile_states.get(&nb) {
                    if s1 != s2 {
                        state_adj.entry(s1).or_default().insert(s2);
                        state_adj.entry(s2).or_default().insert(s1);
                    }
                }
            }
        }

        let state_ids: Vec<u32> = self.states.keys().copied().collect();
        for sid in state_ids {
            let mut used_colors = BTreeSet::new();
            if let Some(nbs) = state_adj.get(&sid) {
                for &nb_sid in nbs {
                    if nb_sid < sid {
                        if let Some(st) = self.states.get(&nb_sid) {
                            used_colors.insert(st.color_idx);
                        }
                    }
                }
            }
            let chosen = (0..NATION_PALETTE.len())
                .find(|c| !used_colors.contains(c))
                .unwrap_or((sid as usize) % NATION_PALETTE.len());
            if let Some(st) = self.states.get_mut(&sid) {
                st.color_idx = chosen;
            }
        }
    }

    fn update_diplomacy_and_trade(&mut self, adjacency: &[Vec<usize>]) {
        self.trade_partners.clear();
        self.conflicts.clear();

        let mut active_states: BTreeSet<u32> = BTreeSet::new();
        for &sid in self.tile_states.values() {
            active_states.insert(sid);
        }
        for st in self.states.values_mut() {
            st.active = active_states.contains(&st.id);
        }

        // Compute per-state species sets and border adjacency (including across river fine faces: `riverneighbors`)
        let mut state_species: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
        let mut border_shared: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for (&t, &s1) in &self.tile_states {
            let sp_set = state_species.entry(s1).or_default();
            for &sp in &self.tile_species[t] {
                sp_set.insert(sp);
            }
            for &nb in &adjacency[t] {
                if let Some(&s2) = self.tile_states.get(&nb) {
                    if s1 != s2 {
                        let pair = if s1 < s2 { (s1, s2) } else { (s2, s1) };
                        *border_shared.entry(pair).or_insert(0) += 1;
                    }
                } else if self.river_tiles[nb] {
                    for &r_nb in &adjacency[nb] {
                        if let Some(&s2) = self.tile_states.get(&r_nb) {
                            if s1 != s2 {
                                let pair = if s1 < s2 { (s1, s2) } else { (s2, s1) };
                                *border_shared.entry(pair).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }

        let active_vec: Vec<u32> = active_states.into_iter().collect();
        for i in 0..active_vec.len() {
            for j in (i + 1)..active_vec.len() {
                let s1 = active_vec[i];
                let s2 = active_vec[j];
                let c1 = self
                    .states
                    .get(&s1)
                    .and_then(|s| self.terrain.terrain_grid.faces.get(s.capital_tile))
                    .map(|(c, _)| *c)
                    .unwrap_or([1.0, 0.0, 0.0]);
                let c2 = self
                    .states
                    .get(&s2)
                    .and_then(|s| self.terrain.terrain_grid.faces.get(s.capital_tile))
                    .map(|(c, _)| *c)
                    .unwrap_or([1.0, 0.0, 0.0]);
                let dot = (c1[0] * c2[0] + c1[1] * c2[1] + c1[2] * c2[2]).clamp(-1.0, 1.0);
                let angle = dot.acos();

                let sp1 = state_species.get(&s1);
                let sp2 = state_species.get(&s2);
                let diff_species = match (sp1, sp2) {
                    (Some(a), Some(b)) => a.symmetric_difference(b).count(),
                    _ => 0,
                };

                let shared_border = border_shared.get(&(s1, s2)).copied().unwrap_or(0);
                if shared_border >= 3 && self.rng.uniform(0.0, 1.0) < 0.35 {
                    self.conflicts.push((s1, s2));
                } else if angle < 1.15 && diff_species >= 2 && self.rng.uniform(0.0, 1.0) < 0.45 {
                    self.trade_partners.push((s1, s2));
                    // Borrow words for traded species and state names (`historysimulation.py:283-294`)
                    let l1 = self.primary_language_for_state(s1);
                    let l2 = self.primary_language_for_state(s2);
                    if let (Some(lid1), Some(lid2)) = (l1, l2) {
                        if lid1 != lid2 {
                            self.exchange_loanwords(s1, lid1, s2, lid2);
                        }
                    }
                }
            }
        }
    }

    fn exchange_loanwords(&mut self, s1: u32, lid1: u32, s2: u32, lid2: u32) {
        let Some(donor1) = self.languages.get(&lid1).cloned() else {
            return;
        };
        let Some(donor2) = self.languages.get(&lid2).cloned() else {
            return;
        };
        if let Some(lang1) = self.languages.get_mut(&lid1) {
            lang1.borrow_concept(ConceptKey::State(s2), &donor2);
            lang1.borrow_concept(ConceptKey::Nation(s2), &donor2);
            lang1.borrow_concept(ConceptKey::Language(lid2), &donor2);
            for (&k, _) in &donor2.lexicon {
                if matches!(k, ConceptKey::Species(_)) {
                    lang1.borrow_concept(k, &donor2);
                }
            }
        }
        if let Some(lang2) = self.languages.get_mut(&lid2) {
            lang2.borrow_concept(ConceptKey::State(s1), &donor1);
            lang2.borrow_concept(ConceptKey::Nation(s1), &donor1);
            lang2.borrow_concept(ConceptKey::Language(lid1), &donor1);
            for (&k, _) in &donor1.lexicon {
                if matches!(k, ConceptKey::Species(_)) {
                    lang2.borrow_concept(k, &donor1);
                }
            }
        }
    }

    /// Advance the History simulation by 1 tick (`200` years, matching `HistorySimulation.update`
    /// in `historysimulation.py:222-335`).
    pub fn step(&mut self) {
        let fine_adj = self.terrain.terrain_adj.clone();
        let fine_elevs = self.terrain.fine_elevations.clone();
        let coarse_elevs = self.prehistory.tile_elevations();
        let koeppen = self.prehistory.tile_koeppen_ids().to_vec();

        // 1. Logistic growth and spillover on subdivided fine faces (`historysimulation.py:168-220`)
        let tile_pops: BTreeMap<usize, f32> = self
            .tile_communities
            .iter()
            .map(|(&t, cs)| (t, cs.iter().map(|c| c.thousands).sum()))
            .collect();
        let mut spillover: Vec<(usize, u32, Community)> = Vec::new();
        for (&t, comms) in self.tile_communities.iter_mut() {
            let cap = self.capacities[t].max(0.2);
            let total: f32 = comms.iter().map(|c| c.thousands).sum();
            let r = 0.08f32;
            let factor = (1.0 + r * (1.0 - total / cap)).clamp(0.85, 1.15);
            for c in comms.iter_mut() {
                c.thousands *= factor;
            }
            let new_total: f32 = comms.iter().map(|c| c.thousands).sum();
            if new_total > cap * 0.85 && !comms.is_empty() {
                let sid = self.tile_states.get(&t).copied().unwrap_or(0);
                for &nb in &fine_adj[t] {
                    let c = self.terrain.primary_coarse_tile(nb, &coarse_elevs);
                    if fine_elevs[nb] > 0.0
                        && is_passable(fine_elevs[nb], koeppen[c], true)
                        && self.capacities[nb] > 0.1
                    {
                        let nb_pop: f32 = tile_pops.get(&nb).copied().unwrap_or(0.0);
                        if nb_pop < self.capacities[nb] * 0.9 {
                            let mut migrant = comms[0].clone();
                            migrant.thousands = (comms[0].thousands * 0.08).max(0.02);
                            comms[0].thousands = (comms[0].thousands - migrant.thousands).max(0.02);
                            spillover.push((nb, sid, migrant));
                            break;
                        }
                    }
                }
            }
        }

        for (nb, sid, migrant) in spillover {
            self.tile_states.entry(nb).or_insert(sid);
            self.tile_communities.entry(nb).or_default().push(migrant);
        }

        // 2. Intermingle and assimilate (`people/community.py`)
        for comms in self.tile_communities.values_mut() {
            if comms.len() > 1 {
                comms.sort_by(|a, b| {
                    b.thousands
                        .partial_cmp(&a.thousands)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let dom_nat = comms[0].nationality_id;
                let dom_lang = comms[0].language_id;
                let mut merged = comms.remove(0);
                let mut kept = Vec::new();
                for mut c in comms.drain(..) {
                    if (c.nationality_id == dom_nat && c.language_id == dom_lang)
                        || c.thousands < 0.05
                    {
                        merged.merge_from(&c);
                    } else {
                        let shift = c.thousands * 0.15;
                        c.thousands -= shift;
                        let mut shifted = c.clone();
                        shifted.thousands = shift;
                        merged.merge_from(&shifted);
                        kept.push(c);
                    }
                }
                kept.insert(0, merged);
                *comms = kept;
            }
        }

        // 3. Language sound changes (`historysimulation.py:242-255`)
        let lang_ids: Vec<u32> = self.languages.keys().copied().collect();
        for lid in lang_ids {
            if self.rng.uniform(0.0, 1.0) < 0.65 {
                let mut temp_rng = self.rng.clone();
                if let Some(lang) = self.languages.get_mut(&lid) {
                    lang.mutate_tick(&mut temp_rng);
                }
                self.rng = temp_rng;
            }
        }

        // 4. Resolve border conflicts (`historysimulation.py:305-322`)
        let current_conflicts = self.conflicts.clone();
        for (s1, s2) in current_conflicts {
            let pop1 = self.state_population_thousands(s1);
            let pop2 = self.state_population_thousands(s2);
            let (victor, loser) = if pop1 >= pop2 { (s1, s2) } else { (s2, s1) };
            let border_tiles: Vec<usize> = self
                .tile_states
                .iter()
                .filter_map(|(&t, &st)| {
                    if st == loser
                        && fine_adj[t]
                            .iter()
                            .any(|nb| self.tile_states.get(nb) == Some(&victor))
                    {
                        Some(t)
                    } else {
                        None
                    }
                })
                .take(6)
                .collect();
            for t in border_tiles {
                self.tile_states.insert(t, victor);
            }
        }

        // 5. Update trade & diplomacy and recolor nations
        self.recolor_states(&fine_adj);
        self.update_diplomacy_and_trade(&fine_adj);
        self.history_ticks += 1;
    }

    pub fn grid(&self) -> &SphericalGrid {
        self.prehistory.grid()
    }

    pub fn terrain(&self) -> &SubdividedTerrainGrid {
        &self.terrain
    }

    pub fn build_display_mesh(&self) -> (Vec<f32>, Vec<f32>, Vec<u32>) {
        self.terrain.build_display_mesh()
    }

    pub fn tile_count(&self) -> usize {
        self.terrain.display_faces.len()
    }

    pub fn fine_face_count(&self) -> usize {
        self.terrain.terrain_grid.faces.len()
    }

    pub fn history_ticks(&self) -> u32 {
        self.history_ticks
    }

    pub fn years(&self) -> u32 {
        self.history_ticks * HISTORY_YEARS_PER_TICK
    }

    pub fn is_custom_planet(&self) -> bool {
        self.prehistory.is_custom_planet()
    }

    pub fn land_percent(&self) -> u32 {
        self.prehistory.land_percent()
    }

    pub fn active_state_count(&self) -> usize {
        let set: BTreeSet<u32> = self.tile_states.values().copied().collect();
        set.len()
    }

    pub fn total_population_thousands(&self) -> f32 {
        self.tile_communities
            .values()
            .flat_map(|cs| cs.iter())
            .map(|c| c.thousands)
            .sum()
    }

    pub fn state_population_thousands(&self, state_id: u32) -> f32 {
        self.tile_states
            .iter()
            .filter(|(_, &sid)| sid == state_id)
            .filter_map(|(t, _)| self.tile_communities.get(t))
            .flatten()
            .map(|c| c.thousands)
            .sum()
    }

    pub fn primary_language_for_state(&self, state_id: u32) -> Option<u32> {
        let mut counts: BTreeMap<u32, f32> = BTreeMap::new();
        for (&t, &sid) in &self.tile_states {
            if sid == state_id {
                if let Some(comms) = self.tile_communities.get(&t) {
                    for c in comms {
                        *counts.entry(c.language_id).or_insert(0.0) += c.thousands;
                    }
                }
            }
        }
        counts
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(lid, _)| lid)
    }

    pub fn state_name(&self, state_id: u32) -> String {
        if let Some(lid) = self.primary_language_for_state(state_id) {
            if let Some(lang) = self.languages.get(&lid) {
                if let Some(orig) = lang.lexicon.get(&ConceptKey::State(state_id)) {
                    return orig.word.write(true);
                }
            }
        }
        format!("State {}", state_id + 1)
    }

    pub fn nation_name(&self, nat_id: u32, display_lang_id: Option<u32>) -> String {
        let lid = display_lang_id.unwrap_or(nat_id);
        if let Some(lang) = self.languages.get(&lid) {
            if let Some(orig) = lang.lexicon.get(&ConceptKey::Nation(nat_id)) {
                return orig.word.write(true);
            }
        }
        format!("Nation {}", nat_id + 1)
    }

    pub fn nearest_tile(&self, vec_grid: Vec3) -> usize {
        self.tree.nearest(normal(vec_grid))
    }

    pub fn state_at_tile(&self, tile_idx: usize) -> Option<u32> {
        self.tile_states.get(&tile_idx).copied()
    }

    /// Detailed multi-line inspection report for a clicked display face and its State (`historypresenter.py`).
    pub fn inspect_tile(&self, display_idx: usize) -> String {
        if display_idx >= self.terrain.display_faces.len() {
            return String::new();
        }
        let fine_idx = match self.terrain.display_faces[display_idx].2 {
            DisplayFaceSource::OceanCoarse(c_idx) => {
                return format!("Ocean Tile #{c_idx}");
            }
            DisplayFaceSource::Fine(f) => f,
        };
        let fine_elev = self.terrain.fine_elevations[fine_idx];
        if fine_elev <= 0.0 {
            return format!("Coastal Bay / Sea Face #{fine_idx} ({fine_elev:.2} km)");
        }

        let mut lines = Vec::new();
        let tile_pop: f32 = self
            .tile_communities
            .get(&fine_idx)
            .map(|cs| cs.iter().map(|c| c.thousands).sum())
            .unwrap_or(0.0);
        let cap = self.capacities[fine_idx];
        let sp_names: Vec<&str> = self.tile_species[fine_idx]
            .iter()
            .filter_map(|&idx| self.settled_species.get(idx).map(|s| s.name.as_str()))
            .collect();

        let Some(&sid) = self.tile_states.get(&fine_idx) else {
            lines.push(format!(
                "Fine Face #{fine_idx} (Unclaimed Land, {fine_elev:.2} km) | Pop: {:.1}K / Cap: {:.1}K",
                tile_pop, cap
            ));
            if !sp_names.is_empty() {
                lines.push(format!("Native Species: {}", sp_names.join(", ")));
            }
            return lines.join("\n");
        };

        let st_name = self.state_name(sid);
        let st_pop = self.state_population_thousands(sid);
        let dom_lang = self.primary_language_for_state(sid);
        lines.push(format!(
            "State: {st_name} | Pop: {:.0}K | Fine Face #{fine_idx} ({fine_elev:.2} km, {:.1}K / {:.1}K)",
            st_pop, tile_pop, cap
        ));

        // Nationalities & Languages breakdown
        let mut nat_totals: BTreeMap<u32, f32> = BTreeMap::new();
        let mut lang_totals: BTreeMap<u32, f32> = BTreeMap::new();
        for (&t, &st) in &self.tile_states {
            if st == sid {
                if let Some(comms) = self.tile_communities.get(&t) {
                    for c in comms {
                        *nat_totals.entry(c.nationality_id).or_insert(0.0) += c.thousands;
                        *lang_totals.entry(c.language_id).or_insert(0.0) += c.thousands;
                    }
                }
            }
        }
        let denom = st_pop.max(1e-3);
        let nat_strs: Vec<String> = nat_totals
            .iter()
            .map(|(&nid, &p)| {
                format!(
                    "{} ({:.0}%)",
                    self.nation_name(nid, dom_lang),
                    (p * 100.0 / denom)
                )
            })
            .collect();
        if !nat_strs.is_empty() {
            lines.push(format!("Nationalities: {}", nat_strs.join(", ")));
        }

        let lang_strs: Vec<String> = lang_totals
            .iter()
            .map(|(&lid, &p)| {
                let lname = self
                    .languages
                    .get(&lid)
                    .map(|l| l.name())
                    .unwrap_or_else(|| format!("Lang_{}", lid + 1));
                format!("{lname} ({:.0}%)", p * 100.0 / denom)
            })
            .collect();
        if !lang_strs.is_empty() {
            lines.push(format!("Languages: {}", lang_strs.join(", ")));
        }

        // Trade partners & Conflicts
        let partners: Vec<String> = self
            .trade_partners
            .iter()
            .filter_map(|&(a, b)| {
                if a == sid {
                    Some(self.state_name(b))
                } else if b == sid {
                    Some(self.state_name(a))
                } else {
                    None
                }
            })
            .collect();
        lines.push(format!(
            "Trade Partners (yellow): {}",
            if partners.is_empty() {
                "None".to_string()
            } else {
                partners.join(", ")
            }
        ));

        let rivals: Vec<String> = self
            .conflicts
            .iter()
            .filter_map(|&(a, b)| {
                if a == sid {
                    Some(self.state_name(b))
                } else if b == sid {
                    Some(self.state_name(a))
                } else {
                    None
                }
            })
            .collect();
        lines.push(format!(
            "Conflicts (red): {}",
            if rivals.is_empty() {
                "None".to_string()
            } else {
                rivals.join(", ")
            }
        ));

        // Sample lexicon from primary language
        if let Some(lid) = dom_lang {
            if let Some(lang) = self.languages.get(&lid) {
                lines.push(format!("--- {} Lexicon ---", lang.name()));
                for (&ck, orig) in lang.lexicon.iter().take(8) {
                    let gloss = match ck {
                        ConceptKey::Species(sp_id) => self
                            .settled_species
                            .get(sp_id as usize)
                            .map(|s| {
                                let k = match s.kind {
                                    SpeciesKind::Animal => "animal",
                                    SpeciesKind::Plant => "plant",
                                };
                                format!("{} ({k})", s.name)
                            })
                            .unwrap_or_else(|| format!("species_{sp_id}")),
                        ConceptKey::State(id) => format!("state #{}", id + 1),
                        ConceptKey::Nation(id) => format!("nation #{}", id + 1),
                        ConceptKey::Language(id) => format!("language #{}", id + 1),
                    };
                    lines.push(format!(
                        "{}: {} [{}] ({})",
                        gloss,
                        orig.word.write(false),
                        orig.word.pronounce(),
                        orig.format_etymology()
                    ));
                }
            }
        }

        lines.join("\n")
    }

    /// Compute per-display-face RGB colors `[r0, g0, b0, ...]` for the given `aspect`
    /// (`"nations"`, `"species"`, `"population"`, `"capacity"`), `show_rivers`, and `selected_tile`
    /// (`historydisplay.py:21-84`).
    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_rivers: bool,
        selected_tile: Option<usize>,
    ) -> Vec<f32> {
        let coarse_elevs = self.prehistory.tile_elevations();
        let felsities = self.prehistory.tile_felsities();
        let koeppen = self.prehistory.tile_koeppen_ids();
        let life = self.prehistory.tile_life();
        let num_display = self.terrain.display_faces.len();

        let selected_state = selected_tile.and_then(|d_idx| {
            match self.terrain.display_faces.get(d_idx)?.2 {
                DisplayFaceSource::Fine(f) => self.tile_states.get(&f).copied(),
                DisplayFaceSource::OceanCoarse(_) => None,
            }
        });
        let mut partner_states = BTreeSet::new();
        let mut rival_states = BTreeSet::new();
        if let Some(sel_sid) = selected_state {
            for &(a, b) in &self.trade_partners {
                if a == sel_sid {
                    partner_states.insert(b);
                } else if b == sel_sid {
                    partner_states.insert(a);
                }
            }
            for &(a, b) in &self.conflicts {
                if a == sel_sid {
                    rival_states.insert(b);
                } else if b == sel_sid {
                    rival_states.insert(a);
                }
            }
        }

        let max_cap = self.capacities.iter().copied().fold(1.0f32, f32::max);
        let max_sp = self
            .tile_species
            .iter()
            .map(|v| v.len())
            .max()
            .unwrap_or(1)
            .max(1) as f32;

        let mut out = Vec::with_capacity(num_display * 3);
        for (_, _, source) in &self.terrain.display_faces {
            let f_idx = match *source {
                DisplayFaceSource::OceanCoarse(_) => {
                    out.extend_from_slice(&[0.0, 0.0, 0.55]);
                    continue;
                }
                DisplayFaceSource::Fine(f) => f,
            };

            if self.terrain.fine_elevations[f_idx] <= 0.0 {
                out.extend_from_slice(&[0.0, 0.0, 0.55]);
                continue;
            }
            if show_rivers && self.river_tiles[f_idx] {
                out.extend_from_slice(&[0.35, 0.88, 0.96]);
                continue;
            }

            let c_idx = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);

            match aspect {
                "species" => {
                    let s = (self.tile_species[f_idx].len() as f32 / max_sp).clamp(0.0, 1.0);
                    out.extend_from_slice(&[0.15 * (1.0 - s), 0.2 + 0.75 * s, 0.12]);
                }
                "population" => {
                    let pop: f32 = self
                        .tile_communities
                        .get(&f_idx)
                        .map(|cs| cs.iter().map(|c| c.thousands).sum())
                        .unwrap_or(0.0);
                    let s = ((1.0 + pop).ln() / (1.0 + max_cap).ln()).clamp(0.0, 1.0);
                    out.extend_from_slice(&[s, 0.35 * s, 0.1 * (1.0 - s)]);
                }
                "capacity" => {
                    let s = (self.capacities[f_idx] / max_cap).clamp(0.0, 1.0);
                    out.extend_from_slice(&[0.2 * s, 0.85 * s, 0.3 * s]);
                }
                _ => {
                    // "nations" aspect (`historydisplay.py:39-72`)
                    if let Some(&sid) = self.tile_states.get(&f_idx) {
                        if selected_state == Some(sid) {
                            out.extend_from_slice(&[0.15, 0.95, 0.35]); // Selected state: bright green
                        } else if rival_states.contains(&sid) {
                            out.extend_from_slice(&[0.98, 0.15, 0.15]); // Conflict rival: bright red
                        } else if partner_states.contains(&sid) {
                            out.extend_from_slice(&[0.98, 0.92, 0.18]); // Trade partner: bright yellow
                        } else {
                            let cidx = self
                                .states
                                .get(&sid)
                                .map(|st| st.color_idx)
                                .unwrap_or((sid as usize) % NATION_PALETTE.len());
                            out.extend_from_slice(&NATION_PALETTE[cidx]);
                        }
                    } else if koeppen[c_idx] == 12 || koeppen[c_idx] == 13 {
                        out.extend_from_slice(&[0.95, 0.97, 1.0]);
                    } else {
                        let f = felsities[c_idx];
                        let r = 0.25;
                        let g = (32.0 + 32.0 * f + 127.5 * life[c_idx]) / 255.0;
                        let b = (64.0 * f) / 255.0;
                        out.extend_from_slice(&[r, g, b]);
                    }
                }
            }
        }
        out
    }

    pub fn to_snapshot(&self) -> tec11_proto::PlanetSnapshot {
        let pre_snap = self.prehistory.to_snapshot();
        let pre_stage = self.prehistory.to_proto_prehistory_stage();

        let populated_faces: Vec<tec11_proto::PopulatedFineFace> = self
            .tile_communities
            .iter()
            .map(|(&t, comms)| {
                let state_id = self.tile_states.get(&t).copied().unwrap_or(0);
                let proto_comms = comms
                    .iter()
                    .map(|c| tec11_proto::Community {
                        thousands: c.thousands,
                        nationality_id: c.nationality_id,
                        language_id: c.language_id,
                        agriculture: c.agriculture,
                        racial_mix: c
                            .racial_mix
                            .iter()
                            .map(|(&hid, &frac)| tec11_proto::RaceContribution {
                                heritage_id: hid,
                                fraction: frac,
                            })
                            .collect(),
                    })
                    .collect();
                tec11_proto::PopulatedFineFace {
                    fine_face_id: t as u32,
                    state_id,
                    communities: proto_comms,
                }
            })
            .collect();

        let proto_langs: Vec<tec11_proto::LanguageHistory> = self
            .languages
            .values()
            .map(|l| tec11_proto::LanguageHistory {
                concepts: l.lexicon.keys().copied().map(ConceptKey::to_proto).collect(),
                changes_per_tick: l
                    .changes_per_tick
                    .iter()
                    .map(|changes| tec11_proto::TickSoundChanges {
                        sound_change_kinds: changes.clone(),
                    })
                    .collect(),
                neologisms: Vec::new(),
                loans: Vec::new(),
                derivations: Vec::new(),
                redefinitions: Vec::new(),
            })
            .collect();

        let trade_partners = self
            .trade_partners
            .iter()
            .map(|&(a, b)| tec11_proto::StatePair {
                state_a: a,
                state_b: b,
            })
            .collect();
        let conflicts = self
            .conflicts
            .iter()
            .map(|&(a, b)| tec11_proto::StatePair {
                state_a: a,
                state_b: b,
            })
            .collect();

        tec11_proto::PlanetSnapshot {
            rng_state_16b: self.rng.to_bytes().to_vec(),
            grid_size: pre_snap.grid_size,
            radius_km: pre_snap.radius_km,
            spin: pre_snap.spin,
            tilt_deg: pre_snap.tilt_deg,
            stage: Some(tec11_proto::PlanetStageOneof::History(
                tec11_proto::HistoryStage {
                    prehistory_base: Some(pre_stage),
                    history_ticks: self.history_ticks,
                    populated_faces,
                    languages: proto_langs,
                    trade_partners,
                    conflicts,
                },
            )),
        }
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        self.to_snapshot().encode_bytes()
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(pre) = PrehistorySimulation::from_snapshot(&snap, 42) else {
            return false;
        };
        let mut sim = Self::from_prehistory(pre, 42);
        if let Some(tec11_proto::PlanetStageOneof::History(hist)) = &snap.stage {
            sim.history_ticks = hist.history_ticks;
            if !hist.populated_faces.is_empty() {
                sim.tile_states.clear();
                sim.tile_communities.clear();
                let num_fine = sim.fine_face_count();
                for pf in &hist.populated_faces {
                    let fid = pf.fine_face_id as usize;
                    if fid < num_fine {
                        sim.tile_states.insert(fid, pf.state_id);
                        let comms = pf
                            .communities
                            .iter()
                            .map(|pc| Community {
                                thousands: pc.thousands,
                                nationality_id: pc.nationality_id,
                                language_id: pc.language_id,
                                agriculture: pc.agriculture,
                                racial_mix: pc
                                    .racial_mix
                                    .iter()
                                    .map(|rc| (rc.heritage_id, rc.fraction))
                                    .collect(),
                            })
                            .collect();
                        sim.tile_communities.insert(fid, comms);
                    }
                }
                sim.trade_partners = hist
                    .trade_partners
                    .iter()
                    .map(|sp| (sp.state_a, sp.state_b))
                    .collect();
                sim.conflicts = hist
                    .conflicts
                    .iter()
                    .map(|sp| (sp.state_a, sp.state_b))
                    .collect();
            }
        }
        *self = sim;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_simulation_and_snapshot_roundtrip() {
        let mut sim = HistorySimulation::new_earth(42);
        assert!(sim.fine_face_count() > 15000);
        assert!(sim.tile_count() > 20000);
        assert!(sim.active_state_count() > 0);
        assert!(sim.total_population_thousands() > 0.0);

        sim.step();
        sim.step();
        assert_eq!(sim.history_ticks(), 2);
        assert_eq!(sim.years(), 400);

        let colors = sim.tile_colors_rgb("nations", true, None);
        assert_eq!(colors.len(), sim.tile_count() * 3);

        let bytes = sim.encode_snapshot();
        assert!(!bytes.is_empty());

        let mut restored = HistorySimulation::new_earth(99);
        assert!(restored.load_snapshot_bytes(&bytes));
        assert_eq!(restored.history_ticks(), 2);
        assert_eq!(restored.active_state_count(), sim.active_state_count());
    }
}
