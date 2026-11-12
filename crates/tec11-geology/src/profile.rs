use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// Profiling categories covering both top-level simulation stages and inner subroutines.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilePhase {
    // Top-level stages
    StageWorldInit = 0,
    StageTectonics = 1,
    StageToPrehistory = 2,
    StagePrehistory = 3,
    StageToHistory = 4,
    StageHistory = 5,
    StageSnapshotEncode = 6,

    // Tectonics subroutines
    TecPlateAdvectionAndSubduction = 7,
    TecRidgeAndOceanBathymetry = 8,
    TecClimateAndOceanCirculation = 9,
    TecErosionAndDeposition = 10,
    TecMetamorphismAndRifting = 11,

    // Prehistory subroutines
    PreGlaciationClimate = 12,
    PreRiversAndTechSparks = 13,
    PrePopulationExpansion = 14,
    PreRacination = 15,

    // History initialization subroutines
    HistInitPrehistoryCopy = 16,
    HistInitSubdivideAndKdTree = 17,
    HistInitBiosphereAndSpecies = 18,
    HistInitRiversAndCapacities = 19,
    HistInitSeedStatesAndLexicons = 20,
    HistInitTravelRoutes = 21,
    HistInitDiplomacyAndTrade = 22,

    // History step subroutines
    HistStepGridClones = 23,
    HistStepLanguageSoundChanges = 24,
    HistStepGrowthSpilloverAndTech = 25,
    HistStepAssimilationAndDialects = 26,
    HistStepWarfareConquestAndSplits = 27,
    HistStepTradeAndConflicts = 28,
    HistStepLexiconConcepts = 29,
}

impl ProfilePhase {
    pub const COUNT: usize = 30;

    pub const ALL: [ProfilePhase; Self::COUNT] = [
        Self::StageWorldInit,
        Self::StageTectonics,
        Self::StageToPrehistory,
        Self::StagePrehistory,
        Self::StageToHistory,
        Self::StageHistory,
        Self::StageSnapshotEncode,
        Self::TecPlateAdvectionAndSubduction,
        Self::TecRidgeAndOceanBathymetry,
        Self::TecClimateAndOceanCirculation,
        Self::TecErosionAndDeposition,
        Self::TecMetamorphismAndRifting,
        Self::PreGlaciationClimate,
        Self::PreRiversAndTechSparks,
        Self::PrePopulationExpansion,
        Self::PreRacination,
        Self::HistInitPrehistoryCopy,
        Self::HistInitSubdivideAndKdTree,
        Self::HistInitBiosphereAndSpecies,
        Self::HistInitRiversAndCapacities,
        Self::HistInitSeedStatesAndLexicons,
        Self::HistInitTravelRoutes,
        Self::HistInitDiplomacyAndTrade,
        Self::HistStepGridClones,
        Self::HistStepLanguageSoundChanges,
        Self::HistStepGrowthSpilloverAndTech,
        Self::HistStepAssimilationAndDialects,
        Self::HistStepWarfareConquestAndSplits,
        Self::HistStepTradeAndConflicts,
        Self::HistStepLexiconConcepts,
    ];

    pub fn is_top_level_stage(self) -> bool {
        (self as usize) <= (Self::StageSnapshotEncode as usize)
    }

