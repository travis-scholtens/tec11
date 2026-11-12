use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

use tec11_math_grid::{
    normal, DisplayFaceSource, DisplayMesh, FineParentage, Pcg64Mcg, PointTree, SphericalGrid,
    SubdividedTerrainGrid, Vec3,
};
use tec11_proto::PlanetSnapshotExt;

use crate::language::{Language, SoundChangeKind, Word, WordOrigin};
use crate::lifeforms::{settle_species_on_climate, SettledSpecies, SpeciesKind};
use crate::population::{
    is_farmable, is_habitable, is_passable, main_ocean, tile_population_thousands,
    PlanetaryGeography,
};
use crate::prehistory::PrehistorySimulation;
use crate::rivers::trace_coarse_rivers;
use crate::sun_sky::{
    generate_default_constellations, generate_solar_system_with_rng, Constellation, HomeStar,
    SolarSystemPlanet,
};

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
    Constellation(u32),
    Continent(u32),
    Ocean(u32),
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
            Self::Constellation(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Constellation as i32,
                id,
            },
            Self::Continent(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Continent as i32,
                id,
            },
            Self::Ocean(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Ocean as i32,
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
            tec11_proto::ConceptKind::Constellation => Self::Constellation(c.id),
            tec11_proto::ConceptKind::Continent => Self::Continent(c.id),
            tec11_proto::ConceptKind::Ocean => Self::Ocean(c.id),
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
        Self::new_for_state_with_system(id, state_id, species_indices, moon_count, &[], rng)
    }

    pub fn new_for_state_with_system(
        id: u32,
        state_id: u32,
        species_indices: &[usize],
        moon_count: u32,
        solar_system: &[SolarSystemPlanet],
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
        // Celestial concepts: Sun, Moons (0..moon_count), and Planets in the actual solar system
        rec.coin_concept(ConceptKey::Sun(0), rng);
        for m in 0..moon_count {
            if m == 0 || ((id + m) % 2 == 0) {
                rec.coin_concept(ConceptKey::Moon(m), rng);
            } else {
                rec.derive_concept(ConceptKey::Moon(0), ConceptKey::Moon(m), rng);
            }
        }
        if solar_system.is_empty() {
            // Fallback when no solar system slice is supplied:
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
        } else {
            for p in solar_system {
                if p.is_home_world || p.apparent_mag <= 1.8 {
                    // The home planet itself and brilliant morning/evening stars or gas giants are named universally:
                    rec.coin_concept(ConceptKey::Planet(p.index), rng);
                } else if p.is_naked_eye_visible() {
                    // Classical naked-eye wandering planets are named by astronomer traditions and shared across trade routes:
                    if (state_id + p.index) % 2 == 0 {
                        rec.coin_concept(ConceptKey::Planet(p.index), rng);
                    }
                } else if (state_id + p.index) % 3 == 0 {
                    // Outer / dimmer planets in the system discovered by observant sky-watchers:
                    rec.coin_concept(ConceptKey::Planet(p.index), rng);
                }
            }
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
        if let Some(donor_origin) = donor.lexicon.get(&key) {
            self.borrow_concept_origin(key, donor.id, donor_origin);
        }
    }

    pub fn borrow_concept_origin(
        &mut self,
        key: ConceptKey,
        donor_id: u32,
        donor_origin: &WordOrigin,
    ) {
        if self.lexicon.contains_key(&key) {
            return;
        }
        let mut local_rng = Pcg64Mcg::new(
            ((self.id as u128) << 64)
                ^ ((donor_id as u128) << 32)
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

/// Canonical ordered pair of state IDs (`a <= b`), representing an undirected bilateral relationship
/// such as a trade partnership or military conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatePair {
    pub a: u32,
    pub b: u32,
}

impl StatePair {
    #[inline]
    pub fn new(u: u32, v: u32) -> Self {
        if u <= v {
            Self { a: u, b: v }
        } else {
            Self { a: v, b: u }
        }
    }

    #[inline]
    pub fn contains(self, state_id: u32) -> bool {
        self.a == state_id || self.b == state_id
    }

    #[inline]
    pub fn other(self, state_id: u32) -> Option<u32> {
        if self.a == state_id {
            Some(self.b)
        } else if self.b == state_id {
            Some(self.a)
        } else {
            None
        }
    }

    #[inline]
    pub fn to_proto(self) -> tec11_proto::StatePair {
        tec11_proto::StatePair {
            state_a: self.a,
            state_b: self.b,
        }
    }

    #[inline]
    pub fn from_proto(sp: &tec11_proto::StatePair) -> Self {
        Self::new(sp.state_a, sp.state_b)
    }
}

/// Physical mechanism of a water travel corridor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TravelRouteKind {
    /// Long-distance pelagic streamline driven by open-ocean winds and gyre currents.
    OceanGyre,
    /// Bounded coastal, island-hopping, or monsoonal sea lane.
    CoastalSeaLane,
    /// Navigable inland river corridor (downstream or upstream).
    RiverCorridor,
}

impl TravelRouteKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::OceanGyre => "Ocean Current & Wind Gyre",
            Self::CoastalSeaLane => "Coastal / Monsoon Sea Lane",
            Self::RiverCorridor => "Riverine Corridor",
        }
    }
}

/// Diplomatic and historical usage status of a [`TravelRoute`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TravelRouteStatus {
    /// Unsettled wind/current corridor with no active port polity at its origin.
    PotentialCorridor,
    /// Active expansion or domestic sea/river lane established by a water-access polity.
    ActiveSeaLane,
    /// Active naval or amphibious war route between conflicting states.
    NavalConflict,
    /// Active maritime or riverine trade route between trading partner states.
    MaritimeTrade,
}

impl TravelRouteStatus {
    pub fn rgb(self, kind: TravelRouteKind) -> [f32; 3] {
        match self {
            Self::MaritimeTrade => [1.00, 0.88, 0.18],
            Self::NavalConflict => [1.00, 0.22, 0.22],
            Self::ActiveSeaLane => match kind {
                TravelRouteKind::RiverCorridor => [0.25, 0.96, 0.68],
                TravelRouteKind::OceanGyre | TravelRouteKind::CoastalSeaLane => [0.18, 0.90, 0.98],
            },
            Self::PotentialCorridor => [0.36, 0.50, 0.84],
        }
    }

    pub fn radius_lift(self) -> f32 {
        match self {
            Self::MaritimeTrade | Self::NavalConflict => 1.010,
            Self::ActiveSeaLane => 1.008,
            Self::PotentialCorridor => 1.005,
        }
    }

    pub fn ocean_sector_label(self) -> &'static str {
        match self {
            Self::MaritimeTrade => "TRADE ROUTE (gold)",
            Self::NavalConflict => "NAVAL CONFLICT (red)",
            Self::ActiveSeaLane => "ACTIVE SEA LANE (cyan)",
            Self::PotentialCorridor => "Potential Wind/Current Corridor",
        }
    }

    pub fn land_label(self) -> &'static str {
        match self {
            Self::MaritimeTrade => "MARITIME TRADE",
            Self::NavalConflict => "NAVAL CONFLICT",
            Self::ActiveSeaLane | Self::PotentialCorridor => "EXPANSION / SEA LANE",
        }
    }
}

/// A maritime or riverine travel route connecting a water-access origin tile to a destination
/// landing tile along prevailing winds, surface ocean currents, or navigable rivers.
#[derive(Clone, Debug, PartialEq)]
pub struct TravelRoute {
    /// Origin fine face index (populated coastal port or river landing).
    pub from_fine_face: usize,
    /// Destination fine face index (reachable coastal landing or river port).
    pub to_fine_face: usize,
    /// Origin coarse tile index.
    pub from_coarse_tile: usize,
    /// Destination coarse tile index.
    pub to_coarse_tile: usize,
    /// Coarse tile trajectory (`from_coarse_tile ..= to_coarse_tile`) across ocean currents/winds or rivers.
    pub coarse_path: Vec<usize>,
    /// Effective wind/current-weighted travel cost (lower = faster/easier voyage).
    pub effective_cost: f32,
    /// Mean wind + ocean current alignment factor in `[0.0, 1.0]`.
    pub wind_current_alignment: f32,
    /// Physical mechanism of this travel route.
    pub route_kind: TravelRouteKind,
}

impl TravelRoute {
    pub fn kind_name(&self) -> &'static str {
        self.route_kind.name()
    }
}

/// Computed territorial, ecological, and diplomatic adjacency topology of all active states.
#[derive(Clone, Debug)]
pub struct StateTopology {
    pub extents: BTreeMap<u32, Vec<usize>>,
    pub species: BTreeMap<u32, BTreeSet<usize>>,
    pub neighbors: BTreeMap<u32, BTreeSet<u32>>,
}

#[derive(Clone, Copy, Debug)]
struct NavEdge {
    target: usize,
    cost: f32,
    alignment: f32,
}

#[derive(Clone, Copy, Debug)]
struct OceanSearchNode {
    cost_int: u32,
    prev_tile: usize,
    sum_alignment: f32,
    hops: usize,
}

#[derive(Clone, Debug)]
struct MigrationSpillover {
    target_face: usize,
    state_id: u32,
    community: Community,
}

#[derive(Clone, Copy, Debug)]
struct WarOutcome {
    winner: u32,
    loser: u32,
}

