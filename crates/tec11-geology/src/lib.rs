pub mod history;
pub mod lifeforms;
pub mod movement;
pub mod pangea;
pub mod population;
pub mod prehistory;
pub mod profile;
pub mod racination;
pub mod rivers;
pub mod rock;
pub mod tectonics;
pub mod terrain;
pub mod tile;

pub use tec11_cosmos as sun_sky;
pub use tec11_language as language;
pub use tec11_sky_view as sky_vantage;

/// Convert `&[TileGeology]` into lightweight `TileSurfaceInfo` entries for `tec11-sky-view`.
pub fn tile_surface_infos(tiles: &[tile::TileGeology]) -> Vec<tec11_sky_view::TileSurfaceInfo> {
    tiles
        .iter()
        .map(|t| {
            let (rock_name, rock_kind_code) = if let Some(top) = t.layers.last() {
                (top.rock.name, top.rock.kind.code())
            } else {
                ("basalt", "I")
            };
            tec11_sky_view::TileSurfaceInfo {
                elev_km: t.elevation(),
                mountainosity_norm: t.mountainosity_norm(),
                top_felsity: t.top_felsity(),
                rock_name,
                rock_kind_code,
            }
        })
        .collect()
}

pub use history::{
    Community, ConceptKey, HistorySimulation, LanguageDetailView, LanguageRecord, LexiconEntryView,
    NationDetailView, StateInspectionView, StatePair, StateRecord, StateTopology, TravelRoute,
    TravelRouteKind, TravelRouteStatus, ZoomedMapPolygon,
};
pub use sky_vantage::{
    build_celestial_sky_view, build_coarse_tile_inspection_view,
    build_uniform_fine_tile_inspection_view, format_share_percent, format_thousands_compact,
    CoarseTileSkyParams, DemographicShareItem, TileInspectionView, TileLiveEnvironment,
    TileSkyConstellation, TileSkyMoon, TileSkyPlanet, TileSkyView, TileVantageNeighbor,
    TileVantageSample, TileVantageView, UniformFineTileSkyParams,
};
pub use language::{
    a_mutate, apheresize, apocopate, break_vowels, i_mutate, open_consonants, sonorize_consonants,
    syncopate, vocalize_consonants, ConsonantDef, Language, LanguageSandbox, PhonotacticConstraint,
    SoundChangeKind, Syllable, VowelDef, Word, WordOrigin, CONSONANTS, VOWELS,
};
pub use lifeforms::{
    settle_species_on_climate, LifeformsSandbox, SettledBiosphere, SettledSpecies, SpeciesKind,
};
pub use pangea::PangeaWorld;
pub use population::{
    agricultural_density, eden_tile, expand_population, expand_population_with_technologies,
    expand_population_with_technologies_and_islands, expand_within,
    identify_continents_and_oceans, is_farmable, is_habitable, is_hoppable, is_passable,
    is_squattable, is_subgrid_island_habitable, main_ocean, near_coast, paleolithic_density,
    tile_population_thousands, within_steps, ContinentInfo, DisjointSet, OceanBasinInfo,
    PlanetaryGeography, PopulationSandbox,
};
pub use prehistory::PrehistorySimulation;
pub use racination::{racinate, racination_shades, Heritage, RacinationSandbox};
pub use rivers::{trace_coarse_rivers, RiversSandbox};
pub use rock::{Rock, RockKind, RockLayer};
pub use sun_sky::{
    build_constellations_from_stars, build_constellations_from_stars_with_limit, bv_to_rgb,
    bv_to_temp_k, classify_star,
    format_solar_system_summary, generate_default_constellations, generate_solar_system_with_rng,
    recompute_planet_visibility_from_home, sample_atmosphere_with_rng,
    sample_galactic_home_with_rng, sample_home_star_with_rng, substitute_simulated_planet,
    synthesize_galactic_sky_constellations, AtmosphereType, CelestialStar, Constellation,
    GalacticSkyBackdrop, GalacticVantage, GalaxyType, HomeStar, PlanetKind, PlanetaryOptics,
    ReconstructedSkyEnvironment, SolarSystemPlanet,
};
pub use tec11_sky_view::SunSandbox;
pub use tectonics::{
    classify_submarine_tile, LithosphericGroup, MantleHotspot, OceanicSubductionArc,
    SpreadingRidgeArc, TectonicsSimulation,
};
pub use terrain::TerrainSandbox;
pub use tile::TileGeology;