    pub fn group_name(self) -> &'static str {
        match self {
            Self::StageWorldInit
            | Self::StageTectonics
            | Self::StageToPrehistory
            | Self::StagePrehistory
            | Self::StageToHistory
            | Self::StageHistory
            | Self::StageSnapshotEncode => "Stage",
            Self::TecPlateAdvectionAndSubduction
            | Self::TecRidgeAndOceanBathymetry
            | Self::TecClimateAndOceanCirculation
            | Self::TecErosionAndDeposition
            | Self::TecMetamorphismAndRifting => "Tectonics",
            Self::PreGlaciationClimate
            | Self::PreRiversAndTechSparks
            | Self::PrePopulationExpansion
            | Self::PreRacination => "Prehistory",
            Self::HistInitPrehistoryCopy
            | Self::HistInitSubdivideAndKdTree
            | Self::HistInitBiosphereAndSpecies
            | Self::HistInitRiversAndCapacities
            | Self::HistInitSeedStatesAndLexicons
            | Self::HistInitTravelRoutes
            | Self::HistInitDiplomacyAndTrade => "HistoryInit",
            Self::HistStepGridClones
            | Self::HistStepLanguageSoundChanges
            | Self::HistStepGrowthSpilloverAndTech
            | Self::HistStepAssimilationAndDialects
            | Self::HistStepWarfareConquestAndSplits
            | Self::HistStepTradeAndConflicts
            | Self::HistStepLexiconConcepts => "HistoryStep",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::StageWorldInit => "0. WorldSimulation::new (Init)",
            Self::StageTectonics => "1. Tectonics::step (All Geoticks)",
            Self::StageToPrehistory => "2. Transition: Tectonics -> Prehistory",
            Self::StagePrehistory => "3. Prehistory::step (All Ticks)",
            Self::StageToHistory => "4. Transition: Prehistory -> History",
            Self::StageHistory => "5. History::step (All Ticks)",
            Self::StageSnapshotEncode => "6. Snapshot Protobuf Encode",

            Self::TecPlateAdvectionAndSubduction => "tec::plate_advection_and_subduction",
            Self::TecRidgeAndOceanBathymetry => "tec::ridge_and_ocean_bathymetry",
            Self::TecClimateAndOceanCirculation => "tec::climate_and_ocean_circulation",
            Self::TecErosionAndDeposition => "tec::erosion_and_deposition",
            Self::TecMetamorphismAndRifting => "tec::metamorphism_rifting_and_impacts",

            Self::PreGlaciationClimate => "pre::glaciation_climate_and_isostasy",
            Self::PreRiversAndTechSparks => "pre::rivers_and_tech_sparks",
            Self::PrePopulationExpansion => "pre::expand_population (x6 steps)",
            Self::PreRacination => "pre::racinate_with_technologies",

            Self::HistInitPrehistoryCopy => "hist_init::prehistory_snapshot_roundtrip",
            Self::HistInitSubdivideAndKdTree => "hist_init::subdivide_terrain_and_kdtree",
            Self::HistInitBiosphereAndSpecies => "hist_init::settle_species_on_climate",
            Self::HistInitRiversAndCapacities => "hist_init::route_rivers_and_capacities",
            Self::HistInitSeedStatesAndLexicons => "hist_init::seed_cities_states_and_lexicons",
            Self::HistInitTravelRoutes => "hist_init::compute_potential_travel_routes",
            Self::HistInitDiplomacyAndTrade => "hist_init::initial_diplomacy_and_trade",

            Self::HistStepGridClones => "hist_step::clone_grid_and_elevation_vecs",
            Self::HistStepLanguageSoundChanges => "hist_step::language_mutate_tick",
            Self::HistStepGrowthSpilloverAndTech => "hist_step::growth_spillover_and_tech",
            Self::HistStepAssimilationAndDialects => "hist_step::assimilation_and_dialects",
            Self::HistStepWarfareConquestAndSplits => "hist_step::warfare_conquest_and_splits",
            Self::HistStepTradeAndConflicts => "hist_step::diplomacy_trade_and_conflicts",
            Self::HistStepLexiconConcepts => "hist_step::accumulate_and_borrow_concepts",
        }
    }
}

static PROFILE_ENABLED: AtomicBool = AtomicBool::new(false);

// Global heap allocator counters updated by `ProfiledAllocator` in `tec11-cli`.
pub static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
pub static DEALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
pub static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
pub static TOTAL_ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
pub static TOTAL_DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
pub static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
pub static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[allow(clippy::declare_interior_mutable_const)]
const ZERO_U64: AtomicU64 = AtomicU64::new(0);

static PHASE_CALLS: [AtomicU64; ProfilePhase::COUNT] = [ZERO_U64; ProfilePhase::COUNT];
static PHASE_NANOS: [AtomicU64; ProfilePhase::COUNT] = [ZERO_U64; ProfilePhase::COUNT];
static PHASE_ALLOC_CALLS: [AtomicU64; ProfilePhase::COUNT] = [ZERO_U64; ProfilePhase::COUNT];
static PHASE_ALLOC_BYTES: [AtomicU64; ProfilePhase::COUNT] = [ZERO_U64; ProfilePhase::COUNT];
static PHASE_DEALLOC_BYTES: [AtomicU64; ProfilePhase::COUNT] = [ZERO_U64; ProfilePhase::COUNT];
static PHASE_PEAK_LIVE_BYTES: [AtomicU64; ProfilePhase::COUNT] = [ZERO_U64; ProfilePhase::COUNT];