/// Complete History simulation (`historysimulation.py`, `historydisplay.py`, `historypresenter.py`).
pub struct HistorySimulation {
    prehistory: PrehistorySimulation,
    terrain: SubdividedTerrainGrid,
    tree: PointTree,
    history_ticks: u32,
    moon_count: u32,
    solar_system: Vec<SolarSystemPlanet>,
    constellations: Vec<Constellation>,
    geography: PlanetaryGeography,
    settled_species: Vec<SettledSpecies>,
    tile_species: Vec<Vec<usize>>,
    rivers: Vec<Vec<usize>>,
    river_routes: Vec<Vec<usize>>,
    river_tiles: Vec<bool>,
    capacities: Vec<f32>,
    /// Representative coastal or river fine face (`Some(f_idx)`) for each coarse tile `c` with water access.
    coarse_port_fine_face: Vec<Option<usize>>,
    /// All physically navigable wind/current/river corridors connecting habitable coasts and river basins.
    potential_travel_routes: Vec<TravelRoute>,
    /// Active travel routes currently established by populated water-access communities/states.
    travel_routes: Vec<TravelRoute>,
    tile_communities: BTreeMap<usize, Vec<Community>>,
    tile_states: BTreeMap<usize, u32>,
    states: BTreeMap<u32, StateRecord>,
    languages: BTreeMap<u32, LanguageRecord>,
    nationalities: BTreeSet<u32>,
    trade_partners: Vec<StatePair>,
    conflicts: Vec<StatePair>,
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
        prehistory: PrehistorySimulation,
        moon_count: u32,
        seed: u128,
    ) -> Self {
        let mut sys_rng = Pcg64Mcg::from_seed_u64(42);
        let solar_system = generate_solar_system_with_rng(
            &mut sys_rng,
            &HomeStar::sol(),
            [0.0, -0.398, 0.917],
            true,
        );
        Self::from_prehistory_with_system(prehistory, moon_count, solar_system, seed)
    }

    pub fn from_prehistory_with_system(
        prehistory: PrehistorySimulation,
        moon_count: u32,
        solar_system: Vec<SolarSystemPlanet>,
        seed: u128,
    ) -> Self {
        let constellations = generate_default_constellations(seed as u64);
        Self::from_prehistory_with_sky(prehistory, moon_count, solar_system, constellations, seed)
    }

    pub fn from_prehistory_with_sky(
        mut prehistory: PrehistorySimulation,
        moon_count: u32,
        solar_system: Vec<SolarSystemPlanet>,
        constellations: Vec<Constellation>,
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
            .map(|(i, face)| (face.center, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);

        let mut sim = Self {
            prehistory,
            terrain,
            tree,
            history_ticks: 0,
            moon_count,
            solar_system,
            constellations,
            geography: PlanetaryGeography::default(),
            settled_species: Vec::new(),
            tile_species: vec![Vec::new(); num_fine],
            rivers: Vec::new(),
            river_routes: Vec::new(),
            river_tiles: vec![false; num_fine],
            capacities: vec![0.0; num_fine],
            coarse_port_fine_face: Vec::new(),
            potential_travel_routes: Vec::new(),
            travel_routes: Vec::new(),
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

        // 1. Identify continents and ocean basins, settle plants & animals on the planet's coarse climate,
        //    then map culturally/economically salient flora & fauna onto subdivided fine faces (`historysimulation.py:facespecies`).
        let biosphere = settle_species_on_climate(&coarse_elevs, self.prehistory.climate_model());
        self.geography = biosphere.geography;
        let coarse_species = biosphere.tile_all_species;
        self.settled_species = biosphere.species;
        let salient_coarse_species: Vec<Vec<usize>> = (0..num_coarse)
            .map(|c| {
                let mut sp_list = coarse_species[c].clone();
                if sp_list.len() > 4 {
                    sp_list.sort_by_key(|&sp_id| {
                        let sp = &self.settled_species[sp_id];
                        let priority = match (sp.kind, sp.trophic_level) {
                            (SpeciesKind::Animal, 4) => 0u32,
                            (SpeciesKind::Animal, 3) => 1u32,
                            (SpeciesKind::Animal, _) => 2u32,
                            (SpeciesKind::Plant, _) => 2u32,
                        };
                        let hash = (sp_id as u32)
                            .wrapping_mul(0x9E37_79B9)
                            .rotate_left(13)
                            ^ (c as u32).wrapping_mul(0x85EB_CA6B);
                        (priority, hash)
                    });
                    sp_list.truncate(4);
                    sp_list.sort_unstable();
                }
                sp_list
            })
            .collect();
        self.tile_species = Vec::with_capacity(num_fine);
        for f_idx in 0..num_fine {
            let sp_vec = match self.terrain.parentage[f_idx] {
                FineParentage::CoarseFace(c)
                | FineParentage::CoarseEdge {
                    primary_coarse: c, ..
                } => salient_coarse_species[c].clone(),
                FineParentage::CoarseVertex([c0, c1, c2]) => {
                    let mut set = BTreeSet::new();
                    for &c in &[c0, c1, c2] {
                        for &sp in &salient_coarse_species[c] {
                            set.insert(sp);
                        }
                    }
                    set.into_iter().take(6).collect()
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

            let lang = LanguageRecord::new_for_state_with_system(
                lid,
                sid,
                &self.tile_species[city_f],
                self.moon_count,
                &self.solar_system,
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
                        if lang.lexicon.len() < 28 {
                            for &sp_idx in &self.tile_species[nb] {
                                lang.coin_concept(ConceptKey::Species(sp_idx as u32), &mut self.rng);
                            }
                        }
                    }
                    q.push_back((nb, sid, dist + step_cost));
                }
            }
        }

        // Coin all constellations visible at some point in the year and home continents/oceans from each state's territory:
        let mut init_extents: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (&f_idx, &sid) in &self.tile_states {
            init_extents.entry(sid).or_default().push(f_idx);
        }
        for (sid, tiles) in &init_extents {
            let vis = self.visible_constellations_for_tiles(tiles);
            let (conts, oceans) = self.continents_and_oceans_for_tiles(tiles, &coarse_elevs);
            let mut temp_rng = self.rng.clone();
            if let Some(lang) = self.languages.get_mut(sid) {
                for cid in conts {
                    lang.coin_concept(ConceptKey::Continent(cid), &mut temp_rng);
                }
                for oid in oceans {
                    lang.coin_concept(ConceptKey::Ocean(oid), &mut temp_rng);
                }
                for c_idx in vis {
                    lang.coin_concept(ConceptKey::Constellation(c_idx), &mut temp_rng);
                }
                lang.refresh_phonology();
            }
            self.rng = temp_rng;
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

        // 5. Compute navigable wind, ocean-current, and riverine travel routes and activate routes for populated water-access polities.
        self.compute_potential_travel_routes(&ocean, &coarse_elevs, &koeppen, &coarse_adj, &fine_adj, &fine_elevs);
        self.update_travel_routes();

        self.recolor_states(&fine_adj);
        self.update_diplomacy_and_trade(&fine_adj);
    }

    /// Precompute physically navigable maritime and riverine travel corridors (`potential_travel_routes`)
    /// driven by prevailing planetary winds (`wind_vector_xyz`), 8-season surface ocean currents (`velocity_xyz`),
    /// coastal cabotage, and major river routes (`self.rivers`).
    fn compute_potential_travel_routes(
        &mut self,
        ocean_set: &BTreeSet<usize>,
        coarse_elevs: &[f32],
        koeppen: &[u8],
        coarse_adj: &[Vec<usize>],
        fine_adj: &[Vec<usize>],
        fine_elevs: &[f32],
    ) {
        let num_coarse = coarse_elevs.len();
        let num_fine = fine_elevs.len();
        let grid = self.prehistory.grid();
        let climate = self.prehistory.climate_model();

        // 1. Identify which coarse land tiles have water access (ocean coast or major river)
        //    and select their best coastal/river fine port face.
        let mut on_coarse_river = vec![false; num_coarse];
        for r in &self.rivers {
            for &c in r {
                if c < num_coarse {
                    on_coarse_river[c] = true;
                }
            }
        }
        let mut is_coastal_coarse = vec![false; num_coarse];
        for c in 0..num_coarse {
            if coarse_elevs[c] > 0.0
                && is_passable(coarse_elevs[c], koeppen[c], true)
                && coarse_adj[c]
                    .iter()
                    .any(|&nb| ocean_set.contains(&nb) || coarse_elevs[nb] <= 0.0)
            {
                is_coastal_coarse[c] = true;
            }
        }

        let mut coarse_ports: Vec<Option<usize>> = vec![None; num_coarse];
        let mut coarse_best_score: Vec<f32> = vec![-1.0; num_coarse];
        for f_idx in 0..num_fine {
            if fine_elevs[f_idx] <= 0.0 || self.capacities[f_idx] <= 0.05 {
                continue;
            }
            let c = self.terrain.primary_coarse_tile(f_idx, coarse_elevs);
            if !is_coastal_coarse[c] && !on_coarse_river[c] {
                continue;
            }
            let touches_water = self.river_tiles[f_idx]
                || fine_adj[f_idx].iter().any(|&nb| fine_elevs[nb] <= 0.0);
            let score = self.capacities[f_idx]
                + if touches_water { 10.0 } else { 0.0 }
                + if self.river_tiles[f_idx] { 5.0 } else { 0.0 };
            if score > coarse_best_score[c] {
                coarse_best_score[c] = score;
                coarse_ports[c] = Some(f_idx);
            }
        }
        self.coarse_port_fine_face = coarse_ports;

        // 2. Build directed wind + ocean-current transition table for coarse ocean tiles.
        let num_seasons = climate.season_count().max(1);
        let wind_vecs: Vec<[f32; 3]> = (0..num_coarse)
            .map(|c| climate.wind_vector_xyz(c))
            .collect();

        // For each coarse ocean tile `u`, store navigable transitions `NavEdge { target, cost, alignment }`
        let nav_edges: Vec<Vec<NavEdge>> = (0..num_coarse)
            .map(|u| {
                if coarse_elevs[u] > 0.0 {
                    return Vec::new();
                }
                let pu = grid.faces[u].0;
                let wu = wind_vecs[u];
                let mut edges: Vec<NavEdge> = coarse_adj[u]
                    .iter()
                    .filter_map(|&v| {
                        let pv = grid.faces[v].0;
                        let dx = (pv[0] - pu[0]) as f32;
                        let dy = (pv[1] - pu[1]) as f32;
                        let dz = (pv[2] - pu[2]) as f32;
                        let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
                        let dir = [dx / len, dy / len, dz / len];

                        let wind_dot = wu[0] * dir[0] + wu[1] * dir[1] + wu[2] * dir[2];
                        let best_align = (0..num_seasons)
                            .filter_map(|s_idx| {
                                let oc = climate.ocean_for_season(s_idx)?;
                                let vel = oc.velocity_xyz.get(u)?;
                                let cur_dot = vel[0] * dir[0] + vel[1] * dir[1] + vel[2] * dir[2];
                                Some(0.45 * wind_dot + 0.85 * cur_dot)
                            })
                            .fold(0.45 * wind_dot, f32::max);

                        let v_touches_land = coarse_elevs[v] > 0.0
                            || coarse_adj[v].iter().any(|&nb| coarse_elevs[nb] > 0.0);
                        (best_align >= 0.06 || (v_touches_land && best_align >= -0.20)).then(|| {
                            let cost = if best_align >= 0.06 {
                                (1.0 - 0.55 * best_align).clamp(0.35, 0.95)
                            } else {
                                1.05
                            };
                            let alignment = ((best_align + 0.25) / 1.25).clamp(0.10, 1.0);
                            NavEdge {
                                target: v,
                                cost,
                                alignment,
                            }
                        })
                    })
                    .collect();
                edges.sort_by(|a, b| {
                    a.cost
                        .partial_cmp(&b.cost)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                edges
            })
            .collect();

        let mut routes_by_pair: BTreeMap<(usize, usize), TravelRoute> = BTreeMap::new();

        // 3. Trace wind & ocean-current gyre streamlines and bounded coastal/monsoon sea lanes from each coastal port.
        let coastal_ports: Vec<(usize, usize)> = (0..num_coarse)
            .filter_map(|c| {
                is_coastal_coarse[c]
                    .then(|| self.coarse_port_fine_face[c].map(|f| (c, f)))
                    .flatten()
            })
            .collect();

        // Stride coastal ports so we get well-spaced maritime hubs across every coastline (~90 hubs)
        let stride = (coastal_ports.len() / 90).max(1);
        for (port_idx, &(c_start, f_start)) in coastal_ports.iter().enumerate() {
            let is_river_mouth = on_coarse_river[c_start];
            if port_idx % stride != 0 && !is_river_mouth {
                continue;
            }
            let direct_land_nbs: BTreeSet<usize> =
                coarse_adj[c_start].iter().copied().collect();

            // (a) Wind & Ocean-Current Streamline Tracing (long-distance pelagic gyre voyages)
            for &o_start in &coarse_adj[c_start] {
                if coarse_elevs[o_start] > 0.0 {
                    continue;
                }
                let mut curr = o_start;
                let mut path = vec![c_start, o_start];
                let mut visited = BTreeSet::from([c_start, o_start]);
                let mut total_cost = 0.6f32;
                let mut total_align = 0.65f32;
                let mut steps = 1usize;

                for _ in 0..16 {
                    // Check if `curr` is adjacent to a distant coastal port with favorable onshore wind/current
                    if steps >= 2 {
                        if let Some(&edge) = nav_edges[curr].iter().find(|edge| {
                            edge.target != c_start
                                && !direct_land_nbs.contains(&edge.target)
                                && self
                                    .coarse_port_fine_face
                                    .get(edge.target)
                                    .copied()
                                    .flatten()
                                    .is_some()
                        }) {
                            let land_v = edge.target;
                            let f_end = self.coarse_port_fine_face[land_v].unwrap();
                            let mut full_path = path.clone();
                            full_path.push(land_v);
                            let eff_cost = total_cost + edge.cost;
                            let mean_align = ((total_align + edge.alignment)
                                / (steps as f32 + 1.0))
                                .clamp(0.1, 1.0);
                            let route = TravelRoute {
                                from_fine_face: f_start,
                                to_fine_face: f_end,
                                from_coarse_tile: c_start,
                                to_coarse_tile: land_v,
                                coarse_path: full_path,
                                effective_cost: eff_cost,
                                wind_current_alignment: mean_align,
                                route_kind: TravelRouteKind::OceanGyre,
                            };
                            let key = (c_start, land_v);
                            if routes_by_pair
                                .get(&key)
                                .is_none_or(|old| eff_cost < old.effective_cost)
                            {
                                routes_by_pair.insert(key, route);
                            }
                            break;
                        }
                    }

                    // Advance to the strongest downwind / down-current ocean tile
                    let Some(next_edge) = nav_edges[curr]
                        .iter()
                        .copied()
                        .find(|edge| coarse_elevs[edge.target] <= 0.0 && !visited.contains(&edge.target))
                    else {
                        break;
                    };
                    visited.insert(next_edge.target);
                    path.push(next_edge.target);
                    total_cost += next_edge.cost;
                    total_align += next_edge.alignment;
                    steps += 1;
                    curr = next_edge.target;
                }
            }

            // (b) Bounded Dijkstra search for nearby coastal/island/monsoon sea lanes (up to 7 ocean steps)
            let mut best_ocean_cost: BTreeMap<usize, OceanSearchNode> = BTreeMap::new();
            let mut heap: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::new();
            for &o_nb in &coarse_adj[c_start] {
                if coarse_elevs[o_nb] <= 0.0 {
                    best_ocean_cost.insert(
                        o_nb,
                        OceanSearchNode {
                            cost_int: 60,
                            prev_tile: c_start,
                            sum_alignment: 0.70,
                            hops: 1,
                        },
                    );
                    heap.push(Reverse((60, o_nb)));
                }
            }
            let mut found_destinations = 0usize;
            while let Some(Reverse((cost_int, u))) = heap.pop() {
                let Some(&node) = best_ocean_cost.get(&u) else {
                    continue;
                };
                if cost_int > node.cost_int || cost_int > 550 || node.hops > 7 {
                    continue;
                }
                for &edge in &nav_edges[u] {
                    let v = edge.target;
                    let next_cost = cost_int + (edge.cost * 100.0) as u32;
                    if coarse_elevs[v] > 0.0 {
                        if node.hops >= 2
                            && v != c_start
                            && !direct_land_nbs.contains(&v)
                            && found_destinations < 2
                        {
                            if let Some(f_end) = self.coarse_port_fine_face[v] {
                                let key = (c_start, v);
                                if !routes_by_pair.contains_key(&key) {
                                    // Reconstruct coarse path
                                    let mut rev_path = vec![v, u];
                                    let mut trace = u;
                                    while let Some(&prev_node) = best_ocean_cost.get(&trace) {
                                        rev_path.push(prev_node.prev_tile);
                                        if prev_node.prev_tile == c_start || rev_path.len() > 12 {
                                            break;
                                        }
                                        trace = prev_node.prev_tile;
                                    }
                                    rev_path.reverse();
                                    let eff_cost = (next_cost as f32) / 100.0;
                                    let mean_align = ((node.sum_alignment + edge.alignment)
                                        / (node.hops as f32 + 1.0))
                                        .clamp(0.1, 1.0);
                                    routes_by_pair.insert(
                                        key,
                                        TravelRoute {
                                            from_fine_face: f_start,
                                            to_fine_face: f_end,
                                            from_coarse_tile: c_start,
                                            to_coarse_tile: v,
                                            coarse_path: rev_path,
                                            effective_cost: eff_cost,
                                            wind_current_alignment: mean_align,
                                            route_kind: TravelRouteKind::CoastalSeaLane,
                                        },
                                    );
                                    found_destinations += 1;
                                }
                            }
                        }
                    } else if node.hops < 7 && next_cost <= 520 {
                        let is_better = best_ocean_cost
                            .get(&v)
                            .is_none_or(|old| next_cost < old.cost_int);
                        if is_better {
                            best_ocean_cost.insert(
                                v,
                                OceanSearchNode {
                                    cost_int: next_cost,
                                    prev_tile: u,
                                    sum_alignment: node.sum_alignment + edge.alignment,
                                    hops: node.hops + 1,
                                },
                            );
                            heap.push(Reverse((next_cost, v)));
                        }
                    }
                }
            }
        }

        // 4. Add navigable riverine corridors (`TravelRouteKind::RiverCorridor`) along major coarse rivers.
        for r in &self.rivers {
            let valid_land: Vec<usize> = r
                .iter()
                .copied()
                .filter(|&c| {
                    c < num_coarse
                        && coarse_elevs[c] > 0.0
                        && self.coarse_port_fine_face[c].is_some()
                })
                .collect();
            if valid_land.len() >= 3 {
                let c_head = valid_land[0];
                let c_mouth = *valid_land.last().unwrap();
                if c_head != c_mouth {
                    let f_head = self.coarse_port_fine_face[c_head].unwrap();
                    let f_mouth = self.coarse_port_fine_face[c_mouth].unwrap();
                    let downstream_cost = (valid_land.len() as f32 * 0.35).clamp(0.5, 4.0);
                    routes_by_pair.insert(
                        (c_head, c_mouth),
                        TravelRoute {
                            from_fine_face: f_head,
                            to_fine_face: f_mouth,
                            from_coarse_tile: c_head,
                            to_coarse_tile: c_mouth,
                            coarse_path: valid_land.clone(),
                            effective_cost: downstream_cost,
                            wind_current_alignment: 0.90,
                            route_kind: TravelRouteKind::RiverCorridor,
                        },
                    );
                    let mut upstream_path = valid_land;
                    upstream_path.reverse();
                    routes_by_pair.insert(
                        (c_mouth, c_head),
                        TravelRoute {
                            from_fine_face: f_mouth,
                            to_fine_face: f_head,
                            from_coarse_tile: c_mouth,
                            to_coarse_tile: c_head,
                            coarse_path: upstream_path,
                            effective_cost: downstream_cost * 1.35,
                            wind_current_alignment: 0.65,
                            route_kind: TravelRouteKind::RiverCorridor,
                        },
                    );
                }
            }
        }

        let mut potential: Vec<TravelRoute> = routes_by_pair.into_values().collect();
        potential.sort_by(|a, b| {
            a.effective_cost
                .partial_cmp(&b.effective_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.potential_travel_routes = potential;
    }

    /// Re-evaluate active `travel_routes` from `potential_travel_routes` based on which water-access
    /// coarse tiles currently have populated communities or state presence.
    pub fn update_travel_routes(&mut self) {
        let coarse_elevs = self.prehistory.tile_elevations();
        let num_coarse = coarse_elevs.len();
        let mut coarse_active_fine: Vec<Option<usize>> = vec![None; num_coarse];

        // Any populated or state-claimed fine face on a coarse tile activates that coarse tile's water access
        for (&f_idx, &sid) in &self.tile_states {
            let pop: f32 = self
                .tile_communities
                .get(&f_idx)
                .map(|cs| cs.iter().map(|c| c.thousands).sum())
                .unwrap_or(0.0);
            if pop > 0.0 || self.states.contains_key(&sid) {
                let c = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                if c < num_coarse {
                    // Prefer the designated port fine face if it is claimed by a state
                    if self.coarse_port_fine_face.get(c).copied().flatten() == Some(f_idx)
                        || coarse_active_fine[c].is_none()
                    {
                        coarse_active_fine[c] = Some(f_idx);
                    }
                }
            }
        }

        self.travel_routes = self
            .potential_travel_routes
            .iter()
            .filter_map(|route| {
                let active_origin_fine = coarse_active_fine
                    .get(route.from_coarse_tile)
                    .copied()
                    .flatten()?;
                let to_fine_face = coarse_active_fine
                    .get(route.to_coarse_tile)
                    .copied()
                    .flatten()
                    .unwrap_or(route.to_fine_face);
                Some(TravelRoute {
                    from_fine_face: active_origin_fine,
                    to_fine_face,
                    ..route.clone()
                })
            })
            .collect();
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
            let used_colors: BTreeSet<usize> = state_adj
                .get(&sid)
                .into_iter()
                .flatten()
                .filter(|&&nb_sid| nb_sid < sid)
                .filter_map(|nb_sid| self.states.get(nb_sid).map(|st| st.color_idx))
                .collect();
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
        self.trade_partners
            .iter()
            .filter_map(|pair| pair.other(n))
            .collect()
    }

    /// Transitive trade network of state `n` with shortest hop counts `h >= 1` (`recursivetradepartners`).
    fn recursive_trade_partner_distances(&self, n: u32) -> BTreeMap<u32, usize> {
        let mut dists = BTreeMap::from([(n, 0usize)]);
        let mut q = VecDeque::from([(n, 0usize)]);
        while let Some((curr, d)) = q.pop_front() {
            for nb in self.lookup_trade_partners(curr) {
                if let std::collections::btree_map::Entry::Vacant(e) = dists.entry(nb) {
                    e.insert(d + 1);
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
            .map(|&(c, _)| c)
            .unwrap_or([1.0, 0.0, 0.0]);
        let c2 = self
            .states
            .get(&s2)
            .and_then(|s| self.terrain.terrain_grid.faces.get(s.capital_tile))
            .map(|&(c, _)| c)
            .unwrap_or([1.0, 0.0, 0.0]);
        let dot = (c1[0] * c2[0] + c1[1] * c2[1] + c1[2] * c2[2]).clamp(-1.0, 1.0);
        dot.acos() as f32
    }

    /// Effective distance between states `s1` and `s2`, taking into account lower friction along
    /// established maritime/riverine wind & current travel routes (`self.travel_routes`).
    fn effective_state_distance(&self, s1: u32, s2: u32) -> f32 {
        let pair = StatePair::new(s1, s2);
        self.travel_routes
            .iter()
            .filter_map(|r| {
                let st_from = *self.tile_states.get(&r.from_fine_face)?;
                let st_to = *self.tile_states.get(&r.to_fine_face)?;
                (st_from != st_to && StatePair::new(st_from, st_to) == pair)
                    .then(|| (r.effective_cost * 0.14).clamp(0.08, 1.15))
            })
            .fold(self.capital_angular_distance(s1, s2), f32::min)
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

        fine_adj[f_idx]
            .iter()
            .filter_map(|&nb| {
                if self.tile_states.get(&nb) == Some(&sid) {
                    return None;
                }
                let nb_comms = self.tile_communities.get(&nb)?;
                let nb_pop: f32 = nb_comms.iter().map(|c| c.thousands).sum();
                if nb_pop <= 0.0 {
                    return None;
                }
                let nb_ag = nb_comms.iter().any(|c| c.agriculture);
                let ag_diff = if f_ag != nb_ag { 0.50 } else { 0.0 };
                let nat_diff = if f_nat != self.majority_nationality(nb) {
                    0.30
                } else {
                    0.0
                };
                let lang_diff = if f_lang != self.majority_language(nb) {
                    0.20
                } else {
                    0.0
                };
                Some(ag_diff + nat_diff + lang_diff)
            })
            .fold(0.0f32, f32::max)
            .clamp(0.0, 1.0)
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
        let new_asabiya: Vec<(u32, f32)> = extents
            .iter()
            .filter(|(_, tiles)| !tiles.is_empty())
            .map(|(&sid, tiles)| {
                let sum_i: f32 = tiles
                    .iter()
                    .map(|&f| self.tile_metaethnic_intensity(f, fine_adj))
                    .sum();
                // Scale frontier exposure so compact frontier marcher states reach high I_bar while sprawling empires dilute I_bar
                let mean_i = (sum_i / (tiles.len() as f32) * 2.5).clamp(0.0, 1.0);
                let s = self.state_asabiya(sid);
                let ds = R_FRONTIER * mean_i * (1.0 - s) - DELTA_CORE * (1.0 - mean_i) * s;
                (sid, (s + ds).clamp(0.10, 1.00))
            })
            .collect();
        for (sid, s) in new_asabiya {
            if let Some(st) = self.states.get_mut(&sid) {
                st.asabiya = s;
            }
        }
    }

    /// Active military opponents of state `n` (`historysimulation.py:lookupopponents`).
    fn lookup_opponents(&self, n: u32) -> BTreeSet<u32> {
        self.conflicts
            .iter()
            .filter_map(|pair| pair.other(n))
            .collect()
    }

    /// Compute per-state tile extents, native species sets, and neighboring states (including across rivers
    /// and along established maritime/riverine wind & current travel routes).
    fn state_topology(&self, adjacency: &[Vec<usize>]) -> StateTopology {
        let mut extents: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut species: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
        let mut neighbors: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();

        for (&t, &s1) in &self.tile_states {
            extents.entry(s1).or_default().push(t);
            let sp_set = species.entry(s1).or_default();
            if sp_set.len() < 20 {
                for &sp in &self.tile_species[t] {
                    if sp_set.len() >= 20 {
                        break;
                    }
                    sp_set.insert(sp);
                }
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

        // Also link states connected by established maritime or riverine travel routes (`self.travel_routes`)
        for r in &self.travel_routes {
            if let (Some(&s1), Some(&s2)) = (
                self.tile_states.get(&r.from_fine_face),
                self.tile_states.get(&r.to_fine_face),
            ) {
                if s1 != s2 {
                    neighbors.entry(s1).or_default().insert(s2);
                    neighbors.entry(s2).or_default().insert(s1);
                }
            }
        }

        StateTopology {
            extents,
            species,
            neighbors,
        }
    }

    /// Hop-decayed Heckscher-Ohlin & Tinbergen trade pressure $\text{Pressure}_{n \leftarrow o}$:
    /// each novel species $s \notin S_n$ available in $o$ (at hop $h = 0$) or via $o$'s recursive trade
    /// network at minimum hop $h \ge 1$ contributes $0.5^h$, damped by effective land/maritime distance $e^{-0.25 d_{no}}$.
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
        let dist = self.effective_state_distance(n, o);
        raw_gain * (-0.25 * dist).exp()
    }

    /// Update trade partnerships and military conflicts using:
    /// 1. Heckscher-Ohlin comparative advantage + Tinbergen hop/distance gravity decay (`compute_trade_pressure`),
    /// 2. Polachek commercial peace (active war blocks trade; trade raises war threshold by peace dividend $\tau_{\text{trade}}$),
    /// 3. Collins resource + territorial carrying-capacity + demographic tension with Boulding distance decay (`e^{-0.5 d}`).
    fn update_diplomacy_and_trade(&mut self, adjacency: &[Vec<usize>]) {
        self.update_travel_routes();
        let StateTopology {
            extents,
            species: state_species,
            neighbors,
        } = self.state_topology(adjacency);
        let active_states: BTreeSet<u32> = extents.keys().copied().collect();
        for st in self.states.values_mut() {
            st.active = active_states.contains(&st.id);
        }

        // Prune any stale trade partnerships or conflicts involving extinct or non-neighboring states
        let is_valid_pair = |pair: &StatePair| {
            active_states.contains(&pair.a)
                && active_states.contains(&pair.b)
                && pair.a != pair.b
                && neighbors.get(&pair.a).is_some_and(|nbs| nbs.contains(&pair.b))
        };
        self.trade_partners.retain(is_valid_pair);
        self.conflicts.retain(is_valid_pair);

        let active_conflict_set: BTreeSet<StatePair> = self.conflicts.iter().copied().collect();

        // 1. Heckscher-Ohlin + Tinbergen trade pressure & mutual eligibility
        let mut pressure_map: BTreeMap<(u32, u32), f32> = BTreeMap::new();
        let mut eligible: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &n in &active_states {
            let native_len = state_species.get(&n).map(|s| s.len()).unwrap_or(0) as f32;
            let min_gain = (native_len * 0.05).max(0.5);
            let mut n_elig = BTreeSet::new();
            if let Some(nbs) = neighbors.get(&n) {
                for &o in nbs {
                    let pair = StatePair::new(n, o);
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
        let mut partner_set: BTreeSet<StatePair> = self
            .trade_partners
            .iter()
            .copied()
            .filter(|&pair| {
                let p_ab = pressure_map.get(&(pair.a, pair.b)).copied().unwrap_or(0.0);
                let p_ba = pressure_map.get(&(pair.b, pair.a)).copied().unwrap_or(0.0);
                let min_a =
                    (state_species.get(&pair.a).map(|s| s.len()).unwrap_or(0) as f32) * 0.025;
                let min_b =
                    (state_species.get(&pair.b).map(|s| s.len()).unwrap_or(0) as f32) * 0.025;
                p_ab >= min_a && p_ba >= min_b
            })
            .collect();

        for (&n, n_elig) in &eligible {
            for &o in n_elig {
                if eligible.get(&o).is_some_and(|o_elig| o_elig.contains(&n)) {
                    partner_set.insert(StatePair::new(n, o));
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

        let mut conflict_set: BTreeSet<StatePair> = active_conflict_set;

        for &n in &active_states {
            let native = state_species.get(&n).cloned().unwrap_or_default();
            let eff_n = state_eff_pops.get(&n).copied().unwrap_or(0.0);
            let cap_n = state_caps.get(&n).copied().unwrap_or(1.0);
            let best_rival = neighbors.get(&n).and_then(|nbs| {
                nbs.iter()
                    .filter_map(|&o| {
                        let eff_o = state_eff_pops.get(&o).copied().unwrap_or(0.0);
                        if eff_o <= 1e-4 || eff_n <= eff_o {
                            return None;
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
                        let dist = self.effective_state_distance(n, o);
                        let boulding_lsg = (-0.5 * dist).exp();
                        let tension = (dr + 0.15 * dk + dp) * boulding_lsg;

                        let pair = StatePair::new(n, o);
                        // Polachek commercial peace dividend raises the threshold required to break peace with a trade partner
                        let war_threshold = if partner_set.contains(&pair) {
                            let p_no = pressure_map.get(&(n, o)).copied().unwrap_or(0.0);
                            let p_on = pressure_map.get(&(o, n)).copied().unwrap_or(0.0);
                            let denom = (native.len() + osp.len()).max(1) as f32;
                            let peace_dividend = (1.5 * (p_no + p_on) / denom).max(1.0);
                            1.0 + peace_dividend
                        } else {
                            1.0
                        };

                        (tension >= war_threshold).then_some((o, tension))
                    })
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            });
            if let Some((priority, _)) = best_rival {
                let pair = StatePair::new(n, priority);
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

    /// Geographic latitude in degrees `[-90.0, 90.0]` of fine face `f_idx`.
    pub fn fine_face_latitude_deg(&self, f_idx: usize) -> f64 {
        let z = self
            .terrain
            .terrain_grid
            .faces
            .get(f_idx)
            .map(|(c, _)| c[2])
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0);
        z.asin() * 180.0 / std::f64::consts::PI
    }

    /// Set of constellation indices `c.index` visible at some point in the year from any of `tiles`.
    pub fn visible_constellations_for_tiles(&self, tiles: &[usize]) -> BTreeSet<u32> {
        self.constellations
            .iter()
            .filter(|c| {
                tiles
                    .iter()
                    .any(|&f_idx| c.is_visible_at_latitude_deg(self.fine_face_latitude_deg(f_idx)))
            })
            .map(|c| c.index)
            .collect()
    }

    /// Set of continent IDs and adjacent ocean basin IDs for `tiles` (fine face indices).
    pub fn continents_and_oceans_for_tiles(
        &self,
        tiles: &[usize],
        coarse_elevs: &[f32],
    ) -> (BTreeSet<u32>, BTreeSet<u32>) {
        let coarse_adj = self.prehistory.adjacency();
        let mut conts = BTreeSet::new();
        let mut oceans = BTreeSet::new();
        for &f_idx in tiles {
            if f_idx >= self.terrain.parentage.len() {
                continue;
            }
            let c = self.terrain.primary_coarse_tile(f_idx, coarse_elevs);
            if let Some(cid) = self.geography.tile_continent.get(c).copied().flatten() {
                conts.insert(cid as u32);
            }
            if let Some(oid) = self.geography.tile_ocean.get(c).copied().flatten() {
                oceans.insert(oid as u32);
            }
            if let Some(nbs) = coarse_adj.get(c) {
                for &nb in nbs {
                    if let Some(oid) = self.geography.tile_ocean.get(nb).copied().flatten() {
                        oceans.insert(oid as u32);
                    }
                }
            }
        }
        (conts, oceans)
    }

    pub fn geography(&self) -> &PlanetaryGeography {
        &self.geography
    }

    /// Accumulate concepts across co-resident communities, trade partners, rivals, and recursive trade imports,
    /// borrowing missing words from source state majority languages (`historysimulation.py:664-705`).
    fn accumulate_and_borrow_concepts(
        &mut self,
        extents: &BTreeMap<u32, Vec<usize>>,
        state_species: &BTreeMap<u32, BTreeSet<usize>>,
    ) {
        let coarse_elevs = self.prehistory.tile_elevations();
        let mut state_langs: BTreeMap<u32, Option<u32>> = BTreeMap::new();
        let mut state_visible_constellations: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_continents: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_oceans: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for (&sid, tiles) in extents {
            state_langs.insert(sid, self.primary_language_for_state(sid));
            state_visible_constellations.insert(sid, self.visible_constellations_for_tiles(tiles));
            let (conts, oceans) = self.continents_and_oceans_for_tiles(tiles, &coarse_elevs);
            state_continents.insert(sid, conts);
            state_oceans.insert(sid, oceans);
        }

        let mut lang_concepts: BTreeMap<u32, BTreeMap<ConceptKey, Option<u32>>> = BTreeMap::new();
        let mut state_sources: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut lang_visible_constellations: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();

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
            let mut all_vis_for_lang = BTreeSet::new();
            for &state_id in states_for_lang {
                if let Some(vis) = state_visible_constellations.get(&state_id) {
                    all_vis_for_lang.extend(vis.iter().copied());
                }
            }
            lang_visible_constellations.insert(lang_id, all_vis_for_lang.clone());

            for &state_id in states_for_lang {
                let native = state_species.get(&state_id).cloned().unwrap_or_default();
                let vis_here = state_visible_constellations
                    .get(&state_id)
                    .cloned()
                    .unwrap_or_default();
                let rec_partners = self.recursive_trade_partners(state_id);
                let direct_partners = self.lookup_trade_partners(state_id);
                let rivals = self.lookup_opponents(state_id);

                let mut related_states = BTreeSet::new();
                related_states.insert(state_id);
                related_states.extend(direct_partners.iter().copied());
                related_states.extend(rivals);
                related_states.extend(rec_partners.iter().copied());

                for s in &related_states {
                    let s_lang = state_langs.get(s).copied().flatten();
                    concepts.insert(ConceptKey::State(*s), s_lang);
                    // Share astronomical knowledge of all planets in `self.solar_system` across trading partners & neighbors:
                    if let Some(slid) = s_lang {
                        if let Some(slang) = self.languages.get(&slid) {
                            for p in &self.solar_system {
                                let pk = ConceptKey::Planet(p.index);
                                if slang.describes(pk) {
                                    concepts.entry(pk).or_insert(Some(slid));
                                }
                            }
                        }
                    }
                }

                // Borrow names from trade partners (direct and recursive) for constellations this language cannot see:
                let mut trade_network = direct_partners;
                trade_network.extend(rec_partners.iter().copied());
                for partner in &trade_network {
                    let p_lang = state_langs.get(partner).copied().flatten();
                    if let Some(p_vis) = state_visible_constellations.get(partner) {
                        for &c_idx in p_vis {
                            if !all_vis_for_lang.contains(&c_idx) {
                                if let Some(plid) = p_lang {
                                    concepts
                                        .entry(ConceptKey::Constellation(c_idx))
                                        .or_insert(Some(plid));
                                }
                            }
                        }
                    }
                    if let Some(plid) = p_lang {
                        if let Some(plang) = self.languages.get(&plid) {
                            for c in &self.constellations {
                                if !all_vis_for_lang.contains(&c.index)
                                    && plang.describes(ConceptKey::Constellation(c.index))
                                {
                                    concepts
                                        .entry(ConceptKey::Constellation(c.index))
                                        .or_insert(Some(plid));
                                }
                            }
                        }
                    }
                }

                // Imported species and foreign continents/oceans from recursive trade network (`imports`)
                let home_conts = state_continents.get(&state_id).cloned().unwrap_or_default();
                let home_oceans = state_oceans.get(&state_id).cloned().unwrap_or_default();
                for partner in rec_partners {
                    let p_lang = state_langs.get(&partner).copied().flatten();
                    if let Some(psp) = state_species.get(&partner) {
                        for &sp in psp.difference(&native).take(6) {
                            concepts.insert(ConceptKey::Species(sp as u32), p_lang);
                        }
                    }
                    if let Some(pconts) = state_continents.get(&partner) {
                        for &cid in pconts.difference(&home_conts) {
                            concepts.entry(ConceptKey::Continent(cid)).or_insert(p_lang);
                        }
                    }
                    if let Some(poceans) = state_oceans.get(&partner) {
                        for &oid in poceans.difference(&home_oceans) {
                            concepts.entry(ConceptKey::Ocean(oid)).or_insert(p_lang);
                        }
                    }
                }

                // Native species, continents, oceans, visible constellations (`vis_here`), and core celestial concepts (`Sun`, `Moons`, `Planets`) in `state_id`
                let s_lang = state_langs.get(&state_id).copied().flatten();
                for &cid in &home_conts {
                    concepts.insert(ConceptKey::Continent(cid), s_lang);
                }
                for &oid in &home_oceans {
                    concepts.insert(ConceptKey::Ocean(oid), s_lang);
                }
                for &c_idx in &vis_here {
                    concepts.insert(ConceptKey::Constellation(c_idx), s_lang);
                }
                concepts.entry(ConceptKey::Sun(0)).or_insert(s_lang);
                for m in 0..self.moon_count {
                    concepts.entry(ConceptKey::Moon(m)).or_insert(s_lang);
                }
                for p in &self.solar_system {
                    if p.is_home_world || p.is_naked_eye_visible() {
                        concepts
                            .entry(ConceptKey::Planet(p.index))
                            .or_insert(s_lang);
                    }
                }
                for &sp in &native {
                    concepts.insert(ConceptKey::Species(sp as u32), s_lang);
                }
            }
        }

        let lang_ids: Vec<u32> = lang_concepts.keys().copied().collect();

        // Pass 1: Coin native concepts (including all constellations visible at some point in the year)
        // so that donor languages have their visible constellations coined before trade partners borrow them.
        for &lang_id in &lang_ids {
            let Some(concepts) = lang_concepts.get(&lang_id).cloned() else {
                continue;
            };
            let vis_set = lang_visible_constellations
                .get(&lang_id)
                .cloned()
                .unwrap_or_default();
            for (concept, src_lang_opt) in concepts {
                if src_lang_opt != Some(lang_id) && src_lang_opt.is_some() {
                    continue;
                }
                if let ConceptKey::Constellation(c_idx) = concept {
                    if !vis_set.contains(&c_idx) {
                        continue;
                    }
                }
                let already = self
                    .languages
                    .get(&lang_id)
                    .is_some_and(|l| l.describes(concept));
                if already {
                    continue;
                }
                let mut temp_rng = self.rng.clone();
                if let Some(lang) = self.languages.get_mut(&lang_id) {
                    lang.coin_concept(concept, &mut temp_rng);
                }
                self.rng = temp_rng;
            }
        }

        // Pass 2: Borrow concepts from donor languages (including invisible constellations from trade partners).
        for lang_id in lang_ids {
            let Some(concepts) = lang_concepts.get(&lang_id).cloned() else {
                continue;
            };
            let vis_set = lang_visible_constellations
                .get(&lang_id)
                .cloned()
                .unwrap_or_default();
            for (concept, src_lang_opt) in concepts {
                let already = self
                    .languages
                    .get(&lang_id)
                    .is_some_and(|l| l.describes(concept));
                if already {
                    continue;
                }
                let donor_origin_opt = src_lang_opt.and_then(|src_id| {
                    let d = self.languages.get(&src_id)?;
                    Some((d.id, d.lexicon.get(&concept)?.clone()))
                });
                if let Some((donor_id, donor_origin)) = donor_origin_opt {
                    if let Some(lang) = self.languages.get_mut(&lang_id) {
                        lang.borrow_concept_origin(concept, donor_id, &donor_origin);
                    }
                } else {
                    // Never coin an invisible constellation natively; it can only be borrowed from a trade partner.
                    if let ConceptKey::Constellation(c_idx) = concept {
                        if !vis_set.contains(&c_idx) {
                            continue;
                        }
                    }
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
        let mut spillover: Vec<MigrationSpillover> = Vec::new();
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
                let state_id = self.tile_states.get(&t).copied().unwrap_or(0);
                if let Some(&nb) = fine_adj[t].iter().find(|&&nb| {
                    let c = self.terrain.primary_coarse_tile(nb, &coarse_elevs);
                    let nb_pop = tile_pops.get(&nb).copied().unwrap_or(0.0);
                    fine_elevs[nb] > 0.0
                        && is_passable(fine_elevs[nb], koeppen[c], true)
                        && self.capacities[nb] > 0.1
                        && nb_pop < self.capacities[nb] * 0.9
                }) {
                    let mut migrant = comms[0].clone();
                    migrant.thousands = (comms[0].thousands * 0.08).max(0.02);
                    comms[0].thousands = (comms[0].thousands - migrant.thousands).max(0.02);
                    spillover.push(MigrationSpillover {
                        target_face: nb,
                        state_id,
                        community: migrant,
                    });
                }
            }
        }

        for item in spillover {
            self.tile_states
                .entry(item.target_face)
                .or_insert(item.state_id);
            self.tile_communities
                .entry(item.target_face)
                .or_default()
                .push(item.community);
        }

        // 2b. Maritime & riverine expansion along established wind/current travel routes (`self.travel_routes`)
        let maritime_spillover: Vec<MigrationSpillover> = self
            .travel_routes
            .iter()
            .filter_map(|route| {
                let src = route.from_fine_face;
                let dst = route.to_fine_face;
                let &state_id = self.tile_states.get(&src)?;
                let src_comms = self.tile_communities.get(&src)?;
                let src_pop: f32 = src_comms.iter().map(|c| c.thousands).sum();
                let src_cap = self.capacities[src].max(0.2);
                if src_pop <= src_cap * 0.55 || src_comms.is_empty() {
                    return None;
                }
                let c_dst = self.terrain.primary_coarse_tile(dst, &coarse_elevs);
                if fine_elevs[dst] <= 0.0
                    || !is_passable(fine_elevs[dst], koeppen[c_dst], true)
                    || self.capacities[dst] <= 0.1
                {
                    return None;
                }
                let dst_pop: f32 = self
                    .tile_communities
                    .get(&dst)
                    .map(|cs| cs.iter().map(|c| c.thousands).sum())
                    .unwrap_or(0.0);
                if dst_pop >= self.capacities[dst] * 0.85 {
                    return None;
                }
                let mut migrant = src_comms[0].clone();
                let frac = (0.06 * route.wind_current_alignment.max(0.35)).clamp(0.02, 0.08);
                migrant.thousands = (src_comms[0].thousands * frac).max(0.02);
                Some(MigrationSpillover {
                    target_face: dst,
                    state_id,
                    community: migrant,
                })
            })
            .collect();
        for item in maritime_spillover {
            self.tile_states
                .entry(item.target_face)
                .or_insert(item.state_id);
            self.tile_communities
                .entry(item.target_face)
                .or_default()
                .push(item.community);
        }
        self.update_travel_routes();

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
        let StateTopology { extents, .. } = self.state_topology(&fine_adj);
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

        let mut results: Vec<WarOutcome> = Vec::new();
        let mut border_transfers: Vec<WarOutcome> = Vec::new();
        for &StatePair { a: n, b: o } in &self.conflicts {
            let pn = state_eff_pops.get(&n).copied().unwrap_or(0.0);
            let po = state_eff_pops.get(&o).copied().unwrap_or(0.0);
            let opp_total_n = opp_pop_totals.get(&n).copied().unwrap_or(1.0).max(0.1);
            let opp_total_o = opp_pop_totals.get(&o).copied().unwrap_or(1.0).max(0.1);
            let weight_n_to_o = po.max(0.1) / opp_total_n;
            let weight_o_to_n = pn.max(0.1) / opp_total_o;
            let dist_no = self.effective_state_distance(n, o);
            let lsg = (-0.5 * dist_no).exp();
            let fn_val = pn * weight_n_to_o * lsg;
            let fo_val = po * weight_o_to_n * lsg;

            if fo_val > 0.0 && fn_val / fo_val > 1.5 {
                results.push(WarOutcome {
                    winner: n,
                    loser: o,
                });
            } else if fn_val > 0.0 && fo_val / fn_val > 1.5 {
                results.push(WarOutcome {
                    winner: o,
                    loser: n,
                });
            } else if fo_val > 0.0 && fn_val / fo_val > 1.15 {
                border_transfers.push(WarOutcome {
                    winner: n,
                    loser: o,
                });
            } else if fn_val > 0.0 && fo_val / fn_val > 1.15 {
                border_transfers.push(WarOutcome {
                    winner: o,
                    loser: n,
                });
            }
        }
        // All active conflicts resolve this tick (decisive annexation, marginal border transfer, or stalemate truce)
        self.conflicts.clear();

        let losers: BTreeSet<u32> = results.iter().map(|out| out.loser).collect();
        let winners: BTreeSet<u32> = results.iter().map(|out| out.winner).collect();
        let mut conquests: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &WarOutcome { winner, loser } in &results {
            if !losers.contains(&winner) {
                conquests.entry(winner).or_default().insert(loser);
                if let Some(loser_tiles) = extents.get(&loser) {
                    for &t in loser_tiles {
                        self.tile_states.insert(t, winner);
                    }
                }
            }
        }
        for &WarOutcome { winner, loser } in &border_transfers {
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
                    // If the conflict was across a maritime/riverine travel route, capture the landing port & adjacent coastal faces
                    if border_faces.is_empty() {
                        if let Some(port_f) = self.travel_routes.iter().find_map(|r| {
                            let from_st = self.tile_states.get(&r.from_fine_face).copied();
                            let to_st = self.tile_states.get(&r.to_fine_face).copied();
                            if from_st == Some(winner) && to_st == Some(loser) {
                                Some(r.to_fine_face)
                            } else if to_st == Some(winner) && from_st == Some(loser) {
                                Some(r.from_fine_face)
                            } else {
                                None
                            }
                        }) {
                            border_faces.push(port_f);
                            border_faces.extend(
                                fine_adj[port_f]
                                    .iter()
                                    .copied()
                                    .filter(|&nb| self.tile_states.get(&nb) == Some(&loser)),
                            );
                        }
                    }
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
            self.trade_partners.retain(|pair| !pair.contains(n));
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

    pub fn build_display_mesh(&self) -> DisplayMesh {
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

    pub fn solar_system(&self) -> &[SolarSystemPlanet] {
        &self.solar_system
    }

    pub fn set_solar_system(&mut self, solar_system: Vec<SolarSystemPlanet>) {
        self.solar_system = solar_system;
    }

    pub fn constellations(&self) -> &[Constellation] {
        &self.constellations
    }

    pub fn set_constellations(&mut self, constellations: Vec<Constellation>) {
        self.constellations = constellations;
        let fine_adj = self.terrain.terrain_adj.clone();
        let StateTopology {
            extents,
            species: state_species,
            ..
        } = self.state_topology(&fine_adj);
        self.accumulate_and_borrow_concepts(&extents, &state_species);
    }

    pub fn travel_routes(&self) -> &[TravelRoute] {
        &self.travel_routes
    }

    pub fn potential_travel_routes(&self) -> &[TravelRoute] {
        &self.potential_travel_routes
    }

    pub fn active_travel_route_count(&self) -> usize {
        self.travel_routes.len()
    }

    pub fn potential_travel_route_count(&self) -> usize {
        self.potential_travel_routes.len()
    }

    pub fn water_access_port_count(&self) -> usize {
        let ports: BTreeSet<usize> = self
            .travel_routes
            .iter()
            .flat_map(|r| [r.from_fine_face, r.to_fine_face])
            .collect();
        ports.len()
    }

    /// Classify the diplomatic/historical usage of a `TravelRoute`.
    pub fn route_status(&self, route: &TravelRoute, is_active: bool) -> TravelRouteStatus {
        if !is_active {
            return TravelRouteStatus::PotentialCorridor;
        }
        if let (Some(&s1), Some(&s2)) = (
            self.tile_states.get(&route.from_fine_face),
            self.tile_states.get(&route.to_fine_face),
        ) {
            if s1 != s2 {
                let pair = StatePair::new(s1, s2);
                if self.conflicts.contains(&pair) {
                    return TravelRouteStatus::NavalConflict;
                }
                if self.trade_partners.contains(&pair) {
                    return TravelRouteStatus::MaritimeTrade;
                }
            }
        }
        TravelRouteStatus::ActiveSeaLane
    }

    pub fn maritime_trade_route_count(&self) -> usize {
        self.travel_routes
            .iter()
            .filter(|r| self.route_status(r, true) == TravelRouteStatus::MaritimeTrade)
            .count()
    }

    pub fn maritime_conflict_route_count(&self) -> usize {
        self.travel_routes
            .iter()
            .filter(|r| self.route_status(r, true) == TravelRouteStatus::NavalConflict)
            .count()
    }

    /// Packed colored 3D line segments `[x0, y0, z0, x1, y1, z1, r, g, b, ...]` (`9` `f32`s per segment)
    /// for rendering established `travel_routes` (and optionally `potential_travel_routes`) on the globe,
    /// including directional chevron arrowheads at the midpoint of each route showing wind/current flow.
    pub fn travel_route_segments_colored(&self, include_potential: bool) -> Vec<f32> {
        let grid = self.prehistory.grid();
        let active_pairs: BTreeSet<(usize, usize)> = self
            .travel_routes
            .iter()
            .map(|r| (r.from_coarse_tile, r.to_coarse_tile))
            .collect();

        let mut out = Vec::new();
        let mut append_route = |route: &TravelRoute, is_active: bool| {
            let status = self.route_status(route, is_active);
            let rgb = status.rgb(route.route_kind);
            let radius_lift = status.radius_lift();

            let lift_pt = |c: Vec3| -> [f32; 3] {
                [
                    c[0] as f32 * radius_lift,
                    c[1] as f32 * radius_lift,
                    c[2] as f32 * radius_lift,
                ]
            };

            let pts: Vec<[f32; 3]> = self
                .terrain
                .terrain_grid
                .faces
                .get(route.from_fine_face)
                .map(|&(c, _)| lift_pt(c))
                .into_iter()
                .chain(
                    route
                        .coarse_path
                        .iter()
                        .filter_map(|&c_idx| grid.faces.get(c_idx).map(|&(c, _)| lift_pt(c))),
                )
                .chain(
                    self.terrain
                        .terrain_grid
                        .faces
                        .get(route.to_fine_face)
                        .map(|&(c, _)| lift_pt(c)),
                )
                .collect();

            if pts.len() < 2 {
                return;
            }
            for pair in pts.windows(2) {
                let p0 = pair[0];
                let p1 = pair[1];
                out.extend_from_slice(&[
                    p0[0], p0[1], p0[2], p1[0], p1[1], p1[2], rgb[0], rgb[1], rgb[2],
                ]);
            }

            // Add directional arrowhead V-chevron at the midpoint segment
            let mid = (pts.len() - 1) / 2;
            let p0 = pts[mid];
            let p1 = pts[mid + 1];
            let mx = 0.5 * (p0[0] + p1[0]);
            let my = 0.5 * (p0[1] + p1[1]);
            let mz = 0.5 * (p0[2] + p1[2]);
            let dx = p1[0] - p0[0];
            let dy = p1[1] - p0[1];
            let dz = p1[2] - p0[2];
            let dlen = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
            let tx = dx / dlen;
            let ty = dy / dlen;
            let tz = dz / dlen;
            // Tangent perpendicular on the sphere: n x t
            let sx = my * tz - mz * ty;
            let sy = mz * tx - mx * tz;
            let sz = mx * ty - my * tx;
            let slen = (sx * sx + sy * sy + sz * sz).sqrt().max(1e-6);
            let ux = sx / slen;
            let uy = sy / slen;
            let uz = sz / slen;
            let scale = 0.016f32;
            let w1 = [
                mx - scale * tx + 0.65 * scale * ux,
                my - scale * ty + 0.65 * scale * uy,
                mz - scale * tz + 0.65 * scale * uz,
            ];
            let w2 = [
                mx - scale * tx - 0.65 * scale * ux,
                my - scale * ty - 0.65 * scale * uy,
                mz - scale * tz - 0.65 * scale * uz,
            ];
            out.extend_from_slice(&[
                mx, my, mz, w1[0], w1[1], w1[2], rgb[0], rgb[1], rgb[2],
                mx, my, mz, w2[0], w2[1], w2[2], rgb[0], rgb[1], rgb[2],
            ]);
        };

        if include_potential {
            for r in &self.potential_travel_routes {
                if !active_pairs.contains(&(r.from_coarse_tile, r.to_coarse_tile)) {
                    append_route(r, false);
                }
            }
        }
        for r in &self.travel_routes {
            append_route(r, true);
        }
        out
    }

    /// Packed colored 3D line segments `[x0, y0, z0, x1, y1, z1, r, g, b, ...]` (`9` `f32`s per segment)
    /// showing surface ocean current vectors and prevailing wind vectors across the globe.
    pub fn wind_current_vector_segments(&self) -> Vec<f32> {
        let grid = self.prehistory.grid();
        let climate = self.prehistory.climate_model();
        let ocean = climate.ocean();
        let coarse_elevs = self.prehistory.tile_elevations();
        let num_coarse = coarse_elevs.len();

        let mut out = Vec::new();
        let stride = (num_coarse / 450).max(1);
        for c in (0..num_coarse).step_by(stride) {
            let p = grid.faces[c].0;
            let r_lift = 1.004f32;
            let p0 = [
                p[0] as f32 * r_lift,
                p[1] as f32 * r_lift,
                p[2] as f32 * r_lift,
            ];

            if coarse_elevs[c] <= 0.0 {
                // Ocean current vector (warm = orange-gold, cold = cyan-blue)
                let vel = ocean.velocity_xyz.get(c).copied().unwrap_or([0.0; 3]);
                let spd = ocean.speed.get(c).copied().unwrap_or(0.0);
                let sst = ocean.sst_anomaly.get(c).copied().unwrap_or(0.0);
                if spd > 0.04 {
                    let len = 0.032 * spd.sqrt().clamp(0.35, 1.0);
                    let p1 = [
                        p0[0] + len * vel[0],
                        p0[1] + len * vel[1],
                        p0[2] + len * vel[2],
                    ];
                    let rgb = if sst >= 0.05 {
                        [0.98, 0.58, 0.20]
                    } else if sst <= -0.05 {
                        [0.25, 0.78, 0.98]
                    } else {
                        [0.45, 0.85, 0.78]
                    };
                    out.extend_from_slice(&[
                        p0[0], p0[1], p0[2], p1[0], p1[1], p1[2], rgb[0], rgb[1], rgb[2],
                    ]);
                }
            }
            // Prevailing wind vector (every 2nd sampled tile, subtle sky-white)
            if (c / stride) % 2 == 0 {
                let w = climate.wind_vector_xyz(c);
                let len = 0.022f32;
                let p1 = [p0[0] + len * w[0], p0[1] + len * w[1], p0[2] + len * w[2]];
                out.extend_from_slice(&[
                    p0[0], p0[1], p0[2], p1[0], p1[1], p1[2], 0.78, 0.86, 0.96,
                ]);
            }
        }
        out
    }

    /// Detailed multi-line inspection report for the Travel mode (or `"travel"` / `"winds_currents"` aspects).
    pub fn inspect_travel_tile(&self, display_idx: usize) -> String {
        let Some(face) = self.terrain.display_faces.get(display_idx) else {
            return String::new();
        };
        let climate = self.prehistory.climate_model();
        let ocean = climate.ocean();
        let coarse_elevs = self.prehistory.tile_elevations();

        match face.source {
            DisplayFaceSource::OceanCoarse(c_idx) => {
                let lat = climate.lats_deg().get(c_idx).copied().unwrap_or(0.0);
                let lon = climate.lons_deg().get(c_idx).copied().unwrap_or(0.0);
                let wind_deg =
                    tec11_climate::wind_direction_deg(lat, climate.cells(), climate.spin());
                let spd = ocean.speed.get(c_idx).copied().unwrap_or(0.0);
                let sst = ocean.sst_anomaly.get(c_idx).copied().unwrap_or(0.0);
                let sst_desc = if sst >= 0.08 {
                    "Warm Boundary Current"
                } else if sst <= -0.08 {
                    "Cold Upwelling / Return Current"
                } else {
                    "Temperate Drift"
                };

                let mut lines = vec![format!(
                    "Ocean Tile #{c_idx} ({lat:+.1}°, {lon:+.1}°) | Wind Bearing: {wind_deg:.0}° | Current Speed: {:.0}% ({sst_desc}, SST {sst:+.2})",
                    spd * 100.0
                )];

                // Find any travel routes passing through or within 1 hop of `c_idx`
                let adj = self.prehistory.adjacency();
                let nearby_coarse: BTreeSet<usize> = std::iter::once(c_idx)
                    .chain(adj.get(c_idx).into_iter().flatten().copied())
                    .collect();

                let active_matches: Vec<(&TravelRoute, bool)> = self
                    .travel_routes
                    .iter()
                    .filter(|r| r.coarse_path.iter().any(|c| nearby_coarse.contains(c)))
                    .map(|r| (r, true))
                    .collect();
                let matching_routes: Vec<(&TravelRoute, bool)> = if !active_matches.is_empty() {
                    active_matches
                } else {
                    self.potential_travel_routes
                        .iter()
                        .filter(|r| r.coarse_path.iter().any(|c| nearby_coarse.contains(c)))
                        .map(|r| (r, false))
                        .collect()
                };

                if matching_routes.is_empty() {
                    lines.push(
                        "No established travel routes pass directly through this ocean sector."
                            .to_string(),
                    );
                } else {
                    lines.push(format!(
                        "--- Travel Routes in Sector ({}) ---",
                        matching_routes.len()
                    ));
                    for (r, is_active) in matching_routes.into_iter().take(8) {
                        let from_label = self
                            .tile_states
                            .get(&r.from_fine_face)
                            .map(|&s| self.state_name(s))
                            .unwrap_or_else(|| format!("Coast #{}", r.from_coarse_tile));
                        let to_label = self
                            .tile_states
                            .get(&r.to_fine_face)
                            .map(|&s| self.state_name(s))
                            .unwrap_or_else(|| format!("Coast #{}", r.to_coarse_tile));
                        let status_str = self.route_status(r, is_active).ocean_sector_label();
                        lines.push(format!(
                            "• {from_label} → {to_label} [{status_str}] | {} | Wind/Current Align: {:.0}% | Cost: {:.2}",
                            r.kind_name(),
                            r.wind_current_alignment * 100.0,
                            r.effective_cost
                        ));
                    }
                }
                lines.join("\n")
            }
            DisplayFaceSource::Fine(fine_idx) => {
                let c_idx = self.terrain.primary_coarse_tile(fine_idx, &coarse_elevs);
                let lat = climate.lats_deg().get(c_idx).copied().unwrap_or(0.0);
                let wind_deg =
                    tec11_climate::wind_direction_deg(lat, climate.cells(), climate.spin());
                let mut lines = Vec::new();

                if let Some(&sid) = self.tile_states.get(&fine_idx) {
                    let st_name = self.state_name(sid);
                    let st_pop = self.state_population_thousands(sid);
                    lines.push(format!(
                        "State: {st_name} | Pop: {:.0}K | Fine Face #{fine_idx} (Coarse #{c_idx}, {lat:+.1}°, Prevailing Wind {wind_deg:.0}°)",
                        st_pop
                    ));
                    let state_routes: Vec<&TravelRoute> = self
                        .travel_routes
                        .iter()
                        .filter(|r| {
                            self.tile_states.get(&r.from_fine_face) == Some(&sid)
                                || self.tile_states.get(&r.to_fine_face) == Some(&sid)
                        })
                        .collect();
                    if state_routes.is_empty() {
                        lines.push(
                            "Water Travel Routes: None (inland or no active coastal ports)"
                                .to_string(),
                        );
                    } else {
                        lines.push(format!(
                            "--- Active Water Travel Routes for {st_name} ({}) ---",
                            state_routes.len()
                        ));
                        for r in state_routes.into_iter().take(12) {
                            let from_label = self
                                .tile_states
                                .get(&r.from_fine_face)
                                .map(|&s| self.state_name(s))
                                .unwrap_or_else(|| {
                                    format!("Unclaimed Coast #{}", r.from_coarse_tile)
                                });
                            let to_label = self
                                .tile_states
                                .get(&r.to_fine_face)
                                .map(|&s| self.state_name(s))
                                .unwrap_or_else(|| {
                                    format!("Unclaimed Coast #{}", r.to_coarse_tile)
                                });
                            let status_str = self.route_status(r, true).land_label();
                            lines.push(format!(
                                "• {from_label} → {to_label} [{status_str}] ({}, Align {:.0}%, Cost {:.2})",
                                r.kind_name(),
                                r.wind_current_alignment * 100.0,
                                r.effective_cost
                            ));
                        }
                    }
                } else {
                    lines.push(format!(
                        "Unclaimed Land Face #{fine_idx} (Coarse #{c_idx}, {lat:+.1}°, Prevailing Wind {wind_deg:.0}°)"
                    ));
                    let pot: Vec<&TravelRoute> = self
                        .potential_travel_routes
                        .iter()
                        .filter(|r| r.from_coarse_tile == c_idx || r.to_coarse_tile == c_idx)
                        .collect();
                    if pot.is_empty() {
                        lines.push("No potential wind/current travel corridors originate or land on this tile.".to_string());
                    } else {
                        lines.push(format!(
                            "--- Potential Wind/Current Corridors ({}) ---",
                            pot.len()
                        ));
                        for r in pot.into_iter().take(8) {
                            lines.push(format!(
                                "• Coast #{} → Coast #{} ({}, Wind/Current Align {:.0}%, Cost {:.2})",
                                r.from_coarse_tile,
                                r.to_coarse_tile,
                                r.kind_name(),
                                r.wind_current_alignment * 100.0,
                                r.effective_cost
                            ));
                        }
                    }
                }
                lines.join("\n")
            }
        }
    }

    /// Detailed multi-line inspection report for a clicked display face and its State (`historypresenter.py`).
    pub fn inspect_tile(&self, display_idx: usize) -> String {
        let Some(face) = self.terrain.display_faces.get(display_idx) else {
            return String::new();
        };
        let fine_idx = match face.source {
            DisplayFaceSource::OceanCoarse(c_idx) => {
                let ocean_label = self.geography.ocean_name(c_idx).unwrap_or("World Ocean");
                return format!("Ocean Tile #{c_idx} ({ocean_label})");
            }
            DisplayFaceSource::Fine(f) => f,
        };
        let fine_elev = self.terrain.fine_elevations[fine_idx];
        let coarse_elevs = self.prehistory.tile_elevations();
        let primary_c = self.terrain.primary_coarse_tile(fine_idx, &coarse_elevs);
        if fine_elev <= 0.0 {
            let ocean_label = self
                .geography
                .ocean_name(primary_c)
                .unwrap_or("Coastal Sea");
            return format!("Coastal Bay / Sea Face #{fine_idx} ({ocean_label}, {fine_elev:.2} km)");
        }

        let cont_name = self
            .geography
            .continent_name(primary_c)
            .unwrap_or("Mainland");
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
                "Continent: {cont_name} | Fine Face #{fine_idx} (Unclaimed Land, {fine_elev:.2} km) | Pop: {:.1}K / Cap: {:.1}K",
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
            "State: {st_name} | Continent: {cont_name} | Pop: {:.0}K | Asabiya: {:.0}% ({frontier_label}) | Fine Face #{fine_idx} ({fine_elev:.2} km, {:.1}K / {:.1}K)",
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
            .lookup_trade_partners(sid)
            .into_iter()
            .map(|p| self.state_name(p))
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
            .lookup_opponents(sid)
            .into_iter()
            .map(|r| self.state_name(r))
            .collect();
        lines.push(format!(
            "Conflicts (red): {}",
            if rivals.is_empty() {
                "None".to_string()
            } else {
                rivals.join(", ")
            }
        ));

        // Active maritime/riverine travel routes for this state
        let mut route_summaries: Vec<String> = self
            .travel_routes
            .iter()
            .filter_map(|r| {
                let s_from = self.tile_states.get(&r.from_fine_face).copied();
                let s_to = self.tile_states.get(&r.to_fine_face).copied();
                if s_from == Some(sid) {
                    let dst_label = s_to
                        .map(|s| self.state_name(s))
                        .unwrap_or_else(|| format!("Coast #{}", r.to_coarse_tile));
                    Some(format!("→ {dst_label} ({})", r.kind_name()))
                } else if s_to == Some(sid) {
                    let src_label = s_from
                        .map(|s| self.state_name(s))
                        .unwrap_or_else(|| format!("Coast #{}", r.from_coarse_tile));
                    Some(format!("← {src_label} ({})", r.kind_name()))
                } else {
                    None
                }
            })
            .collect();
        route_summaries.dedup();
        lines.push(format!(
            "Water Travel Routes ({}): {}",
            route_summaries.len(),
            if route_summaries.is_empty() {
                "None".to_string()
            } else {
                route_summaries
                    .into_iter()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));

        // Sample lexicon from primary language (including celestial Sun, Moons, Planets & Constellations)
        if let Some(lid) = dom_lang {
            if let Some(lang) = self.languages.get(&lid) {
                lines.push(format!("--- {} Lexicon ---", lang.name()));
                for (&ck, orig) in lang.lexicon.iter().take(28) {
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
                        ConceptKey::Planet(p) => self
                            .solar_system
                            .get(p as usize)
                            .map(|pl| pl.lexicon_gloss())
                            .unwrap_or_else(|| format!("planet #{}", p + 1)),
                        ConceptKey::Constellation(c) => self
                            .constellations
                            .get(c as usize)
                            .map(|con| con.lexicon_gloss())
                            .unwrap_or_else(|| format!("constellation #{}", c + 1)),
                        ConceptKey::Continent(cid) => self
                            .geography
                            .continents
                            .get(cid as usize)
                            .map(|c| format!("continent ({})", c.name))
                            .unwrap_or_else(|| format!("continent #{}", cid + 1)),
                        ConceptKey::Ocean(oid) => self
                            .geography
                            .oceans
                            .get(oid as usize)
                            .map(|o| format!("ocean ({})", o.name))
                            .unwrap_or_else(|| format!("ocean #{}", oid + 1)),
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
            match self.terrain.display_faces.get(d_idx)?.source {
                DisplayFaceSource::Fine(f) => self.tile_states.get(&f).copied(),
                DisplayFaceSource::OceanCoarse(_) => None,
            }
        });
        let (partner_states, rival_states) = if let Some(sel_sid) = selected_state {
            (
                self.lookup_trade_partners(sel_sid),
                self.lookup_opponents(sel_sid),
            )
        } else {
            (BTreeSet::new(), BTreeSet::new())
        };

        let max_cap = self.capacities.iter().copied().fold(1.0f32, f32::max);
        let max_sp = self
            .tile_species
            .iter()
            .map(|v| v.len())
            .max()
            .unwrap_or(1)
            .max(1) as f32;

        let active_ports: BTreeSet<usize> = if aspect == "travel" || aspect == "winds_currents" {
            self.travel_routes
                .iter()
                .flat_map(|r| [r.from_fine_face, r.to_fine_face])
                .collect()
        } else {
            BTreeSet::new()
        };
        let ocean = self.prehistory.climate_model().ocean();

        let mut out = Vec::with_capacity(num_display * 3);
        for face in &self.terrain.display_faces {
            let f_idx = match face.source {
                DisplayFaceSource::OceanCoarse(c_idx) => {
                    if aspect == "winds_currents" || aspect == "travel" {
                        let spd = ocean.speed.get(c_idx).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                        let sst = ocean.sst_anomaly.get(c_idx).copied().unwrap_or(0.0);
                        let (r, g, b) = if aspect == "winds_currents" {
                            if sst >= 0.0 {
                                let warm = (sst / 6.0).clamp(0.0, 1.0);
                                (
                                    (ocean_rgb[0] + 0.55 * spd + 0.25 * warm).clamp(0.0, 1.0),
                                    (ocean_rgb[1] + 0.35 * spd + 0.10 * warm).clamp(0.0, 1.0),
                                    (ocean_rgb[2] + 0.15 * spd).clamp(0.0, 1.0),
                                )
                            } else {
                                let cold = (-sst / 6.0).clamp(0.0, 1.0);
                                (
                                    (ocean_rgb[0] + 0.08 * spd).clamp(0.0, 1.0),
                                    (ocean_rgb[1] + 0.45 * spd + 0.18 * cold).clamp(0.0, 1.0),
                                    (ocean_rgb[2] + 0.55 * spd + 0.28 * cold).clamp(0.0, 1.0),
                                )
                            }
                        } else {
                            // Subtle gyre shading in "travel" mode so route lines stand out clearly
                            (
                                (ocean_rgb[0] + 0.16 * spd).clamp(0.0, 1.0),
                                (ocean_rgb[1] + 0.22 * spd).clamp(0.0, 1.0),
                                (ocean_rgb[2] + 0.28 * spd).clamp(0.0, 1.0),
                            )
                        };
                        out.extend_from_slice(&[r, g, b]);
                    } else {
                        out.extend_from_slice(&ocean_rgb);
                    }
                    continue;
                }
                DisplayFaceSource::Fine(f) => f,
            };

            if self.terrain.fine_elevations[f_idx] <= 0.0 {
                if aspect == "winds_currents" || aspect == "travel" {
                    let c_idx = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                    let spd = ocean.speed.get(c_idx).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    out.extend_from_slice(&[
                        (ocean_rgb[0] + 0.16 * spd).clamp(0.0, 1.0),
                        (ocean_rgb[1] + 0.22 * spd).clamp(0.0, 1.0),
                        (ocean_rgb[2] + 0.28 * spd).clamp(0.0, 1.0),
                    ]);
                } else {
                    out.extend_from_slice(&ocean_rgb);
                }
                continue;
            }
            if active_ports.contains(&f_idx) {
                out.extend_from_slice(&[1.0, 0.92, 0.25]);
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
            .copied()
            .map(StatePair::to_proto)
            .collect();
        let conflicts = self
            .conflicts
            .iter()
            .copied()
            .map(StatePair::to_proto)
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
                    .map(StatePair::from_proto)
                    .collect();
                sim.conflicts = hist.conflicts.iter().map(StatePair::from_proto).collect();
                sim.update_travel_routes();
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
            ConceptKey::Constellation(0),
            ConceptKey::Constellation(7),
        ] {
            let p = ck.to_proto();
            assert_eq!(ConceptKey::from_proto(&p), ck);
        }
    }

    #[test]
    fn test_constellation_visibility_and_trade_partner_borrowing() {
        let mut sim = HistorySimulation::new_earth(42);
        assert!(
            sim.constellations.len() >= 12,
            "Expected at least 12 constellations, got {}",
            sim.constellations.len()
        );

        let fine_adj = sim.terrain.terrain_adj.clone();
        let StateTopology {
            extents,
            species: state_species,
            ..
        } = sim.state_topology(&fine_adj);

        // Verify that every state's language natively describes all constellations visible
        // from its tiles at some point in the year.
        for (&sid, tiles) in &extents {
            let vis = sim.visible_constellations_for_tiles(tiles);
            let Some(lid) = sim.primary_language_for_state(sid) else {
                continue;
            };
            let Some(lang) = sim.languages.get(&lid) else {
                continue;
            };
            for c in &sim.constellations {
                let ck = ConceptKey::Constellation(c.index);
                if vis.contains(&c.index) {
                    assert!(
                        lang.describes(ck),
                        "State {sid} language should describe visible constellation {}",
                        c.index
                    );
                }
            }
        }

        // Find two states in different hemispheres (one northern, one southern) so each sees
        // constellations that the other cannot see, then link them as trade partners and verify
        // that accumulate_and_borrow_concepts borrows the unseen constellations from the trade partner.
        let north_sid = extents.iter().find_map(|(&sid, tiles)| {
            let vis = sim.visible_constellations_for_tiles(tiles);
            let sees_north_pole = sim
                .constellations
                .iter()
                .any(|c| c.declination_deg > 60.0 && vis.contains(&c.index));
            let misses_south_pole = sim
                .constellations
                .iter()
                .any(|c| c.declination_deg < -60.0 && !vis.contains(&c.index));
            (sees_north_pole && misses_south_pole).then_some(sid)
        });
        let south_sid = extents.iter().find_map(|(&sid, tiles)| {
            let vis = sim.visible_constellations_for_tiles(tiles);
            let sees_south_pole = sim
                .constellations
                .iter()
                .any(|c| c.declination_deg < -60.0 && vis.contains(&c.index));
            let misses_north_pole = sim
                .constellations
                .iter()
                .any(|c| c.declination_deg > 60.0 && !vis.contains(&c.index));
            (sees_south_pole && misses_north_pole).then_some(sid)
        });

        if let (Some(n_id), Some(s_id)) = (north_sid, south_sid) {
            let n_vis = sim.visible_constellations_for_tiles(&extents[&n_id]);
            let s_vis = sim.visible_constellations_for_tiles(&extents[&s_id]);
            let unseen_by_north: Vec<u32> = s_vis.difference(&n_vis).copied().collect();
            assert!(
                !unseen_by_north.is_empty(),
                "Northern state should miss at least one constellation seen by southern state"
            );

            // Connect them as trade partners and run accumulate_and_borrow_concepts
            sim.trade_partners.push(StatePair::new(n_id, s_id));
            sim.accumulate_and_borrow_concepts(&extents, &state_species);

            let n_lang_id = sim
                .primary_language_for_state(n_id)
                .expect("Northern state should have a primary language");
            let n_lang = &sim.languages[&n_lang_id];
            for c_idx in unseen_by_north {
                let ck = ConceptKey::Constellation(c_idx);
                assert!(
                    n_lang.describes(ck),
                    "Northern state language should borrow unseen constellation {c_idx} from southern trade partner"
                );
                let borrowed_origin = n_lang
                    .lexicon
                    .get(&ck)
                    .expect("Lexicon entry for borrowed constellation should exist");
                assert!(
                    borrowed_origin.parent.is_some(),
                    "Word for unseen constellation {c_idx} should be a loanword (parent.is_some())"
                );
            }
        }
    }

    #[test]
    fn test_travel_routes_winds_currents_and_maritime_diplomacy() {
        let mut sim = HistorySimulation::new_earth(42);
        assert!(
            sim.potential_travel_route_count() > 0,
            "Expected potential wind/current/river travel routes on Earth"
        );
        assert!(
            sim.water_access_port_count() > 0,
            "Expected water-access port coarse tiles on Earth"
        );
        assert!(
            sim.active_travel_route_count() > 0,
            "Expected populated coastal/river communities to establish active travel routes"
        );

        let route_segs = sim.travel_route_segments_colored(true);
        assert!(
            !route_segs.is_empty() && route_segs.len() % 9 == 0,
            "Travel route segments should be non-empty multiples of 9 floats"
        );

        let vec_segs = sim.wind_current_vector_segments();
        assert!(
            !vec_segs.is_empty() && vec_segs.len() % 9 == 0,
            "Wind & ocean current vector segments should be non-empty multiples of 9 floats"
        );

        let travel_colors = sim.tile_colors_rgb("travel", true, None);
        assert_eq!(travel_colors.len(), sim.tile_count() * 3);

        sim.step();
        assert!(sim.active_travel_route_count() > 0);
    }
}
