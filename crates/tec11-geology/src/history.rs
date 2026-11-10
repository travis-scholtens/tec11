use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

use tec11_math_grid::{
    normal, DisplayFaceSource, FineParentage, Pcg64Mcg, PointTree, SphericalGrid,
    SubdividedTerrainGrid, Vec3,
};
use tec11_proto::PlanetSnapshotExt;

use crate::language::{Language, SoundChangeKind, Word, WordOrigin};
use crate::lifeforms::{settle_species_on_climate, SettledSpecies, SpeciesKind};
use crate::population::{
    is_farmable, is_habitable, is_passable, main_ocean, tile_population_thousands,
};
use crate::prehistory::PrehistorySimulation;
use crate::rivers::trace_coarse_rivers;

pub const HISTORY_YEARS_PER_TICK: u32 = 200;
pub const HISTORY_MIN_RIVER_ELEV: f32 = 2.0;
pub const HISTORY_MIN_RIVER_PRECIP: f32 = 0.25;
pub const MAJOR_LANGUAGE_THRESHOLD: f32 = 0.25;

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
    Sun(u32),
    Moon(u32),
    Planet(u32),
    Species(u32),
    Nation(u32),
    State(u32),
    Language(u32),
}

impl ConceptKey {
    pub fn to_proto(self) -> tec11_proto::Concept {
        match self {
            Self::Sun(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Sun as i32,
                id,
            },
            Self::Moon(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Moon as i32,
                id,
            },
            Self::Planet(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Planet as i32,
                id,
            },
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
            tec11_proto::ConceptKind::Sun => Self::Sun(c.id),
            tec11_proto::ConceptKind::Moon => Self::Moon(c.id),
            tec11_proto::ConceptKind::Planet => Self::Planet(c.id),
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
    pub used_words: BTreeSet<Word>,
    pub used_wordkeys: BTreeSet<Vec<(bool, i32)>>,
    pub changes_per_tick: Vec<Vec<u32>>,
}

impl LanguageRecord {
    pub fn new(id: u32, rng: &mut Pcg64Mcg) -> Self {
        let phonology = Language::generate(rng);
        let mut rec = Self {
            id,
            phonology,
            lexicon: BTreeMap::new(),
            used_words: BTreeSet::new(),
            used_wordkeys: BTreeSet::new(),
            changes_per_tick: Vec::new(),
        };
        rec.coin_concept(ConceptKey::Language(id), rng);
        rec.refresh_phonology();
        rec
    }

    /// Initialize a language for state `state_id` (`historysimulation.py:1029-1039`, `langfromspecies`):
    /// coins native species, celestial concepts (`Sun`, `Moons`, and `Planets`), and `('nation', state_id)`,
    /// then derives `('state', state_id)` and `('language', id)` from `('nation', state_id)`.
    pub fn new_for_state(
        id: u32,
        state_id: u32,
        species_indices: &[usize],
        rng: &mut Pcg64Mcg,
    ) -> Self {
        Self::new_for_state_with_celestial(id, state_id, species_indices, 1, rng)
    }

    pub fn new_for_state_with_celestial(
        id: u32,
        state_id: u32,
        species_indices: &[usize],
        moon_count: u32,
        rng: &mut Pcg64Mcg,
    ) -> Self {
        let phonology = Language::generate(rng);
        let mut rec = Self {
            id,
            phonology,
            lexicon: BTreeMap::new(),
            used_words: BTreeSet::new(),
            used_wordkeys: BTreeSet::new(),
            changes_per_tick: Vec::new(),
        };
        rec.coin_concept(ConceptKey::Nation(state_id), rng);
        rec.derive_concept(
            ConceptKey::Nation(state_id),
            ConceptKey::Language(id),
            rng,
        );
        rec.derive_concept(
            ConceptKey::Nation(state_id),
            ConceptKey::State(state_id),
            rng,
        );
        // Refresh self-origins with the newly derived language name
        let lang_name = rec.name();
        for origin in rec.lexicon.values_mut() {
            origin.lang_name = lang_name.clone();
        }
        // Celestial concepts: Sun, Moons (0..moon_count), and Planets (home world + classical naked-eye planets)
        rec.coin_concept(ConceptKey::Sun(0), rng);
        for m in 0..moon_count {
            if m == 0 || ((id + m) % 2 == 0) {
                rec.coin_concept(ConceptKey::Moon(m), rng);
            } else {
                rec.derive_concept(ConceptKey::Moon(0), ConceptKey::Moon(m), rng);
            }
        }
        // Planet(0) = Home planet / world; Planet(1..=5) = classical naked-eye planets
        rec.coin_concept(ConceptKey::Planet(0), rng);
        rec.coin_concept(ConceptKey::Planet(1), rng);
        rec.coin_concept(ConceptKey::Planet(2), rng);
        if state_id % 2 == 0 {
            rec.coin_concept(ConceptKey::Planet(3), rng);
        }
        if state_id % 3 == 0 {
            rec.coin_concept(ConceptKey::Planet(4), rng);
        }
        if state_id % 4 == 0 {
            rec.coin_concept(ConceptKey::Planet(5), rng);
        }
        for &sp_idx in species_indices {
            rec.coin_concept(ConceptKey::Species(sp_idx as u32), rng);
        }
        rec.refresh_phonology();
        rec
    }

    pub fn name(&self) -> String {
        self.lexicon
            .get(&ConceptKey::Language(self.id))
            .map(|o| o.word.write(true))
            .unwrap_or_else(|| format!("Lang_{}", self.id + 1))
    }

    pub fn describes(&self, key: ConceptKey) -> bool {
        self.lexicon.contains_key(&key)
    }

    /// Recompute emergent phoneme inventory, syllable onset/coda probabilities, and attested
    /// onset/coda cluster constraints from the current living lexicon (`language/stats.py:3-33`).
    pub fn refresh_phonology(&mut self) {
        if self.lexicon.is_empty() {
            return;
        }
        let words: Vec<Word> = self.lexicon.values().map(|o| o.word.clone()).collect();
        self.phonology.refresh_from_lexicon(&words);
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
        let word = self
            .phonology
            .generate_word_unique(&mut self.used_wordkeys, &self.used_words, rng);
        self.used_words.insert(word.clone());
        let origin = WordOrigin::new(word, lang_label, None);
        self.lexicon.insert(key, origin.clone());
        origin
    }

    /// Derive `dest_key` from `src_key` by adapting sounds, constraining clusters, and appending a
    /// 1-syllable suffix (`language/lazy.py:11-21, 90-100`).
    pub fn derive_concept(
        &mut self,
        src_key: ConceptKey,
        dest_key: ConceptKey,
        rng: &mut Pcg64Mcg,
    ) -> Option<WordOrigin> {
        if let Some(existing) = self.lexicon.get(&dest_key) {
            return Some(existing.clone());
        }
        let src_origin = self.lexicon.get(&src_key)?.clone();
        let derived_word = self
            .phonology
            .derive_word_unique(&src_origin.word, &self.used_words, rng);
        self.used_wordkeys.insert(derived_word.wordkey());
        self.used_words.insert(derived_word.clone());
        let my_name = self.name();
        let origin = WordOrigin::new(derived_word, my_name, Some(src_origin));
        self.lexicon.insert(dest_key, origin.clone());
        Some(origin)
    }

    pub fn borrow_concept(&mut self, key: ConceptKey, donor: &LanguageRecord) {
        if self.lexicon.contains_key(&key) {
            return;
        }
        if let Some(donor_origin) = donor.lexicon.get(&key) {
            let mut local_rng = Pcg64Mcg::new(
                ((self.id as u128) << 64)
                    ^ ((donor.id as u128) << 32)
                    ^ (self.lexicon.len() as u128),
            );
            let borrowed_word = self.phonology.borrow_word_unique(
                &donor_origin.word,
                &self.used_words,
                &mut local_rng,
            );
            self.used_wordkeys.insert(borrowed_word.wordkey());
            self.used_words.insert(borrowed_word.clone());
            let my_name = self.name();
            let origin = WordOrigin::new(borrowed_word, my_name, Some(donor_origin.clone()));
            self.lexicon.insert(key, origin);
        }
    }

    /// Clone a dialect and redefine `Language(self.id)` to `Language(new_id)`
    /// (`language/lazy.py:137-146, 190-194`).
    pub fn clone_dialect(&self, new_id: u32) -> Self {
        let mut cloned = self.clone();
        let old_id = cloned.id;
        cloned.id = new_id;
        if let Some(orig) = cloned.lexicon.remove(&ConceptKey::Language(old_id)) {
            cloned.lexicon.insert(ConceptKey::Language(new_id), orig);
        }
        cloned
    }

    /// Apply 2 to 4 cascaded language-wide sound changes for a 200-year tick, preventing homophonous
    /// mergers (`language/lazy.py:47`, `languagesimulation.py:25`) and refreshing the language's
    /// emergent phoneme inventory and cluster constraints (`language/stats.py`).
    pub fn mutate_tick(&mut self, rng: &mut Pcg64Mcg) {
        let num_changes = 2 + ((rng.next_u64() as usize) % 3); // 2, 3, or 4 sound changes per 200 years
        let mut tick_changes = Vec::with_capacity(num_changes);

        for _ in 0..num_changes {
            let change_id = (rng.next_u64() as u32) % 9;
            let change = SoundChangeKind::from_u32(change_id);
            tick_changes.push(change_id);
            let my_name = self.name();

            for origin in self.lexicon.values_mut() {
                let candidate = change.apply(&origin.word);
                if candidate != origin.word && !self.used_words.contains(&candidate) {
                    self.used_words.remove(&origin.word);
                    self.used_wordkeys.insert(candidate.wordkey());
                    self.used_words.insert(candidate);
                    let mut next = origin.mutate(change);
                    next.lang_name = my_name.clone();
                    *origin = next;
                }
            }
        }
        self.changes_per_tick.push(tick_changes);
        self.refresh_phonology();
    }
}

#[derive(Clone, Debug)]
pub struct StateRecord {
    pub id: u32,
    pub capital_tile: usize,
    pub color_idx: usize,
    pub active: bool,
    /// Turchin's collective solidarity / *asabiya* ($S_n \in [0.1, 1.0]$; *Historical Dynamics*, 2003).
    pub asabiya: f32,
}

/// Complete History simulation (`historysimulation.py`, `historydisplay.py`, `historypresenter.py`).
pub struct HistorySimulation {
    prehistory: PrehistorySimulation,
    terrain: SubdividedTerrainGrid,
    tree: PointTree,
    history_ticks: u32,
    moon_count: u32,
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

    pub fn from_prehistory(prehistory: PrehistorySimulation, seed: u128) -> Self {
        Self::from_prehistory_with_moons(prehistory, 1, seed)
    }

    pub fn from_prehistory_with_moons(
        mut prehistory: PrehistorySimulation,
        moon_count: u32,
        seed: u128,
    ) -> Self {
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
            moon_count,
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

        // Create initial states, nationalities, and languages at each city (`langfromspecies`)
        let mut q = VecDeque::new();
        for &city_f in &city_tiles {
            let sid = self.next_state_id;
            self.next_state_id += 1;
            let lid = self.next_language_id;
            self.next_language_id += 1;

            let lang = LanguageRecord::new_for_state_with_celestial(
                lid,
                sid,
                &self.tile_species[city_f],
                self.moon_count,
                &mut self.rng,
            );
            self.languages.insert(lid, lang);
            self.nationalities.insert(sid);
            self.states.insert(
                sid,
                StateRecord {
                    id: sid,
                    capital_tile: city_f,
                    color_idx: (sid as usize) % NATION_PALETTE.len(),
                    active: true,
                    asabiya: 0.5,
                },
            );
            self.tile_states.insert(city_f, sid);
            q.push_back((city_f, sid, 0usize));
        }

        // Expand state boundaries over passable fine faces with Boulding river step costs (`nearestcity`)
        while let Some((curr, sid, dist)) = q.pop_front() {
            if dist >= 16 {
                continue;
            }
            for &nb in &fine_adj[curr] {
                let c = self.terrain.primary_coarse_tile(nb, &coarse_elevs);
                let step_cost = if self.river_tiles[nb] { 10 } else { 1 };
                if dist + step_cost <= 16
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

    /// Direct trade partners of state `n` (`historysimulation.py:lookuptradepartners`).
    fn lookup_trade_partners(&self, n: u32) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for &(a, b) in &self.trade_partners {
            if a == n {
                out.insert(b);
            } else if b == n {
                out.insert(a);
            }
        }
        out
    }

    /// Transitive trade network of state `n` with shortest hop counts `h >= 1` (`recursivetradepartners`).
    fn recursive_trade_partner_distances(&self, n: u32) -> BTreeMap<u32, usize> {
        let mut dists = BTreeMap::new();
        let mut q = VecDeque::new();
        dists.insert(n, 0usize);
        q.push_back((n, 0usize));
        while let Some((curr, d)) = q.pop_front() {
            for nb in self.lookup_trade_partners(curr) {
                if !dists.contains_key(&nb) {
                    dists.insert(nb, d + 1);
                    q.push_back((nb, d + 1));
                }
            }
        }
        dists.remove(&n);
        dists
    }

    /// Transitive trade network of state `n` (`historysimulation.py:recursivetradepartners`).
    fn recursive_trade_partners(&self, n: u32) -> BTreeSet<u32> {
        self.recursive_trade_partner_distances(n)
            .into_keys()
            .collect()
    }

    /// Spherical angular distance in radians `[0, pi]` between the capitals of states `s1` and `s2`,
    /// used in Boulding's Loss-of-Strength Gradient and Tinbergen's Gravity Model.
    fn capital_angular_distance(&self, s1: u32, s2: u32) -> f32 {
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
        dot.acos() as f32
    }

    /// Total carrying capacity $K_n$ of state `state_id` across its fine faces.
    fn state_carrying_capacity(&self, tiles: &[usize]) -> f32 {
        tiles.iter().map(|&t| self.capacities[t]).sum::<f32>().max(0.1)
    }

    /// Collective solidarity (*asabiya*) $S_n \in [0.1, 1.0]$ of state `sid` (Turchin 2003).
    pub fn state_asabiya(&self, sid: u32) -> f32 {
        self.states
            .get(&sid)
            .map(|s| s.asabiya)
            .unwrap_or(0.5)
            .clamp(0.1, 1.0)
    }

    /// Metaethnic faultline intensity $I_f \in [0.0, 1.0]$ on fine face `f_idx` across political borders
    /// (Turchin, *Historical Dynamics*, 2003, Ch. 4):
    /// `+0.50` across the agricultural/pastoral divide, `+0.30` across nationality boundaries,
    /// and `+0.20` across language boundaries.
    fn tile_metaethnic_intensity(&self, f_idx: usize, fine_adj: &[Vec<usize>]) -> f32 {
        let Some(&sid) = self.tile_states.get(&f_idx) else {
            return 0.0;
        };
        let f_ag = self
            .tile_communities
            .get(&f_idx)
            .is_some_and(|cs| cs.iter().any(|c| c.agriculture));
        let f_nat = self.majority_nationality(f_idx);
        let f_lang = self.majority_language(f_idx);

        let mut max_intensity = 0.0f32;
        for &nb in &fine_adj[f_idx] {
            if self.tile_states.get(&nb) == Some(&sid) {
                continue;
            }
            let nb_pop: f32 = self
                .tile_communities
                .get(&nb)
                .map(|cs| cs.iter().map(|c| c.thousands).sum())
                .unwrap_or(0.0);
            if nb_pop <= 0.0 {
                continue;
            }
            let nb_ag = self
                .tile_communities
                .get(&nb)
                .is_some_and(|cs| cs.iter().any(|c| c.agriculture));
            let nb_nat = self.majority_nationality(nb);
            let nb_lang = self.majority_language(nb);

            let mut intensity = 0.0f32;
            if f_ag != nb_ag {
                intensity += 0.50;
            }
            if f_nat != nb_nat {
                intensity += 0.30;
            }
            if f_lang != nb_lang {
                intensity += 0.20;
            }
            if intensity > max_intensity {
                max_intensity = intensity;
            }
        }
        max_intensity.clamp(0.0, 1.0)
    }

    /// Update each active state's collective solidarity (*asabiya*) $S_n$ using Turchin's logistic
    /// metaethnic frontier equation (*Historical Dynamics*, 2003, eq. 4.3-4.6):
    /// $S_n(t+1) = S_n(t) + r_{\text{frontier}} \bar{I}_n (1 - S_n(t)) - \delta_{\text{core}} (1 - \bar{I}_n) S_n(t)$.
    fn update_state_asabiya(
        &mut self,
        extents: &BTreeMap<u32, Vec<usize>>,
        fine_adj: &[Vec<usize>],
    ) {
        const R_FRONTIER: f32 = 0.45;
        const DELTA_CORE: f32 = 0.12;
        let mut new_asabiya: BTreeMap<u32, f32> = BTreeMap::new();
        for (&sid, tiles) in extents {
            if tiles.is_empty() {
                continue;
            }
            let sum_i: f32 = tiles
                .iter()
                .map(|&f| self.tile_metaethnic_intensity(f, fine_adj))
                .sum();
            // Scale frontier exposure so compact frontier marcher states reach high I_bar while sprawling empires dilute I_bar
            let mean_i = (sum_i / (tiles.len() as f32) * 2.5).clamp(0.0, 1.0);
            let s = self.state_asabiya(sid);
            let ds = R_FRONTIER * mean_i * (1.0 - s) - DELTA_CORE * (1.0 - mean_i) * s;
            new_asabiya.insert(sid, (s + ds).clamp(0.10, 1.00));
        }
        for (sid, s) in new_asabiya {
            if let Some(st) = self.states.get_mut(&sid) {
                st.asabiya = s;
            }
        }
    }

    /// Active military opponents of state `n` (`historysimulation.py:lookupopponents`).
    fn lookup_opponents(&self, n: u32) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for &(a, b) in &self.conflicts {
            if a == n {
                out.insert(b);
            } else if b == n {
                out.insert(a);
            }
        }
        out
    }

    /// Compute per-state tile extents, native species sets, and neighboring states (including across rivers: `riverneighbors`).
    fn state_topology(
        &self,
        adjacency: &[Vec<usize>],
    ) -> (
        BTreeMap<u32, Vec<usize>>,
        BTreeMap<u32, BTreeSet<usize>>,
        BTreeMap<u32, BTreeSet<u32>>,
    ) {
        let mut extents: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut state_species: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
        let mut neighbors: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();

        for (&t, &s1) in &self.tile_states {
            extents.entry(s1).or_default().push(t);
            let sp_set = state_species.entry(s1).or_default();
            for &sp in &self.tile_species[t] {
                sp_set.insert(sp);
            }
            let nb_set = neighbors.entry(s1).or_default();
            let mut checked_river = false;
            for &nb in &adjacency[t] {
                if let Some(&s2) = self.tile_states.get(&nb) {
                    if s1 != s2 {
                        nb_set.insert(s2);
                    }
                }
                if !checked_river && self.river_tiles[nb] {
                    checked_river = true;
                    for &r_nb in &adjacency[nb] {
                        if let Some(&s2) = self.tile_states.get(&r_nb) {
                            if s1 != s2 {
                                nb_set.insert(s2);
                            }
                        }
                    }
                }
            }
        }
        (extents, state_species, neighbors)
    }

    /// Hop-decayed Heckscher-Ohlin & Tinbergen trade pressure $\text{Pressure}_{n \leftarrow o}$:
    /// each novel species $s \notin S_n$ available in $o$ (at hop $h = 0$) or via $o$'s recursive trade
    /// network at minimum hop $h \ge 1$ contributes $0.5^h$, damped by capital distance $e^{-0.25 d_{no}}$.
    fn compute_trade_pressure(
        &self,
        n: u32,
        o: u32,
        state_species: &BTreeMap<u32, BTreeSet<usize>>,
    ) -> f32 {
        let Some(native) = state_species.get(&n) else {
            return 0.0;
        };
        let mut min_hop_for_species: BTreeMap<usize, usize> = BTreeMap::new();
        if let Some(osp) = state_species.get(&o) {
            for &sp in osp.difference(native) {
                min_hop_for_species.insert(sp, 0);
            }
        }
        for (partner, hop) in self.recursive_trade_partner_distances(o) {
            if partner == n {
                continue;
            }
            if let Some(psp) = state_species.get(&partner) {
                for &sp in psp.difference(native) {
                    let entry = min_hop_for_species.entry(sp).or_insert(hop);
                    if hop < *entry {
                        *entry = hop;
                    }
                }
            }
        }
        let raw_gain: f32 = min_hop_for_species
            .values()
            .map(|&h| 0.5f32.powi(h as i32))
            .sum();
        let dist = self.capital_angular_distance(n, o);
        raw_gain * (-0.25 * dist).exp()
    }

    /// Update trade partnerships and military conflicts using:
    /// 1. Heckscher-Ohlin comparative advantage + Tinbergen hop/distance gravity decay (`compute_trade_pressure`),
    /// 2. Polachek commercial peace (active war blocks trade; trade raises war threshold by peace dividend $\tau_{\text{trade}}$),
    /// 3. Collins resource + territorial carrying-capacity + demographic tension with Boulding distance decay (`e^{-0.5 d}`).
    fn update_diplomacy_and_trade(&mut self, adjacency: &[Vec<usize>]) {
        let (extents, state_species, neighbors) = self.state_topology(adjacency);
        let active_states: BTreeSet<u32> = extents.keys().copied().collect();
        for st in self.states.values_mut() {
            st.active = active_states.contains(&st.id);
        }

        // Prune any stale trade partnerships or conflicts involving extinct or non-neighboring states
        self.trade_partners.retain(|&(a, b)| {
            active_states.contains(&a)
                && active_states.contains(&b)
                && a != b
                && neighbors.get(&a).is_some_and(|nbs| nbs.contains(&b))
        });
        self.conflicts.retain(|&(a, b)| {
            active_states.contains(&a)
                && active_states.contains(&b)
                && a != b
                && neighbors.get(&a).is_some_and(|nbs| nbs.contains(&b))
        });

        let active_conflict_set: BTreeSet<(u32, u32)> = self.conflicts.iter().copied().collect();

        // 1. Heckscher-Ohlin + Tinbergen trade pressure & mutual eligibility
        let mut pressure_map: BTreeMap<(u32, u32), f32> = BTreeMap::new();
        let mut eligible: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &n in &active_states {
            let native_len = state_species.get(&n).map(|s| s.len()).unwrap_or(0) as f32;
            let min_gain = (native_len * 0.05).max(0.5);
            let mut n_elig = BTreeSet::new();
            if let Some(nbs) = neighbors.get(&n) {
                for &o in nbs {
                    let pair = if n < o { (n, o) } else { (o, n) };
                    // Polachek: states actively at war cannot simultaneously form a trade partnership
                    if active_conflict_set.contains(&pair) {
                        continue;
                    }
                    let p = self.compute_trade_pressure(n, o, &state_species);
                    pressure_map.insert((n, o), p);
                    if p >= min_gain {
                        n_elig.insert(o);
                    }
                }
            }
            eligible.insert(n, n_elig);
        }

        // Re-evaluate existing trade partners (dissolve if mutual gains drop below half the threshold)
        let mut partner_set: BTreeSet<(u32, u32)> = self
            .trade_partners
            .iter()
            .copied()
            .filter(|&(a, b)| {
                let p_ab = pressure_map.get(&(a, b)).copied().unwrap_or(0.0);
                let p_ba = pressure_map.get(&(b, a)).copied().unwrap_or(0.0);
                let min_a = (state_species.get(&a).map(|s| s.len()).unwrap_or(0) as f32) * 0.025;
                let min_b = (state_species.get(&b).map(|s| s.len()).unwrap_or(0) as f32) * 0.025;
                p_ab >= min_a && p_ba >= min_b
            })
            .collect();

        for (&n, n_elig) in &eligible {
            for &o in n_elig {
                if eligible.get(&o).is_some_and(|o_elig| o_elig.contains(&n)) {
                    let pair = if n < o { (n, o) } else { (o, n) };
                    partner_set.insert(pair);
                }
            }
        }

        // 2. Collins military tension + Turchin asabiya mobilization + Boulding Loss-of-Strength Gradient + Polachek commercial peace
        let state_eff_pops: BTreeMap<u32, f32> = active_states
            .iter()
            .map(|&sid| {
                let pop = self.state_population_thousands(sid);
                let asabiya = self.state_asabiya(sid);
                (sid, pop * (0.5 + asabiya))
            })
            .collect();
        let state_caps: BTreeMap<u32, f32> = extents
            .iter()
            .map(|(&sid, tiles)| (sid, self.state_carrying_capacity(tiles)))
            .collect();

        let mut conflict_set: BTreeSet<(u32, u32)> = active_conflict_set;

        for &n in &active_states {
            let native = state_species.get(&n).cloned().unwrap_or_default();
            let eff_n = state_eff_pops.get(&n).copied().unwrap_or(0.0);
            let cap_n = state_caps.get(&n).copied().unwrap_or(1.0);
            let mut best_rival: Option<(u32, f32)> = None;

            if let Some(nbs) = neighbors.get(&n) {
                for &o in nbs {
                    let eff_o = state_eff_pops.get(&o).copied().unwrap_or(0.0);
                    if eff_o <= 1e-4 || eff_n <= eff_o {
                        continue;
                    }
                    let cap_o = state_caps.get(&o).copied().unwrap_or(1.0);
                    let osp = state_species.get(&o).cloned().unwrap_or_default();

                    // Heckscher-Ohlin resource motive + Collins territorial carrying-capacity motive
                    let dr = if native.is_empty() {
                        0.0
                    } else {
                        (osp.difference(&native).count() as f32) / (native.len() as f32)
                    };
                    let dk = cap_o / (cap_n + cap_o);
                    let dp = (eff_n - eff_o) / eff_o;
                    let dist = self.capital_angular_distance(n, o);
                    let boulding_lsg = (-0.5 * dist).exp();
                    let tension = (dr + 0.15 * dk + dp) * boulding_lsg;

                    let pair = if n < o { (n, o) } else { (o, n) };
                    // Polachek commercial peace dividend raises the threshold required to break peace with a trade partner
                    let war_threshold = if partner_set.contains(&pair) {
                        let p_no = pressure_map.get(&(n, o)).copied().unwrap_or(0.0);
                        let p_on = pressure_map.get(&(o, n)).copied().unwrap_or(0.0);
                        let denom =
                            (native.len() + osp.len()).max(1) as f32;
                        let peace_dividend = (1.5 * (p_no + p_on) / denom).max(1.0);
                        1.0 + peace_dividend
                    } else {
                        1.0
                    };

                    if tension >= war_threshold
                        && best_rival.is_none_or(|(_, best_t)| tension > best_t)
                    {
                        best_rival = Some((o, tension));
                    }
                }
            }
            if let Some((priority, _)) = best_rival {
                let pair = if n < priority {
                    (n, priority)
                } else {
                    (priority, n)
                };
                // If tension overcame Polachek commercial peace, abrogate the trade pact
                partner_set.remove(&pair);
                conflict_set.insert(pair);
            }
        }
        self.trade_partners = partner_set.into_iter().collect();
        self.conflicts = conflict_set.into_iter().collect();

        // 3. Accumulate concepts and borrow missing words (`historysimulation.py:664-705`)
        self.accumulate_and_borrow_concepts(&extents, &state_species);
    }

    /// Accumulate concepts across co-resident communities, trade partners, rivals, and recursive trade imports,
    /// borrowing missing words from source state majority languages (`historysimulation.py:664-705`).
    fn accumulate_and_borrow_concepts(
        &mut self,
        extents: &BTreeMap<u32, Vec<usize>>,
        state_species: &BTreeMap<u32, BTreeSet<usize>>,
    ) {
        let mut state_langs: BTreeMap<u32, Option<u32>> = BTreeMap::new();
        for &sid in extents.keys() {
            state_langs.insert(sid, self.primary_language_for_state(sid));
        }

        let mut lang_concepts: BTreeMap<u32, BTreeMap<ConceptKey, Option<u32>>> = BTreeMap::new();
        let mut state_sources: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();

        for (&f, comms) in &self.tile_communities {
            let Some(&sid) = self.tile_states.get(&f) else {
                continue;
            };
            let nats_langs: Vec<(u32, u32)> = comms
                .iter()
                .filter(|c| c.thousands > 0.0)
                .map(|c| (c.nationality_id, c.language_id))
                .collect();
            for &(_, lang) in &nats_langs {
                let entry = lang_concepts.entry(lang).or_default();
                for &(n, l) in &nats_langs {
                    entry.insert(ConceptKey::Language(l), Some(l));
                    entry.insert(ConceptKey::Nation(n), Some(l));
                }
                state_sources.entry(lang).or_default().insert(sid);
            }
        }

        for (&lang_id, concepts) in lang_concepts.iter_mut() {
            let Some(states_for_lang) = state_sources.get(&lang_id) else {
                continue;
            };
            for &state_id in states_for_lang {
                let native = state_species.get(&state_id).cloned().unwrap_or_default();
                let rec_partners = self.recursive_trade_partners(state_id);
                let direct_partners = self.lookup_trade_partners(state_id);
                let rivals = self.lookup_opponents(state_id);

                let mut related_states = BTreeSet::new();
                related_states.insert(state_id);
                related_states.extend(direct_partners);
                related_states.extend(rivals);
                related_states.extend(rec_partners.iter().copied());

                for s in &related_states {
                    let s_lang = state_langs.get(s).copied().flatten();
                    concepts.insert(ConceptKey::State(*s), s_lang);
                    // Share astronomical knowledge of outer wandering planets (`Planet(3..=5)`) with trading partners & neighbors
                    if let Some(slid) = s_lang {
                        if let Some(slang) = self.languages.get(&slid) {
                            for p_id in 3..=5 {
                                let pk = ConceptKey::Planet(p_id);
                                if slang.describes(pk) {
                                    concepts.entry(pk).or_insert(Some(slid));
                                }
                            }
                        }
                    }
                }

                // Imported species from recursive trade network (`imports`)
                for partner in rec_partners {
                    let p_lang = state_langs.get(&partner).copied().flatten();
                    if let Some(psp) = state_species.get(&partner) {
                        for &sp in psp.difference(&native) {
                            concepts.insert(ConceptKey::Species(sp as u32), p_lang);
                        }
                    }
                }

                // Native species and core celestial concepts (`Sun`, `Moons`, and `Planets`) in `state_id`
                let s_lang = state_langs.get(&state_id).copied().flatten();
                concepts.entry(ConceptKey::Sun(0)).or_insert(s_lang);
                for m in 0..self.moon_count {
                    concepts.entry(ConceptKey::Moon(m)).or_insert(s_lang);
                }
                for p in 0..=2 {
                    concepts.entry(ConceptKey::Planet(p)).or_insert(s_lang);
                }
                for &sp in &native {
                    concepts.insert(ConceptKey::Species(sp as u32), s_lang);
                }
            }
        }

        let lang_ids: Vec<u32> = lang_concepts.keys().copied().collect();
        for lang_id in lang_ids {
            let Some(concepts) = lang_concepts.get(&lang_id).cloned() else {
                continue;
            };
            for (concept, src_lang_opt) in concepts {
                let already = self
                    .languages
                    .get(&lang_id)
                    .is_some_and(|l| l.describes(concept));
                if already {
                    continue;
                }
                let donor_opt = src_lang_opt
                    .and_then(|src_id| self.languages.get(&src_id).cloned())
                    .filter(|d| d.describes(concept));
                if let Some(donor) = donor_opt {
                    if let Some(lang) = self.languages.get_mut(&lang_id) {
                        lang.borrow_concept(concept, &donor);
                    }
                } else {
                    let mut temp_rng = self.rng.clone();
                    if let Some(lang) = self.languages.get_mut(&lang_id) {
                        lang.coin_concept(concept, &mut temp_rng);
                    }
                    self.rng = temp_rng;
                }
            }
        }
    }

    /// Cubic state split probability (`historysimulation.py:497-503`), scaled by 1/9 for $N+2$ fine faces per coarse tile:
    /// `x = (lentiles / 9.0) - 25`, `y = 3 + sign(x) * |x|^(1/3)`, `y / 600`.
    pub fn split_probability(len_tiles: usize) -> f64 {
        let x = (len_tiles as f64) / 9.0 - 25.0;
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let root = sign * (sign * x).cbrt();
        let y = (3.0 + root).max(0.05);
        y / 600.0
    }

    /// Turchin-modulated state split probability: high *asabiya* ($S_n \to 1$) holds large states together,
    /// while low *asabiya* ($S_n \to 0.1$) accelerates imperial fragmentation.
    pub fn split_probability_with_asabiya(len_tiles: usize, asabiya: f32) -> f64 {
        Self::split_probability(len_tiles) * ((1.5 - asabiya).clamp(0.4, 1.5) as f64)
    }

    /// City spawning probability per fine face (`historysimulation.py:846-867`), scaled by 1/9 for $N+2$ subdivision
    /// and boosted on high-intensity metaethnic frontiers $(1 + 1.5 I_f)$ per Turchin (2003).
    fn city_prob(
        &self,
        f_idx: usize,
        coarse_elevs: &[f32],
        koeppen: &[u8],
        fine_adj: &[Vec<usize>],
    ) -> f64 {
        let c = self.terrain.primary_coarse_tile(f_idx, coarse_elevs);
        let k = koeppen[c];
        if k == 11 || k == 12 {
            // ET (tundra) or EF (ice cap)
            return 0.0;
        }
        let has_ag = self
            .tile_communities
            .get(&f_idx)
            .is_some_and(|cs| cs.iter().any(|cm| cm.agriculture));
        let coarse_prob = if !has_ag {
            0.25
        } else if k == 0 || k == 1 || k == 2 {
            // BW, BS, Af
            0.15
        } else {
            0.05
        };
        let frontier_boost = 1.0 + 1.5 * (self.tile_metaethnic_intensity(f_idx, fine_adj) as f64);
        (coarse_prob / 9.0) * frontier_boost
    }

    fn majority_nationality(&self, f_idx: usize) -> Option<u32> {
        let comms = self.tile_communities.get(&f_idx)?;
        let mut counts: BTreeMap<u32, f32> = BTreeMap::new();
        for c in comms {
            if c.thousands > 0.0 {
                *counts.entry(c.nationality_id).or_insert(0.0) += c.thousands;
            }
        }
        counts
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| k)
    }

    fn majority_language(&self, f_idx: usize) -> Option<u32> {
        let comms = self.tile_communities.get(&f_idx)?;
        let mut counts: BTreeMap<u32, f32> = BTreeMap::new();
        for c in comms {
            if c.thousands > 0.0 {
                *counts.entry(c.language_id).or_insert(0.0) += c.thousands;
            }
        }
        counts
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| k)
    }

    /// Multi-source Boulding Loss-of-Strength Gradient Dijkstra partitioner (`historysimulation.py:900-936`).
    /// Assigns each tile in `tiles` to the candidate capital in `cities` with minimum cumulative
    /// administrative friction (`+1` normal step, `+2` language boundary, `+3` nationality boundary,
    /// `+10` river crossing, `+\infty` sea, unpopulated, or agricultural/pastoral divide).
    fn partition_by_boulding_lsg(
        &self,
        tiles: &[usize],
        cities: &[usize],
        fine_adj: &[Vec<usize>],
    ) -> BTreeMap<usize, usize> {
        let tile_set: BTreeSet<usize> = tiles.iter().copied().collect();
        let mut best_cost: BTreeMap<usize, (u32, usize)> = BTreeMap::new();
        let mut heap: BinaryHeap<Reverse<(u32, usize, usize)>> = BinaryHeap::new();

        for (city_idx, &city_tile) in cities.iter().enumerate() {
            best_cost.insert(city_tile, (0, city_idx));
            heap.push(Reverse((0, city_tile, city_idx)));
        }

        while let Some(Reverse((cost, curr, city_idx))) = heap.pop() {
            if let Some(&(best_c, best_idx)) = best_cost.get(&curr) {
                if (cost, city_idx) > (best_c, best_idx) {
                    continue;
                }
            }
            let curr_ag = self
                .tile_communities
                .get(&curr)
                .is_some_and(|cs| cs.iter().any(|c| c.agriculture));
            let curr_nat = self.majority_nationality(curr);
            let curr_lang = self.majority_language(curr);

            for &next in &fine_adj[curr] {
                if !tile_set.contains(&next) {
                    continue;
                }
                let next_pop: f32 = self
                    .tile_communities
                    .get(&next)
                    .map(|cs| cs.iter().map(|c| c.thousands).sum())
                    .unwrap_or(0.0);
                let next_ag = self
                    .tile_communities
                    .get(&next)
                    .is_some_and(|cs| cs.iter().any(|c| c.agriculture));
                if self.terrain.fine_elevations[next] <= 0.0 || next_pop <= 0.0 || curr_ag != next_ag {
                    continue;
                }
                let step_cost = if self.river_tiles[next] {
                    10
                } else if curr_nat != self.majority_nationality(next) {
                    3
                } else if curr_lang != self.majority_language(next) {
                    2
                } else {
                    1
                };
                let next_cost = cost.saturating_add(step_cost);
                let is_better = match best_cost.get(&next) {
                    Some(&(bc, b_idx)) => (next_cost, city_idx) < (bc, b_idx),
                    None => true,
                };
                if is_better {
                    best_cost.insert(next, (next_cost, city_idx));
                    heap.push(Reverse((next_cost, next, city_idx)));
                }
            }
        }

        best_cost
            .into_iter()
            .map(|(tile, (_, city_idx))| (tile, city_idx))
            .collect()
    }

    /// Branch dialects when a language's speakers become geographically disconnected (`historysimulation.py:247-280`).
    fn branch_disconnected_dialects(&mut self, fine_adj: &[Vec<usize>]) {
        let mut lang_locs: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
        for (&f, comms) in &self.tile_communities {
            for c in comms {
                if c.thousands > 0.0 {
                    lang_locs.entry(c.language_id).or_default().insert(f);
                }
            }
        }

        for (lang_id, locs) in lang_locs {
            if locs.len() <= 1 {
                continue;
            }
            let mut visited = BTreeSet::new();
            let mut components: Vec<Vec<usize>> = Vec::new();
            for &start in &locs {
                if !visited.insert(start) {
                    continue;
                }
                let mut comp = Vec::new();
                let mut q = VecDeque::new();
                q.push_back(start);
                while let Some(curr) = q.pop_front() {
                    comp.push(curr);
                    for &nb in &fine_adj[curr] {
                        if locs.contains(&nb) && visited.insert(nb) {
                            q.push_back(nb);
                        }
                    }
                }
                comp.sort_unstable();
                components.push(comp);
            }
            if components.len() <= 1 {
                continue;
            }
            // Largest connected component retains `lang_id`; smaller islands branch into new dialects
            let source_idx = components
                .iter()
                .enumerate()
                .max_by_key(|(_, comp)| comp.len())
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            let Some(base_lang) = self.languages.get(&lang_id).cloned() else {
                continue;
            };
            for (idx, faces) in components.into_iter().enumerate() {
                if idx == source_idx {
                    continue;
                }
                let new_lid = self.next_language_id;
                self.next_language_id += 1;
                let dialect = base_lang.clone_dialect(new_lid);
                self.languages.insert(new_lid, dialect);
                for f in faces {
                    if let Some(comms) = self.tile_communities.get_mut(&f) {
                        for c in comms.iter_mut() {
                            if c.language_id == lang_id {
                                c.language_id = new_lid;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Advance the History simulation by 1 tick (`200` years, matching `HistorySimulation.update`
    /// in `historysimulation.py:504-706`).
    pub fn step(&mut self) {
        let fine_adj = self.terrain.terrain_adj.clone();
        let fine_elevs = self.terrain.fine_elevations.clone();
        let coarse_elevs = self.prehistory.tile_elevations();
        let koeppen = self.prehistory.tile_koeppen_ids().to_vec();

        // 1. Language sound changes (`historysimulation.py:511-515`: 2-4 cascaded per-language sound changes per 200-year tick)
        let active_langs: BTreeSet<u32> = self
            .tile_communities
            .values()
            .flat_map(|cs| cs.iter())
            .filter(|c| c.thousands > 0.0)
            .map(|c| c.language_id)
            .collect();
        for lid in active_langs {
            let mut temp_rng = self.rng.clone();
            if let Some(lang) = self.languages.get_mut(&lid) {
                lang.mutate_tick(&mut temp_rng);
            }
            self.rng = temp_rng;
        }

        // 2. Verhulst logistic growth and neighborhood spillover (`historysimulation.py:168-220`)
        let tile_pops: BTreeMap<usize, f32> = self
            .tile_communities
            .iter()
            .map(|(&t, cs)| (t, cs.iter().map(|c| c.thousands).sum()))
            .collect();
        let mut spillover: Vec<(usize, u32, Community)> = Vec::new();
        for (&t, comms) in self.tile_communities.iter_mut() {
            let cap = self.capacities[t].max(0.2);
            let total: f32 = comms.iter().map(|c| c.thousands).sum();
            if total <= 1e-5 {
                continue;
            }
            // Verhulst logistic growth: P(t+1) = K / (1 + ((K - P0) / P0) * exp(-0.25))
            let exp_neg_r = (-0.25f32).exp();
            let grown = cap / (1.0 + ((cap - total) / total) * exp_neg_r);
            let factor = (grown / total).clamp(0.85, 1.25);
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

        // 3. Intermingle and assimilate (`people/community.py:16-92`), modulated by Turchin asabiya
        let tile_asabiya: BTreeMap<usize, f32> = self
            .tile_states
            .iter()
            .map(|(&t, &sid)| (t, self.state_asabiya(sid)))
            .collect();
        for (&t, comms) in self.tile_communities.iter_mut() {
            if comms.len() > 1 {
                comms.sort_by(|a, b| {
                    b.thousands
                        .partial_cmp(&a.thousands)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let total_pop: f32 = comms.iter().map(|c| c.thousands).sum();
                let ag_pop: f32 = comms
                    .iter()
                    .filter(|c| c.agriculture)
                    .map(|c| c.thousands)
                    .sum();
                // Cultural assimilation (`assimilateculture`: >= 75% agricultural converts communities > 5%)
                if total_pop > 0.0 && ag_pop / total_pop >= 0.75 {
                    for c in comms.iter_mut() {
                        if c.thousands / total_pop > 0.05 {
                            c.agriculture = true;
                        }
                    }
                }

                let asabiya = tile_asabiya.get(&t).copied().unwrap_or(0.5);
                let assim_rate = (0.02 * (0.5 + asabiya)).clamp(0.01, 0.05);
                let dom_nat = comms[0].nationality_id;
                let dom_lang = comms[0].language_id;
                let mut merged = comms.remove(0);
                let mut kept = Vec::new();
                for mut c in comms.drain(..) {
                    if (c.nationality_id == dom_nat && c.language_id == dom_lang)
                        || (c.thousands < 1.0 && total_pop > 0.0 && c.thousands / total_pop < 0.01)
                    {
                        merged.merge_from(&c);
                    } else {
                        let shift = c.thousands * assim_rate;
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

        // 4. Branch dialects for geographically disconnected language communities (`historysimulation.py:247-280`)
        self.branch_disconnected_dialects(&fine_adj);

        // 5. Update Turchin metaethnic frontier asabiya & resolve conflicts via Collins-Turchin multi-front force allocation & Boulding distance decay
        let (extents, _, _) = self.state_topology(&fine_adj);
        self.update_state_asabiya(&extents, &fine_adj);
        let state_eff_pops: BTreeMap<u32, f32> = extents
            .keys()
            .map(|&sid| {
                let pop = self.state_population_thousands(sid);
                let asabiya = self.state_asabiya(sid);
                (sid, pop * (0.5 + asabiya))
            })
            .collect();
        let opp_pop_totals: BTreeMap<u32, f32> = extents
            .keys()
            .map(|&sid| {
                let opps = self.lookup_opponents(sid);
                let total: f32 = opps
                    .iter()
                    .map(|k| state_eff_pops.get(k).copied().unwrap_or(0.0).max(0.1))
                    .sum();
                (sid, total)
            })
            .collect();

        let mut results: Vec<(u32, u32)> = Vec::new();
        let mut border_transfers: Vec<(u32, u32)> = Vec::new();
        for &(n, o) in &self.conflicts {
            let pn = state_eff_pops.get(&n).copied().unwrap_or(0.0);
            let po = state_eff_pops.get(&o).copied().unwrap_or(0.0);
            let opp_total_n = opp_pop_totals.get(&n).copied().unwrap_or(1.0).max(0.1);
            let opp_total_o = opp_pop_totals.get(&o).copied().unwrap_or(1.0).max(0.1);
            let weight_n_to_o = po.max(0.1) / opp_total_n;
            let weight_o_to_n = pn.max(0.1) / opp_total_o;
            let dist_no = self.capital_angular_distance(n, o);
            let lsg = (-0.5 * dist_no).exp();
            let fn_val = pn * weight_n_to_o * lsg;
            let fo_val = po * weight_o_to_n * lsg;

            if fo_val > 0.0 && fn_val / fo_val > 1.5 {
                results.push((n, o));
            } else if fn_val > 0.0 && fo_val / fn_val > 1.5 {
                results.push((o, n));
            } else if fo_val > 0.0 && fn_val / fo_val > 1.15 {
                border_transfers.push((n, o));
            } else if fn_val > 0.0 && fo_val / fn_val > 1.15 {
                border_transfers.push((o, n));
            }
        }
        // All active conflicts resolve this tick (decisive annexation, marginal border transfer, or stalemate truce)
        self.conflicts.clear();

        let losers: BTreeSet<u32> = results.iter().map(|&(_, l)| l).collect();
        let winners: BTreeSet<u32> = results.iter().map(|&(w, _)| w).collect();
        let mut conquests: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &(winner, loser) in &results {
            if !losers.contains(&winner) {
                conquests.entry(winner).or_default().insert(loser);
                if let Some(loser_tiles) = extents.get(&loser) {
                    for &t in loser_tiles {
                        self.tile_states.insert(t, winner);
                    }
                }
            }
        }
        for &(winner, loser) in &border_transfers {
            if losers.contains(&winner) || losers.contains(&loser) {
                continue;
            }
            if let Some(loser_tiles) = extents.get(&loser) {
                if loser_tiles.len() > 2 {
                    let mut border_faces: Vec<usize> = loser_tiles
                        .iter()
                        .copied()
                        .filter(|&t| {
                            fine_adj[t]
                                .iter()
                                .any(|&nb| self.tile_states.get(&nb) == Some(&winner))
                        })
                        .collect();
                    border_faces.truncate(6.min(loser_tiles.len().saturating_sub(1)));
                    for t in border_faces {
                        self.tile_states.insert(t, winner);
                    }
                }
            }
        }

        // 6. Adapt monoglot conquests (`historysimulation.py:552-616`)
        for (&winner, conquered_set) in &conquests {
            let Some(winner_tiles) = extents.get(&winner) else {
                continue;
            };
            let mut winner_lang_pops: BTreeMap<u32, f32> = BTreeMap::new();
            for &t in winner_tiles {
                if let Some(comms) = self.tile_communities.get(&t) {
                    for c in comms {
                        if c.thousands > 0.0 {
                            *winner_lang_pops.entry(c.language_id).or_insert(0.0) += c.thousands;
                        }
                    }
                }
            }
            let mut sorted_winner_langs: Vec<(u32, f32)> = winner_lang_pops.into_iter().collect();
            sorted_winner_langs
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let winner_pop: f32 = sorted_winner_langs.iter().map(|(_, p)| *p).sum();
            if sorted_winner_langs.is_empty() || winner_pop <= 0.0 {
                continue;
            }
            let (mut winner_lang, winner_lang_pop) = sorted_winner_langs[0];
            let winner_lang_fraction = winner_lang_pop / winner_pop;
            if winner_lang_fraction <= MAJOR_LANGUAGE_THRESHOLD
                || (sorted_winner_langs.len() > 1
                    && sorted_winner_langs[1].1 / winner_pop > MAJOR_LANGUAGE_THRESHOLD)
            {
                // Winning state is polyglot; nothing to do
                continue;
            }

            let winner_lang_set: BTreeSet<u32> =
                sorted_winner_langs.iter().map(|&(l, _)| l).collect();
            let mut loser_lang_pops: BTreeMap<u32, f32> = BTreeMap::new();
            for &loser in conquered_set {
                if let Some(loser_tiles) = extents.get(&loser) {
                    for &t in loser_tiles {
                        if let Some(comms) = self.tile_communities.get(&t) {
                            for c in comms {
                                if c.thousands > 0.0 && !winner_lang_set.contains(&c.language_id) {
                                    *loser_lang_pops.entry(c.language_id).or_insert(0.0) +=
                                        c.thousands;
                                }
                            }
                        }
                    }
                }
            }

            let mut sorted_loser_langs: Vec<(u32, f32)> = loser_lang_pops.into_iter().collect();
            sorted_loser_langs
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut old_lang: Option<u32> = None;
            let mut created_dialect = false;
            let mut to_convert = BTreeSet::new();

            for (lang, pop) in sorted_loser_langs {
                if lang == winner_lang {
                    continue;
                }
                if pop > winner_lang_pop {
                    // Conquered language outnumbers conquerors: form a new dialect cloned from winner_lang
                    if !created_dialect {
                        if let Some(base_lang) = self.languages.get(&winner_lang).cloned() {
                            let new_lid = self.next_language_id;
                            self.next_language_id += 1;
                            let dialect = base_lang.clone_dialect(new_lid);
                            self.languages.insert(new_lid, dialect);
                            old_lang = Some(winner_lang);
                            winner_lang = new_lid;
                            created_dialect = true;
                        }
                    }
                    if let Some(donor) = self.languages.get(&lang).cloned() {
                        if let Some(dialect) = self.languages.get_mut(&winner_lang) {
                            let keys: Vec<ConceptKey> = donor.lexicon.keys().copied().collect();
                            for k in keys {
                                dialect.borrow_concept(k, &donor);
                            }
                        }
                    }
                } else {
                    to_convert.insert(lang);
                }
            }

            let mut affected_states: Vec<u32> = conquered_set.iter().copied().collect();
            affected_states.push(winner);
            for st_id in affected_states {
                let Some(st_tiles) = extents.get(&st_id) else {
                    continue;
                };
                for &t in st_tiles {
                    let Some(comms) = self.tile_communities.get_mut(&t) else {
                        continue;
                    };
                    let mut new_comms = Vec::new();
                    for c in comms.iter_mut() {
                        if c.thousands > 0.0
                            && (to_convert.contains(&c.language_id)
                                || (st_id == winner && old_lang == Some(c.language_id)))
                        {
                            if (winner_lang_fraction - 1.0).abs() < 1e-4 {
                                c.language_id = winner_lang;
                            } else {
                                let converted = c.thousands * winner_lang_fraction;
                                c.thousands -= converted;
                                let mut split_c = c.clone();
                                split_c.thousands = converted;
                                split_c.language_id = winner_lang;
                                new_comms.push(split_c);
                            }
                        }
                    }
                    comms.extend(new_comms);
                }
            }
        }

        // 7. Break up overextended peaceful states (`breaking up states`, `historysimulation.py:617-662`)
        let state_ids: Vec<u32> = extents.keys().copied().collect();
        for n in state_ids {
            if winners.contains(&n) || losers.contains(&n) {
                continue;
            }
            let Some(tiles) = extents.get(&n) else {
                continue;
            };
            if tiles.len() < 18 {
                continue;
            }
            let state_asabiya = self.state_asabiya(n);
            if self.rng.uniform(0.0, 1.0)
                >= Self::split_probability_with_asabiya(tiles.len(), state_asabiya)
            {
                continue;
            }
            let mut cities: Vec<usize> = Vec::new();
            for &f in tiles {
                let p = self.city_prob(f, &coarse_elevs, &koeppen, &fine_adj);
                if self.rng.uniform(0.0, 1.0) < p {
                    cities.push(f);
                }
            }
            if cities.len() < 2 {
                continue;
            }
            let mut partition = self.partition_by_boulding_lsg(tiles, &cities, &fine_adj);
            for &f in tiles {
                if !partition.contains_key(&f) {
                    cities.push(f);
                    partition = self.partition_by_boulding_lsg(tiles, &cities, &fine_adj);
                }
            }
            let mut city_tiles_assigned: Vec<Vec<usize>> = vec![Vec::new(); cities.len()];
            let mut state_lang_pops: Vec<BTreeMap<u32, f32>> = vec![BTreeMap::new(); cities.len()];
            for &f in tiles {
                let i = partition.get(&f).copied().unwrap_or(0);
                city_tiles_assigned[i].push(f);
                if let Some(comms) = self.tile_communities.get(&f) {
                    for c in comms {
                        if c.thousands > 0.0 {
                            *state_lang_pops[i].entry(c.language_id).or_insert(0.0) += c.thousands;
                        }
                    }
                }
            }
            for i in 1..cities.len() {
                if city_tiles_assigned[i].is_empty() || state_lang_pops[i].is_empty() {
                    continue;
                }
                let new_state = self.next_state_id;
                self.next_state_id += 1;
                let mut langs_by_pop: Vec<(u32, f32)> =
                    state_lang_pops[i].iter().map(|(&l, &p)| (l, p)).collect();
                langs_by_pop
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let maj_lang = langs_by_pop[0].0;

                let mean_frontier: f32 = city_tiles_assigned[i]
                    .iter()
                    .map(|&f| self.tile_metaethnic_intensity(f, &fine_adj))
                    .sum::<f32>()
                    / (city_tiles_assigned[i].len() as f32);
                let breakaway_asabiya = (0.5 + 0.5 * mean_frontier).clamp(0.45, 0.95);

                for &f in &city_tiles_assigned[i] {
                    self.tile_states.insert(f, new_state);
                }
                let color_idx = (self.rng.next_u64() as usize) % NATION_PALETTE.len();
                self.states.insert(
                    new_state,
                    StateRecord {
                        id: new_state,
                        capital_tile: cities[i],
                        color_idx,
                        active: true,
                        asabiya: breakaway_asabiya,
                    },
                );

                // Coin the new state's name in its majority language and borrow into minority languages
                let mut temp_rng = self.rng.clone();
                if let Some(lang) = self.languages.get_mut(&maj_lang) {
                    lang.coin_concept(ConceptKey::State(new_state), &mut temp_rng);
                }
                self.rng = temp_rng;
                if let Some(donor) = self.languages.get(&maj_lang).cloned() {
                    for &(min_lang, _) in langs_by_pop.iter().skip(1) {
                        if let Some(lang) = self.languages.get_mut(&min_lang) {
                            lang.borrow_concept(ConceptKey::State(new_state), &donor);
                        }
                    }
                }
            }
            self.trade_partners
                .retain(|&(a, b)| a != n && b != n);
        }

        // 8. Update trade, conflicts, concept accumulation, and recolor nations
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
        let st_asabiya = self.state_asabiya(sid);
        let frontier_label = if st_asabiya >= 0.70 {
            "Marcher Frontier"
        } else if st_asabiya <= 0.35 {
            "Imperial Core"
        } else {
            "Cohesive"
        };
        let dom_lang = self.primary_language_for_state(sid);
        lines.push(format!(
            "State: {st_name} | Pop: {:.0}K | Asabiya: {:.0}% ({frontier_label}) | Fine Face #{fine_idx} ({fine_elev:.2} km, {:.1}K / {:.1}K)",
            st_pop,
            st_asabiya * 100.0,
            tile_pop,
            cap
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

        // Sample lexicon from primary language (including celestial Sun, Moons & Planets)
        if let Some(lid) = dom_lang {
            if let Some(lang) = self.languages.get(&lid) {
                lines.push(format!("--- {} Lexicon ---", lang.name()));
                for (&ck, orig) in lang.lexicon.iter().take(14) {
                    let gloss = match ck {
                        ConceptKey::Sun(0) => "sun (host star)".to_string(),
                        ConceptKey::Sun(id) => format!("sun #{}", id + 1),
                        ConceptKey::Moon(m) => {
                            if self.moon_count <= 1 && m == 0 {
                                "moon".to_string()
                            } else {
                                match m {
                                    0 => "moon #1 (inner M1)".to_string(),
                                    1 => "moon #2 (2:1 M2)".to_string(),
                                    2 => "moon #3 (4:1 M3)".to_string(),
                                    3 => "moon #4 (8:1 M4)".to_string(),
                                    _ => format!("moon #{}", m + 1),
                                }
                            }
                        }
                        ConceptKey::Planet(p) => match p {
                            0 => "planet #1 (home world)".to_string(),
                            1 => "planet #2 (inner swift)".to_string(),
                            2 => "planet #3 (morning star)".to_string(),
                            3 => "planet #4 (red outer)".to_string(),
                            4 => "planet #5 (giant planet)".to_string(),
                            5 => "planet #6 (ringed outer)".to_string(),
                            _ => format!("planet #{}", p + 1),
                        },
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
        self.tile_colors_rgb_with_optics(aspect, show_rivers, selected_tile, None)
    }

    pub fn tile_colors_rgb_with_optics(
        &self,
        aspect: &str,
        show_rivers: bool,
        selected_tile: Option<usize>,
        optics: Option<&crate::sun_sky::PlanetaryOptics>,
    ) -> Vec<f32> {
        let coarse_elevs = self.prehistory.tile_elevations();
        let felsities = self.prehistory.tile_felsities();
        let koeppen = self.prehistory.tile_koeppen_ids();
        let life = self.prehistory.tile_life();
        let num_display = self.terrain.display_faces.len();

        let ocean_rgb = if let Some(op) = optics {
            [
                (0.02 + 0.30 * op.sky_zenith_rgb[0]).clamp(0.0, 0.70),
                (0.05 + 0.38 * op.sky_zenith_rgb[1]).clamp(0.0, 0.75),
                (0.14 + 0.55 * op.sky_zenith_rgb[2]).clamp(0.0, 0.85),
            ]
        } else {
            [0.0, 0.0, 0.55]
        };

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
                    out.extend_from_slice(&ocean_rgb);
                    continue;
                }
                DisplayFaceSource::Fine(f) => f,
            };

            if self.terrain.fine_elevations[f_idx] <= 0.0 {
                out.extend_from_slice(&ocean_rgb);
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
                    } else if let Some(op) = optics {
                        let f = felsities[c_idx];
                        let veg = life[c_idx].clamp(0.0, 1.0);
                        let pf = op.plant_foliage_rgb;
                        let rock_r = 0.25;
                        let rock_g = (32.0 + 32.0 * f) / 255.0;
                        let rock_b = (64.0 * f) / 255.0;
                        out.extend_from_slice(&[
                            (rock_r * (1.0 - 0.55 * veg) + pf[0] * 0.72 * veg).clamp(0.0, 1.0),
                            (rock_g * (1.0 - 0.55 * veg) + pf[1] * 0.72 * veg).clamp(0.0, 1.0),
                            (rock_b * (1.0 - 0.55 * veg) + pf[2] * 0.72 * veg).clamp(0.0, 1.0),
                        ]);
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

    #[test]
    fn test_celestial_language_concepts_sun_moons_planets() {
        let mut rng = Pcg64Mcg::new(12345);
        let lang = LanguageRecord::new_for_state_with_celestial(0, 0, &[0, 1], 3, &mut rng);
        assert!(lang.describes(ConceptKey::Sun(0)));
        assert!(lang.describes(ConceptKey::Moon(0)));
        assert!(lang.describes(ConceptKey::Moon(1)));
        assert!(lang.describes(ConceptKey::Moon(2)));
        assert!(!lang.describes(ConceptKey::Moon(3)));
        assert!(lang.describes(ConceptKey::Planet(0)));
        assert!(lang.describes(ConceptKey::Planet(1)));
        assert!(lang.describes(ConceptKey::Planet(2)));

        // Proto roundtrip for celestial ConceptKey variants:
        for ck in [
            ConceptKey::Sun(0),
            ConceptKey::Moon(0),
            ConceptKey::Moon(1),
            ConceptKey::Planet(0),
            ConceptKey::Planet(4),
        ] {
            let p = ck.to_proto();
            assert_eq!(ConceptKey::from_proto(&p), ck);
        }
    }
}