#[derive(Clone, Copy, Debug, Default)]
pub struct PhaseStats {
    pub calls: u64,
    pub total_nanos: u64,
    pub alloc_calls: u64,
    pub alloc_bytes: u64,
    pub dealloc_bytes: u64,
    pub peak_live_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GlobalHeapStats {
    pub alloc_calls: u64,
    pub dealloc_calls: u64,
    pub realloc_calls: u64,
    pub total_alloc_bytes: u64,
    pub total_dealloc_bytes: u64,
    pub live_bytes: usize,
    pub peak_live_bytes: usize,
}

pub fn set_enabled(enabled: bool) {
    PROFILE_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    PROFILE_ENABLED.load(Ordering::Relaxed)
}

pub fn reset_stats() {
    ALLOC_CALLS.store(0, Ordering::Relaxed);
    DEALLOC_CALLS.store(0, Ordering::Relaxed);
    REALLOC_CALLS.store(0, Ordering::Relaxed);
    TOTAL_ALLOC_BYTES.store(0, Ordering::Relaxed);
    TOTAL_DEALLOC_BYTES.store(0, Ordering::Relaxed);
    let cur_live = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(cur_live, Ordering::Relaxed);

    for i in 0..ProfilePhase::COUNT {
        PHASE_CALLS[i].store(0, Ordering::Relaxed);
        PHASE_NANOS[i].store(0, Ordering::Relaxed);
        PHASE_ALLOC_CALLS[i].store(0, Ordering::Relaxed);
        PHASE_ALLOC_BYTES[i].store(0, Ordering::Relaxed);
        PHASE_DEALLOC_BYTES[i].store(0, Ordering::Relaxed);
        PHASE_PEAK_LIVE_BYTES[i].store(0, Ordering::Relaxed);
    }
}

pub fn global_heap_stats() -> GlobalHeapStats {
    GlobalHeapStats {
        alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
        dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
        realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
        total_alloc_bytes: TOTAL_ALLOC_BYTES.load(Ordering::Relaxed),
        total_dealloc_bytes: TOTAL_DEALLOC_BYTES.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
    }
}

pub fn phase_stats(phase: ProfilePhase) -> PhaseStats {
    let idx = phase as usize;
    PhaseStats {
        calls: PHASE_CALLS[idx].load(Ordering::Relaxed),
        total_nanos: PHASE_NANOS[idx].load(Ordering::Relaxed),
        alloc_calls: PHASE_ALLOC_CALLS[idx].load(Ordering::Relaxed),
        alloc_bytes: PHASE_ALLOC_BYTES[idx].load(Ordering::Relaxed),
        dealloc_bytes: PHASE_DEALLOC_BYTES[idx].load(Ordering::Relaxed),
        peak_live_bytes: PHASE_PEAK_LIVE_BYTES[idx].load(Ordering::Relaxed),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct ProfileSpan {
    phase: ProfilePhase,
    start_time: Instant,
    start_alloc_calls: u64,
    start_alloc_bytes: u64,
    start_dealloc_bytes: u64,
}

#[cfg(target_arch = "wasm32")]
pub struct ProfileSpan;

#[inline]
pub fn span(phase: ProfilePhase) -> Option<ProfileSpan> {
    if !PROFILE_ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let cur_live = LIVE_BYTES.load(Ordering::Relaxed) as u64;
        PHASE_PEAK_LIVE_BYTES[phase as usize].fetch_max(cur_live, Ordering::Relaxed);
        Some(ProfileSpan {
            phase,
            start_time: Instant::now(),
            start_alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            start_alloc_bytes: TOTAL_ALLOC_BYTES.load(Ordering::Relaxed),
            start_dealloc_bytes: TOTAL_DEALLOC_BYTES.load(Ordering::Relaxed),
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = phase;
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ProfileSpan {
    #[inline]
    fn drop(&mut self) {
        let elapsed_ns = self.start_time.elapsed().as_nanos() as u64;
        let d_calls = ALLOC_CALLS
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_alloc_calls);
        let d_alloc = TOTAL_ALLOC_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_alloc_bytes);
        let d_dealloc = TOTAL_DEALLOC_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_dealloc_bytes);
        let cur_live = LIVE_BYTES.load(Ordering::Relaxed) as u64;

        let idx = self.phase as usize;
        PHASE_CALLS[idx].fetch_add(1, Ordering::Relaxed);
        PHASE_NANOS[idx].fetch_add(elapsed_ns, Ordering::Relaxed);
        PHASE_ALLOC_CALLS[idx].fetch_add(d_calls, Ordering::Relaxed);
        PHASE_ALLOC_BYTES[idx].fetch_add(d_alloc, Ordering::Relaxed);
        PHASE_DEALLOC_BYTES[idx].fetch_add(d_dealloc, Ordering::Relaxed);
        PHASE_PEAK_LIVE_BYTES[idx].fetch_max(cur_live, Ordering::Relaxed);
    }
}
