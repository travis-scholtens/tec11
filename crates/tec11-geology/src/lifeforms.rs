use tec11_climate::{ClimateModel, EARTH_SIZE6_ELEVATIONS_M};
use tec11_math_grid::{normal, PointTree, SphericalGrid, Vec3};
use tec11_proto::PlanetSnapshotExt;

use crate::population::{identify_continents_and_oceans, PlanetaryGeography};
use crate::rivers::trace_coarse_rivers;
use crate::tectonics::TectonicsSimulation;

const ANIMALS_TSV: &str = include_str!("../data/animals.tsv");
const PLANTS_TSV: &str = include_str!("../data/plants.tsv");

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpeciesKind {
    Animal,
    Plant,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ValueRange {
    pub min: f32,
    pub max: f32,
}

impl ValueRange {
    #[inline]
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn contains(self, v: f32) -> bool {
        v >= self.min && v <= self.max
    }

    #[inline]
    pub fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            min: f(self.min),
            max: f(self.max),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ClimateTolerance {
    pub temp_norm: ValueRange,
    pub precip_norm: ValueRange,
    pub sun_norm: ValueRange,
}

impl ClimateTolerance {
    #[inline]
    pub fn in_range(&self, temp: f32, precip: f32, sun: f32) -> bool {
        self.temp_norm.contains(temp)
            && self.precip_norm.contains(precip)
            && self.sun_norm.contains(sun)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TileSeasonPresence {
    pub tile: usize,
    pub season_mask: u8,
}

#[derive(Clone, Debug)]
pub struct SettledSpecies {
    pub id: usize,
    pub kind: SpeciesKind,
    pub subkind: String,
    /// `1` = Primary Producer (Plant), `2` = Primary Consumer (Herbivore / Invertebrate),
    /// `3` = Secondary Consumer (Mesopredator / Omnivore), `4` = Apex Predator.
    pub trophic_level: u8,
    pub sealife: bool,
    pub name: String,
    /// Tile presence entries where `season_mask != 0`.
    pub tile_seasons: Vec<TileSeasonPresence>,
}

const RES_FOLIAGE: u16 = 1 << 0;
const RES_GRASS: u16 = 1 << 1;
const RES_WOOD_BARK: u16 = 1 << 2;
const RES_FLOWER_NECTAR: u16 = 1 << 3;
const RES_FRUIT: u16 = 1 << 4;
const RES_NUT: u16 = 1 << 5;
const RES_SEED: u16 = 1 << 6;
const RES_TUBER_FUNGUS: u16 = 1 << 7;
const RES_MARINE_BASE: u16 = 1 << 8;

const PREY_INSECT: usize = 0;
const PREY_SMALL_FISH: usize = 1;
const PREY_LARGE_FISH: usize = 2;
const PREY_SMALL_AMPHIBIAN: usize = 3;
const PREY_LARGE_AMPHIBIAN: usize = 4;
const PREY_SMALL_REPTILE: usize = 5;
const PREY_LARGE_REPTILE: usize = 6;
const PREY_SMALL_BIRD: usize = 7;
const PREY_LARGE_BIRD: usize = 8;
const PREY_SMALL_MAMMAL: usize = 9;
const PREY_LARGE_MAMMAL: usize = 10;
const NUM_PREY_GUILDS: usize = 11;

fn plant_output_mask(json_str: &str, kind_str: &str, sealife: bool) -> u16 {
    if sealife {
        return RES_MARINE_BASE | RES_FOLIAGE;
    }
    let mut mask = RES_FOLIAGE;
    match kind_str {
        "grass" => mask |= RES_GRASS | RES_SEED,
        "tree" | "bush" => mask |= RES_WOOD_BARK,
        "tuber" | "fungus" => mask |= RES_TUBER_FUNGUS,
        "flower" => mask |= RES_FLOWER_NECTAR,
        "fruit" => mask |= RES_FRUIT,
        _ => {}
    }
    if extract_json_bool(json_str, "flower").unwrap_or(false) {
        mask |= RES_FLOWER_NECTAR;
    }
    if extract_json_bool(json_str, "fruits").unwrap_or(false) {
        mask |= RES_FRUIT;
    }
    if extract_json_bool(json_str, "nuts").unwrap_or(false) {
        mask |= RES_NUT;
    }
    if extract_json_bool(json_str, "seeds").unwrap_or(false)
        || extract_json_bool(json_str, "cones").unwrap_or(false)
    {
        mask |= RES_SEED;
    }
    if extract_json_bool(json_str, "tuber").unwrap_or(false) {
        mask |= RES_TUBER_FUNGUS;
    }
    mask
}

fn prey_guild_for_animal_kind(kind_str: &str) -> usize {
    match kind_str {
        "insect" => PREY_INSECT,
        "small fish" => PREY_SMALL_FISH,
        "large fish" => PREY_LARGE_FISH,
        "small amphibian" => PREY_SMALL_AMPHIBIAN,
        "large amphibian" => PREY_LARGE_AMPHIBIAN,
        "small reptile" => PREY_SMALL_REPTILE,
        "large reptile" => PREY_LARGE_REPTILE,
        "small bird" => PREY_SMALL_BIRD,
        "large bird" => PREY_LARGE_BIRD,
        "small mammal" => PREY_SMALL_MAMMAL,
        "large mammal" | "giant mammal" => PREY_LARGE_MAMMAL,
        _ => PREY_SMALL_MAMMAL,
    }
}

fn parse_animal_diet_masks(json_str: &str) -> (u16, u16) {
    let needle = "\"diet\":";
    let Some(pos) = json_str.find(needle) else {
        return (RES_FOLIAGE, 0);
    };
    let rest = &json_str[pos + needle.len()..];
    let Some(open_b) = rest.find('[') else {
        return (RES_FOLIAGE, 0);
    };
    let after_open = &rest[open_b + 1..];
    let Some(close_b) = after_open.find(']') else {
        return (RES_FOLIAGE, 0);
    };
    let list_str = &after_open[..close_b];

    let mut plant_mask = 0u16;
    let mut prey_mask = 0u16;
    for raw_item in list_str.split(',') {
        let item = raw_item.trim().trim_matches('"').trim();
        match item {
            "plant" | "leaf" | "eucalyptus leaves" | "forb" | "forbs" | "herb" | "shrub"
            | "shrubs" | "bud" | "cactus" | "succulent" | "crop" => {
                plant_mask |= RES_FOLIAGE;
            }
            "grass" | "grain" | "sedge" => {
                plant_mask |= RES_GRASS | RES_FOLIAGE;
            }
            "wood" | "bark" | "twig" | "sap" | "tree sap" | "gum" => {
                plant_mask |= RES_WOOD_BARK;
            }
            "flower" | "nectar" | "pollen" | "honey" | "honeydew" => {
                plant_mask |= RES_FLOWER_NECTAR;
            }
            "fruit" | "small fruit" | "berry" => {
                plant_mask |= RES_FRUIT;
            }
            "nut" => {
                plant_mask |= RES_NUT;
            }
            "seed" => {
                plant_mask |= RES_SEED;
            }
            "tuber" | "tubers" | "root" | "bulb" | "fungi" | "mushroom" | "moss" | "lichen" => {
                plant_mask |= RES_TUBER_FUNGUS | RES_FOLIAGE;
            }
            "algae" | "seaweed" | "plankton" | "zooplankton" | "krill" | "copepods" | "coral" => {
                plant_mask |= RES_MARINE_BASE | RES_FOLIAGE;
            }
            "insect" | "large insect" | "ant" | "flying ant" | "termite" | "termite larvae"
            | "bee" | "wasp" | "hornet" | "butterfly" | "dragonfly" | "locust" | "spider"
            | "scorpion" | "centipede" | "worm" | "snail" | "mollusk" | "crustacean" | "crab"
            | "crayfish" | "shrimp" | "yabbies" | "invertebrate" | "small invertebrate"
            | "other invertebrates" => {
                prey_mask |= 1u16 << PREY_INSECT;
            }
            "small fish" | "squid" | "octopus" | "cuttlefish" => {
                prey_mask |= 1u16 << PREY_SMALL_FISH;
            }
            "large fish" | "shark" | "rays" => {
                prey_mask |= (1u16 << PREY_LARGE_FISH) | (1u16 << PREY_SMALL_FISH);
            }
            "small amphibian" => {
                prey_mask |= 1u16 << PREY_SMALL_AMPHIBIAN;
            }
            "large amphibian" => {
                prey_mask |= (1u16 << PREY_LARGE_AMPHIBIAN) | (1u16 << PREY_SMALL_AMPHIBIAN);
            }
            "small reptile" | "turtle" => {
                prey_mask |= 1u16 << PREY_SMALL_REPTILE;
            }
            "large reptile" => {
                prey_mask |= (1u16 << PREY_LARGE_REPTILE) | (1u16 << PREY_SMALL_REPTILE);
            }
            "small bird" | "egg" | "penguin" => {
                prey_mask |= 1u16 << PREY_SMALL_BIRD;
            }
            "large bird" => {
                prey_mask |= (1u16 << PREY_LARGE_BIRD) | (1u16 << PREY_SMALL_BIRD);
            }
            "small mammal" => {
                prey_mask |= 1u16 << PREY_SMALL_MAMMAL;
            }
            "carcass" | "bone" | "bone marrow" | "suet" | "seal placenta" | "garbage" => {
                prey_mask |= (1u16 << PREY_SMALL_MAMMAL)
                    | (1u16 << PREY_SMALL_BIRD)
                    | (1u16 << PREY_SMALL_REPTILE);
            }
            "large mammal" | "giant mammal" | "pig" | "water buffalo" | "seal" | "sea lion"
            | "walrus" | "dolphin" | "whale" => {
                prey_mask |= (1u16 << PREY_LARGE_MAMMAL) | (1u16 << PREY_SMALL_MAMMAL);
            }
            _ => {}
        }
    }
    (plant_mask, prey_mask)
}

fn classify_animal_trophic_level(
    kind_str: &str,
    sealife: bool,
    plant_mask: u16,
    prey_mask: u16,
) -> u8 {
    if kind_str == "insect" || prey_mask == 0 {
        return 2;
    }
    let vertebrate_prey_mask = prey_mask & !(1u16 << PREY_INSECT);
    if vertebrate_prey_mask == 0 {
        // Pure insectivore / omnivore eating plants + insects
        return if plant_mask != 0
            && !matches!(kind_str, "large mammal" | "large bird" | "large reptile")
        {
            2
        } else {
            3
        };
    }
    let is_large_predator = matches!(
        kind_str,
        "large mammal" | "giant mammal" | "large reptile" | "large bird" | "large fish"
    );
    let large_vertebrate_mask = (1u16 << PREY_LARGE_MAMMAL)
        | (1u16 << PREY_LARGE_REPTILE)
        | (1u16 << PREY_LARGE_BIRD)
        | (1u16 << PREY_LARGE_FISH);
    let eats_insects = (prey_mask & (1u16 << PREY_INSECT)) != 0;
    let is_marine_mammal_pinniped = sealife
        && matches!(kind_str, "large mammal" | "giant mammal")
        && (prey_mask & (1u16 << PREY_LARGE_MAMMAL)) == 0;
    if is_large_predator
        && plant_mask == 0
        && !eats_insects
        && !is_marine_mammal_pinniped
        && (prey_mask & large_vertebrate_mask) != 0
    {
        4
    } else {
        3
    }
}

#[derive(Clone)]
struct ParsedAnimalSpec<'a> {
    name: &'a str,
    kind_str: &'a str,
    prey_guild: usize,
    trophic_level: u8,
    sealife: bool,
    is_obligate_freshwater: bool,
    tol: ClimateTolerance,
    plant_diet_mask: u16,
    animal_prey_mask: u16,
}

#[derive(Clone, Debug)]
pub struct SettledBiosphere {
    pub geography: PlanetaryGeography,
    pub species: Vec<SettledSpecies>,
    /// Per-season (`0..8`), per-tile animal count.
    pub seasonal_animal_counts: Vec<Vec<u16>>,
    /// Per-season (`0..8`), per-tile plant count.
    pub seasonal_plant_counts: Vec<Vec<u16>>,
    /// Per-tile union of all settled species indices (`0..species.len()`) across all seasons,
    /// matching `HistorySimulation.tilespecies` (`historysimulation.py:957-964`).
    pub tile_all_species: Vec<Vec<usize>>,
}

/// Extract a `"<key>": "<low>-<high>"` range from a JSON line in `animals.tsv` / `plants.tsv`.
fn extract_json_range(json_str: &str, key: &str) -> Option<ValueRange> {
    let needle = format!("\"{key}\":");
    let pos = json_str.find(&needle)?;
    let rest = &json_str[pos + needle.len()..];
    let first_quote = rest.find('"')?;
    let after_first = &rest[first_quote + 1..];
    let second_quote = after_first.find('"')?;
    let range_str = after_first[..second_quote].trim();

    // Match `(-?[0-9]+)-(-?[0-9]+)`: find the hyphen separating low and high (skipping leading '-')
    let sep = (1..range_str.len()).find(|&i| range_str.as_bytes()[i] == b'-')?;
    let min: f32 = range_str[..sep].parse().ok()?;
    let max: f32 = range_str[sep + 1..].parse().ok()?;
    Some(ValueRange::new(min, max))
}

fn extract_json_bool(json_str: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\":");
    let pos = json_str.find(&needle)?;
    let rest = json_str[pos + needle.len()..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn extract_json_str<'a>(json_str: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let pos = json_str.find(&needle)?;
    let rest = &json_str[pos + needle.len()..];
    let first_quote = rest.find('"')?;
    let after_first = &rest[first_quote + 1..];
    let second_quote = after_first.find('"')?;
    Some(after_first[..second_quote].trim())
}

fn normalize_temp(celsius_range: ValueRange) -> ValueRange {
    celsius_range.map(|c| (c + 25.0) / 75.0)
}

fn normalize_precip(cm_range: ValueRange) -> ValueRange {
    ValueRange::new(
        (cm_range.min / 200.0).min(0.68),
        (cm_range.max + 20.0) / 110.0,
    )
}

fn normalize_sun(hours_range: ValueRange) -> ValueRange {
    ValueRange::new(
        ((hours_range.min - 400.0).max(0.0) / 3400.0).min(0.65),
        (hours_range.max + 600.0) / 2600.0,
    )
}

/// Settle all flora and fauna from `animals.tsv` and `plants.tsv` onto the given planet's
/// `elevations` and `climate_model`, matching `lifeformsmethod.settle` and `species.py`.
pub fn settle_species_on_climate(
    elevations: &[f32],
    climate_model: &ClimateModel,
) -> SettledBiosphere {
    let n = elevations.len();
    let num_seasons = climate_model.season_count().clamp(1, 8);
    let all_seasons_mask: u8 = if num_seasons >= 8 {
        0xFF
    } else {
        (1u8 << num_seasons) - 1
    };

    let grid = climate_model.grid();
    let adj = grid.adjacency();
    let geography = identify_continents_and_oceans(grid, elevations);

    let land_tiles: Vec<usize> = (0..n).filter(|&t| elevations[t] > 0.0).collect();
    let sea_tiles: Vec<usize> = (0..n).filter(|&t| elevations[t] <= 0.0).collect();

    let koeppen_ids = climate_model.koeppen_ids();
    let mean_precip: Vec<f32> = climate_model
        .classifications()
        .iter()
        .map(|c| c.mean_precipitation as f32)
        .collect();
    let rivers = trace_coarse_rivers(elevations, &mean_precip, &koeppen_ids, &adj, 2.0, 0.25);
    let mut is_river_or_wetland = vec![false; n];
    for r in rivers {
        for t in r {
            if t < n {
                is_river_or_wetland[t] = true;
            }
        }
    }
    for &t in &land_tiles {
        if mean_precip.get(t).copied().unwrap_or(0.0) >= 0.45
            || adj[t].iter().any(|&nb| nb < n && elevations[nb] <= 0.0)
        {
            is_river_or_wetland[t] = true;
        }
    }
    let freshwater_tiles: Vec<usize> = land_tiles
        .iter()
        .copied()
        .filter(|&t| is_river_or_wetland[t])
        .collect();

    // Flatten seasonal climate arrays `[season * n + tile]` for cache-friendly inner loops
    let mut temp_s = vec![0.0f32; num_seasons * n];
    let mut precip_s = vec![0.0f32; num_seasons * n];
    let mut sun_s = vec![0.0f32; num_seasons * n];
    for s in 0..num_seasons {
        if let Some(snap) = climate_model.season_snapshot(s) {
            let base = s * n;
            temp_s[base..base + n].copy_from_slice(&snap.temperature);
            precip_s[base..base + n].copy_from_slice(&snap.precipitation);
            sun_s[base..base + n].copy_from_slice(&snap.insolation);
        }
    }

    let mut settled = Vec::new();
    let mut seasonal_animal_counts = vec![vec![0u16; n]; num_seasons];
    let mut seasonal_plant_counts = vec![vec![0u16; n]; num_seasons];
    let mut tile_all_species = vec![Vec::new(); n];

    // Per-(season, tile) plant food output bitmask and total settled plant species count.
    let mut plant_res_mask = vec![0u16; num_seasons * n];
    let mut plant_species_count = vec![0u16; num_seasons * n];
    for &t in &sea_tiles {
        for s in 0..num_seasons {
            let idx = s * n + t;
            if sun_s[idx] > 0.05 && temp_s[idx] > 0.25 {
                plant_res_mask[idx] |= RES_MARINE_BASE | RES_FOLIAGE;
            }
        }
    }

    // --- Pass 1 (Trophic Level 1): Primary Producers (Plants & Marine Flora) ---
    for line in PLANTS_TSV.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name_part, json_part)) = line.split_once('\t') else {
            continue;
        };
        let Some(t_range) = extract_json_range(json_part, "temperatureRange") else {
            continue;
        };
        let Some(p_range) = extract_json_range(json_part, "precipitationRange") else {
            continue;
        };
        let Some(s_range) = extract_json_range(json_part, "sunlightRange") else {
            continue;
        };
        let sealife = extract_json_bool(json_part, "sealife").unwrap_or(false);
        let plant_kind = extract_json_str(json_part, "kind").unwrap_or("bush");
        let out_mask = plant_output_mask(json_part, plant_kind, sealife);
        let tol = ClimateTolerance {
            temp_norm: normalize_temp(t_range),
            precip_norm: if sealife {
                ValueRange::new(0.0, 2.0)
            } else {
                normalize_precip(p_range)
            },
            sun_norm: normalize_sun(s_range),
        };

        let candidate_tiles: &[usize] = if sealife { &sea_tiles } else { &land_tiles };
        let plant_tiles: Vec<TileSeasonPresence> = candidate_tiles
            .iter()
            .copied()
            .filter_map(|t| {
                let mask = (0..num_seasons).fold(0u8, |acc, s| {
                    let idx = s * n + t;
                    let eff_precip = if sealife { 1.0 } else { precip_s[idx] };
                    if tol.in_range(temp_s[idx], eff_precip, sun_s[idx]) {
                        acc | (1u8 << s)
                    } else {
                        acc
                    }
                });
                (mask != 0).then_some(TileSeasonPresence {
                    tile: t,
                    season_mask: mask,
                })
            })
            .collect();

        if !plant_tiles.is_empty() {
            let sp_id = settled.len();
            for &presence in &plant_tiles {
                tile_all_species[presence.tile].push(sp_id);
                for s in 0..num_seasons {
                    if (presence.season_mask & (1u8 << s)) != 0 {
                        let idx = s * n + presence.tile;
                        plant_res_mask[idx] |= out_mask;
                        plant_species_count[idx] = plant_species_count[idx].saturating_add(1);
                        seasonal_plant_counts[s][presence.tile] =
                            seasonal_plant_counts[s][presence.tile].saturating_add(1);
                    }
                }
            }
            settled.push(SettledSpecies {
                id: sp_id,
                kind: SpeciesKind::Plant,
                subkind: plant_kind.to_string(),
                trophic_level: 1,
                sealife,
                name: name_part.trim().to_string(),
                tile_seasons: plant_tiles,
            });
        }
    }

    // Parse all animal templates and sort by trophic level:
    // Level 2 insects first -> Level 2 vertebrate herbivores -> Level 3 mesopredators/omnivores -> Level 4 apex predators.
    let mut parsed_animals: Vec<ParsedAnimalSpec<'_>> = Vec::new();
    for line in ANIMALS_TSV.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name_part, json_part)) = line.split_once('\t') else {
            continue;
        };
        let Some(t_range) = extract_json_range(json_part, "temperatureRange") else {
            continue;
        };
        let Some(p_range) = extract_json_range(json_part, "precipitationRange") else {
            continue;
        };
        let sealife = extract_json_bool(json_part, "sealife").unwrap_or(false);
        let kind_str = extract_json_str(json_part, "kind").unwrap_or("small mammal");
        let is_obligate_freshwater = !sealife
            && matches!(
                kind_str,
                "small fish" | "large fish" | "small amphibian" | "large amphibian"
            );
        let (plant_diet_mask, animal_prey_mask) = parse_animal_diet_masks(json_part);
        let trophic_level =
            classify_animal_trophic_level(kind_str, sealife, plant_diet_mask, animal_prey_mask);
        let tol = ClimateTolerance {
            temp_norm: normalize_temp(t_range),
            precip_norm: if sealife {
                ValueRange::new(0.0, 2.0)
            } else {
                normalize_precip(p_range)
            },
            sun_norm: ValueRange::new(0.0, 2.0),
        };
        parsed_animals.push(ParsedAnimalSpec {
            name: name_part.trim(),
            kind_str,
            prey_guild: prey_guild_for_animal_kind(kind_str),
            trophic_level,
            sealife,
            is_obligate_freshwater,
            tol,
            plant_diet_mask,
            animal_prey_mask,
        });
    }

    parsed_animals.sort_by_key(|a| (a.trophic_level, a.kind_str != "insect", a.name));

    let mut prey_mask = vec![0u16; num_seasons * n];
    let mut prey_guild_counts = vec![0u16; num_seasons * n * NUM_PREY_GUILDS];
    // Baseline marine invertebrates/forage fish food web on productive sea tiles so Level 2/3 marine trophic chains bootstrap:
    for &t in &sea_tiles {
        for s in 0..num_seasons {
            let idx = s * n + t;
            if (plant_res_mask[idx] & RES_MARINE_BASE) != 0 {
                prey_mask[idx] |= (1u16 << PREY_INSECT) | (1u16 << PREY_SMALL_FISH);
            }
        }
    }

    let find_viable_animal_tiles = |spec: &ParsedAnimalSpec<'_>,
                                    plant_res_mask: &[u16],
                                    plant_species_count: &[u16],
                                    prey_mask: &[u16]|
     -> Vec<TileSeasonPresence> {
        let candidate_tiles: &[usize] = if spec.sealife {
            &sea_tiles
        } else if spec.is_obligate_freshwater {
            &freshwater_tiles
        } else {
            &land_tiles
        };

        let winter_range = ValueRange::new(
            (spec.tol.temp_norm.min - 0.25).clamp(0.0, 0.45),
            spec.tol.temp_norm.min,
        );
        let min_plants_required: u16 = if spec.sealife {
            0
        } else if spec.kind_str == "giant mammal" {
            4
        } else if spec.kind_str == "large mammal" && spec.trophic_level == 2 {
            2
        } else {
            1
        };

        let mut combined_tiles: Vec<TileSeasonPresence> = Vec::new();
        let mut mig_candidates: Vec<TileSeasonPresence> = Vec::new();

        for &t in candidate_tiles {
            let (mask, winter_ok) =
                (0..num_seasons).fold((0u8, true), |(mask, winter_ok), s| {
                    let idx = s * n + t;
                    let eff_precip = if spec.sealife { 1.0 } else { precip_s[idx] };
                    let climate_ok = spec.tol.in_range(temp_s[idx], eff_precip, sun_s[idx]);
                    let has_plant_food = (plant_res_mask[idx] & spec.plant_diet_mask) != 0
                        && plant_species_count[idx] >= min_plants_required;
                    let has_animal_prey = (prey_mask[idx] & spec.animal_prey_mask) != 0;
                    let food_ok = match spec.trophic_level {
                        2 => has_plant_food || (spec.plant_diet_mask == 0 && has_animal_prey),
                        3 => has_animal_prey || has_plant_food,
                        _ => has_animal_prey,
                    };
                    if climate_ok && food_ok {
                        (mask | (1u8 << s), winter_ok)
                    } else {
                        (mask, winter_ok && winter_range.contains(temp_s[idx]))
                    }
                });

            if mask == all_seasons_mask {
                combined_tiles.push(TileSeasonPresence {
                    tile: t,
                    season_mask: all_seasons_mask,
                });
            } else if mask != 0 && winter_ok && !spec.sealife {
                combined_tiles.push(TileSeasonPresence {
                    tile: t,
                    season_mask: mask,
                });
            } else if mask != 0 {
                mig_candidates.push(TileSeasonPresence {
                    tile: t,
                    season_mask: mask,
                });
            }
        }

        if !mig_candidates.is_empty() {
            let can_fly_or_swim_globally =
                spec.sealife || matches!(spec.kind_str, "small bird" | "large bird");
            let num_realms = if can_fly_or_swim_globally {
                1
            } else {
                geography.continents.len().max(1)
            };
            let mut realm_masks = vec![vec![false; 256]; num_realms];
            for &p in &combined_tiles {
                if p.season_mask == all_seasons_mask {
                    let r = if can_fly_or_swim_globally {
                        0
                    } else {
                        geography.tile_continent[p.tile].unwrap_or(0)
                    };
                    if r < num_realms {
                        realm_masks[r][all_seasons_mask as usize] = true;
                    }
                }
            }
            for &p in &mig_candidates {
                let r = if can_fly_or_swim_globally {
                    0
                } else {
                    geography.tile_continent[p.tile].unwrap_or(0)
                };
                if r < num_realms {
                    realm_masks[r][p.season_mask as usize] = true;
                }
            }
            let mut valid_realm_mask = vec![vec![false; 256]; num_realms];
            for r in 0..num_realms {
                for m1 in 1..(all_seasons_mask as usize) {
                    if realm_masks[r][m1]
                        && (1..=(all_seasons_mask as usize)).any(|m2| {
                            realm_masks[r][m2] && ((m1 | m2) as u8) == all_seasons_mask
                        })
                    {
                        valid_realm_mask[r][m1] = true;
                    }
                }
            }
            for p in mig_candidates {
                let r = if can_fly_or_swim_globally {
                    0
                } else {
                    geography.tile_continent[p.tile].unwrap_or(0)
                };
                if r < num_realms && valid_realm_mask[r][p.season_mask as usize] {
                    combined_tiles.push(p);
                }
            }
        }

        combined_tiles.sort_by_key(|p| p.tile);
        combined_tiles
    };

    // --- Passes 2 & 3: Settle Level 2 (Insects & Herbivores) and Level 3 (Mesopredators & Omnivores) ---
    let mut apex_candidates: Vec<(ParsedAnimalSpec<'_>, Vec<TileSeasonPresence>)> = Vec::new();
    for spec in parsed_animals {
        if spec.trophic_level >= 4 {
            apex_candidates.push((spec, Vec::new()));
            continue;
        }
        let tile_seasons =
            find_viable_animal_tiles(&spec, &plant_res_mask, &plant_species_count, &prey_mask);
        if tile_seasons.is_empty() {
            continue;
        }
        let sp_id = settled.len();
        for &presence in &tile_seasons {
            tile_all_species[presence.tile].push(sp_id);
            for s in 0..num_seasons {
                if (presence.season_mask & (1u8 << s)) != 0 {
                    let idx = s * n + presence.tile;
                    prey_mask[idx] |= 1u16 << spec.prey_guild;
                    let g_idx = idx * NUM_PREY_GUILDS + spec.prey_guild;
                    prey_guild_counts[g_idx] = prey_guild_counts[g_idx].saturating_add(1);
                    seasonal_animal_counts[s][presence.tile] =
                        seasonal_animal_counts[s][presence.tile].saturating_add(1);
                }
            }
        }
        settled.push(SettledSpecies {
            id: sp_id,
            kind: SpeciesKind::Animal,
            subkind: spec.kind_str.to_string(),
            trophic_level: spec.trophic_level,
            sealife: spec.sealife,
            name: spec.name.to_string(),
            tile_seasons,
        });
    }

    // --- Pass 4 (Trophic Level 4): Apex Predators with Emergent Prey-Limited Competition ---
    for (spec, tiles_out) in &mut apex_candidates {
        *tiles_out =
            find_viable_animal_tiles(spec, &plant_res_mask, &plant_species_count, &prey_mask);
    }

    // Group candidate apex predators by tile and hunting class (0 = mammalian, 1 = reptilian, 2 = avian, 3 = marine/fish)
    // and select winners per tile up to the emergent prey-supported capacity on that tile.
    let mut tile_class_candidates: Vec<[Vec<(usize, u8, f32)>; 4]> =
        vec![[Vec::new(), Vec::new(), Vec::new(), Vec::new()]; n];
    for (cand_idx, (spec, tile_seasons)) in apex_candidates.iter().enumerate() {
        let apex_class = if spec.sealife || spec.kind_str == "large fish" {
            3
        } else if matches!(spec.kind_str, "large mammal" | "giant mammal") {
            0
        } else if spec.kind_str == "large reptile" {
            1
        } else {
            2
        };
        let t_mid = 0.5 * (spec.tol.temp_norm.min + spec.tol.temp_norm.max);
        let t_half = (0.5 * (spec.tol.temp_norm.max - spec.tol.temp_norm.min)).max(0.05);
        let p_mid = 0.5 * (spec.tol.precip_norm.min + spec.tol.precip_norm.max);
        let p_half = (0.5 * (spec.tol.precip_norm.max - spec.tol.precip_norm.min)).max(0.05);
        let name_hash = spec
            .name
            .bytes()
            .fold(0x811C_9DC5u32, |acc, b| (acc ^ (b as u32)).wrapping_mul(0x0100_0193));

        for &presence in tile_seasons {
            let t = presence.tile;
            let realm_id = if spec.sealife {
                geography.tile_ocean[t].unwrap_or(0)
            } else {
                geography.tile_continent[t].unwrap_or(0)
            };
            // Deterministic continental lineage affinity so different continents favor distinct apex predators
            let lineage_affinity = (((name_hash
                ^ ((realm_id as u32).wrapping_mul(0x9E37_79B9)))
                .wrapping_mul(0x85EB_CA6B))
                & 0xFFFF) as f32
                / 65535.0;

            let mut active_s = 0u32;
            let mut climate_fit_sum = 0.0f32;
            let mut matching_prey_sum = 0u32;
            for s in 0..num_seasons {
                if (presence.season_mask & (1u8 << s)) != 0 {
                    active_s += 1;
                    let idx = s * n + t;
                    let dt = ((temp_s[idx] - t_mid) / t_half).abs();
                    let dp = if spec.sealife {
                        0.0
                    } else {
                        ((precip_s[idx] - p_mid) / p_half).abs()
                    };
                    climate_fit_sum += (2.0 - dt - dp).max(0.0);
                    for g in 0..NUM_PREY_GUILDS {
                        if (spec.animal_prey_mask & (1u16 << g)) != 0 {
                            matching_prey_sum +=
                                prey_guild_counts[idx * NUM_PREY_GUILDS + g] as u32;
                        }
                    }
                }
            }
            let mean_climate_fit = climate_fit_sum / (active_s.max(1) as f32);
            let mean_prey = (matching_prey_sum as f32) / (active_s.max(1) as f32);
            let score = mean_climate_fit * 1.5 + (mean_prey.sqrt() * 0.4) + lineage_affinity * 1.8;
            tile_class_candidates[t][apex_class].push((cand_idx, presence.season_mask, score));
        }
    }

    let mut accepted_apex_tiles: Vec<Vec<TileSeasonPresence>> =
        vec![Vec::new(); apex_candidates.len()];
    for t in 0..n {
        // Compute peak seasonal vertebrate prey richness on tile `t`
        let mut max_mammal_prey = 0u16;
        let mut max_herp_fish_prey = 0u16;
        let mut max_avian_prey = 0u16;
        let mut max_marine_prey = 0u16;
        for s in 0..num_seasons {
            let base = (s * n + t) * NUM_PREY_GUILDS;
            let mam = prey_guild_counts[base + PREY_SMALL_MAMMAL]
                + prey_guild_counts[base + PREY_LARGE_MAMMAL];
            let herp = mam
                + prey_guild_counts[base + PREY_SMALL_REPTILE]
                + prey_guild_counts[base + PREY_LARGE_REPTILE]
                + prey_guild_counts[base + PREY_SMALL_AMPHIBIAN]
                + prey_guild_counts[base + PREY_SMALL_FISH];
            let avi = mam
                + prey_guild_counts[base + PREY_SMALL_BIRD]
                + prey_guild_counts[base + PREY_SMALL_REPTILE]
                + prey_guild_counts[base + PREY_SMALL_FISH];
            let mar = prey_guild_counts[base + PREY_SMALL_FISH]
                + prey_guild_counts[base + PREY_LARGE_FISH]
                + mam;
            max_mammal_prey = max_mammal_prey.max(mam);
            max_herp_fish_prey = max_herp_fish_prey.max(herp);
            max_avian_prey = max_avian_prey.max(avi);
            max_marine_prey = max_marine_prey.max(mar);
        }

        // Emergent trophic pyramid capacity per apex class (no hardcoded biome labels):
        let class_caps: [usize; 4] = [
            ((max_mammal_prey as usize) / 4).clamp(0, 3),
            ((max_herp_fish_prey as usize) / 6).clamp(0, 2),
            ((max_avian_prey as usize) / 5).clamp(0, 3),
            ((max_marine_prey as usize) / 2).clamp(0, 3),
        ];

        for apex_class in 0..4 {
            let cap = class_caps[apex_class];
            if cap == 0 || tile_class_candidates[t][apex_class].is_empty() {
                continue;
            }
            tile_class_candidates[t][apex_class]
                .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
            for &(cand_idx, season_mask, _) in
                tile_class_candidates[t][apex_class].iter().take(cap)
            {
                accepted_apex_tiles[cand_idx].push(TileSeasonPresence {
                    tile: t,
                    season_mask,
                });
            }
        }
    }

    for ((spec, _), mut tile_seasons) in apex_candidates.into_iter().zip(accepted_apex_tiles) {
        if tile_seasons.is_empty() {
            continue;
        }
        tile_seasons.sort_by_key(|p| p.tile);
        let sp_id = settled.len();
        for &presence in &tile_seasons {
            tile_all_species[presence.tile].push(sp_id);
            for s in 0..num_seasons {
                if (presence.season_mask & (1u8 << s)) != 0 {
                    seasonal_animal_counts[s][presence.tile] =
                        seasonal_animal_counts[s][presence.tile].saturating_add(1);
                }
            }
        }
        settled.push(SettledSpecies {
            id: sp_id,
            kind: SpeciesKind::Animal,
            subkind: spec.kind_str.to_string(),
            trophic_level: 4,
            sealife: spec.sealife,
            name: spec.name.to_string(),
            tile_seasons,
        });
    }

    SettledBiosphere {
        geography,
        species: settled,
        seasonal_animal_counts,
        seasonal_plant_counts,
        tile_all_species,
    }
}

/// Standalone Lifeforms sandbox (`lifeformssimulation.py`, `lifeformsdisplay.py`, `lifeformspresenter.py`).
pub struct LifeformsSandbox {
    grid: SphericalGrid,
    tree: PointTree,
    elevations: Vec<f32>,
    felsities: Vec<f32>,
    climate_model: ClimateModel,
    biosphere: SettledBiosphere,
    custom_geoticks: Option<u32>,
    custom_land_percent: Option<u32>,
}

impl LifeformsSandbox {
    pub fn new_earth(glaciation: f64) -> Self {
        let grid = SphericalGrid::with_size(6);
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let elevations: Vec<f32> = EARTH_SIZE6_ELEVATIONS_M
            .iter()
            .map(|&m| {
                if m > 0 {
                    ((m as f32) / 900.0).min(10.0)
                } else {
                    0.0
                }
            })
            .collect();
        let felsities = vec![0.6f32; elevations.len()];
        let climate_model =
            ClimateModel::with_elevations(grid.clone(), elevations.clone(), 3, glaciation);
        let biosphere = settle_species_on_climate(&elevations, &climate_model);

        Self {
            grid,
            tree,
            elevations,
            felsities,
            climate_model,
            biosphere,
            custom_geoticks: None,
            custom_land_percent: None,
        }
    }

    pub fn load_earth(&mut self) {
        *self = Self::new_earth(self.climate_model.glaciation());
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        let Some(tec_sim) = TectonicsSimulation::from_snapshot(&snap, 42) else {
            return false;
        };
        let grid = tec_sim.grid().clone();
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let elevations: Vec<f32> = tec_sim
            .tiles()
            .iter()
            .map(|t| t.elevation().max(0.0))
            .collect();
        let felsities: Vec<f32> = tec_sim.tiles().iter().map(|t| t.top_felsity()).collect();
        let glaciation = self.climate_model.glaciation();
        let climate_model =
            ClimateModel::with_elevations(grid.clone(), elevations.clone(), 3, glaciation);
        let biosphere = settle_species_on_climate(&elevations, &climate_model);

        self.grid = grid;
        self.tree = tree;
        self.elevations = elevations;
        self.felsities = felsities;
        self.climate_model = climate_model;
        self.biosphere = biosphere;
        self.custom_geoticks = Some(tec_sim.geoticks());
        self.custom_land_percent = Some(tec_sim.land_percent());
        true
    }

    pub fn set_glaciation(&mut self, glaciation: f64) {
        let g = glaciation.clamp(0.0, 1.0);
        if (self.climate_model.glaciation() - g).abs() < 1e-6 {
            return;
        }
        self.climate_model
            .set_params(self.climate_model.cells(), g);
        self.biosphere = settle_species_on_climate(&self.elevations, &self.climate_model);
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.elevations.len()
    }

    pub fn season_count(&self) -> usize {
        self.climate_model.season_count()
    }

    pub fn total_settled_species(&self) -> usize {
        self.biosphere.species.len()
    }

    pub fn settled_animal_count(&self) -> usize {
        self.biosphere
            .species
            .iter()
            .filter(|s| s.kind == SpeciesKind::Animal)
            .count()
    }

    pub fn settled_plant_count(&self) -> usize {
        self.biosphere
            .species
            .iter()
            .filter(|s| s.kind == SpeciesKind::Plant)
            .count()
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

    pub fn tile_elevations(&self) -> &[f32] {
        &self.elevations
    }

    pub fn tile_felsities(&self) -> &[f32] {
        &self.felsities
    }

    pub fn tile_koeppen_ids(&self) -> Vec<u8> {
        self.climate_model.koeppen_ids()
    }

    pub fn tile_animal_counts(&self, season_idx: usize) -> &[u16] {
        let s = season_idx.min(self.biosphere.seasonal_animal_counts.len().saturating_sub(1));
        &self.biosphere.seasonal_animal_counts[s]
    }

    pub fn tile_plant_counts(&self, season_idx: usize) -> &[u16] {
        let s = season_idx.min(self.biosphere.seasonal_plant_counts.len().saturating_sub(1));
        &self.biosphere.seasonal_plant_counts[s]
    }

    pub fn nearest_tile(&self, x: f64, y: f64, z: f64) -> usize {
        self.tree.nearest(normal([x, y, z]))
    }

    pub fn geography(&self) -> &PlanetaryGeography {
        &self.biosphere.geography
    }

    pub fn tile_region_name(&self, tile_idx: usize) -> String {
        if let Some(cname) = self.biosphere.geography.continent_name(tile_idx) {
            format!("Continent: {cname}")
        } else if let Some(oname) = self.biosphere.geography.ocean_name(tile_idx) {
            format!("Ocean: {oname}")
        } else {
            String::new()
        }
    }

    /// Return a newline-separated list of `"<species_id>\t<kind>\t<name>"` for all settled species
    /// present on `tile_idx` during `season_idx` matching `attribute` (`"life"`, `"animals"`, `"plants"`).
    pub fn species_on_tile_tsv(&self, tile_idx: usize, season_idx: usize, attribute: &str) -> String {
        if tile_idx >= self.elevations.len() {
            return String::new();
        }
        let s_bit = 1u8 << (season_idx.min(7) as u8);
        let include_animals = attribute != "plants";
        let include_plants = attribute != "animals";

        let mut list: Vec<&SettledSpecies> = self.biosphere.tile_all_species[tile_idx]
            .iter()
            .filter_map(|&sp_id| {
                let sp = &self.biosphere.species[sp_id];
                let kind_ok = match sp.kind {
                    SpeciesKind::Animal => include_animals,
                    SpeciesKind::Plant => include_plants,
                };
                if !kind_ok {
                    return None;
                }
                let active_in_season = sp
                    .tile_seasons
                    .binary_search_by_key(&tile_idx, |p| p.tile)
                    .ok()
                    .is_some_and(|idx| (sp.tile_seasons[idx].season_mask & s_bit) != 0);
                active_in_season.then_some(sp)
            })
            .collect();

        list.sort_by(|a, b| a.name.cmp(&b.name));
        list.into_iter()
            .map(|sp| {
                let kind_code = match sp.kind {
                    SpeciesKind::Animal => "A",
                    SpeciesKind::Plant => "P",
                };
                format!("{}\t{}\t{}\n", sp.id, kind_code, sp.name)
            })
            .collect()
    }

    /// Return a per-tile `u8` presence mask (`1` if `species_id` is present on tile `t` in `season_idx`, else `0`).
    pub fn species_presence_mask(&self, species_id: usize, season_idx: usize) -> Vec<u8> {
        let mut mask = vec![0u8; self.elevations.len()];
        let Some(sp) = self.biosphere.species.get(species_id) else {
            return mask;
        };
        let s_bit = 1u8 << (season_idx.min(7) as u8);
        for &presence in &sp.tile_seasons {
            if (presence.season_mask & s_bit) != 0 {
                mask[presence.tile] = 1;
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifeforms_sandbox_earth() {
        let sim = LifeformsSandbox::new_earth(0.5);
        assert_eq!(sim.tile_count(), 7292);
        assert!(
            sim.settled_animal_count() > 200,
            "Expected >200 settled animal species on Earth, got {}",
            sim.settled_animal_count()
        );
        assert!(
            sim.settled_plant_count() > 300,
            "Expected >300 settled plant species on Earth, got {}",
            sim.settled_plant_count()
        );
    }

    #[test]
    fn test_canonical_species_tables_have_no_earth_toponyms() {
        let banned_prefixes = [
            "african ",
            "american ",
            "north american ",
            "south american ",
            "asian ",
            "asiatic ",
            "australian ",
            "european ",
            "eurasian ",
            "indian ",
            "brazilian ",
            "bolivian ",
            "mexican ",
            "canadian ",
            "egyptian ",
            "galapagos ",
            "madagascar ",
            "tasmanian ",
        ];
        for (label, tsv) in [("animals.tsv", ANIMALS_TSV), ("plants.tsv", PLANTS_TSV)] {
            for line in tsv.lines() {
                let Some((name, _)) = line.split_once('\t') else {
                    continue;
                };
                for prefix in banned_prefixes {
                    assert!(
                        !name.starts_with(prefix),
                        "{label} still contains Earth-named species '{name}' with prefix '{prefix}'"
                    );
                }
            }
        }
    }

    #[test]
    fn test_trophic_food_web_and_apex_predator_competition() {
        let sim = LifeformsSandbox::new_earth(0.5);
        let bio = &sim.biosphere;
        let level_1 = bio.species.iter().filter(|s| s.trophic_level == 1).count();
        let level_2 = bio.species.iter().filter(|s| s.trophic_level == 2).count();
        let level_3 = bio.species.iter().filter(|s| s.trophic_level == 3).count();
        let level_4 = bio.species.iter().filter(|s| s.trophic_level == 4).count();
        assert!(level_1 > 300, "Expected >300 plants (trophic level 1), got {level_1}");
        assert!(level_2 > 100, "Expected >100 herbivores/insects (trophic level 2), got {level_2}");
        assert!(level_3 > 50, "Expected >50 mesopredators/omnivores (trophic level 3), got {level_3}");
        assert!(level_4 > 15, "Expected >15 apex predators (trophic level 4), got {level_4}");

        // Verify that on every tile, the number of mammalian apex predators is bounded by emergent prey capacity (<= 3)
        // and that every tile with an apex predator also has lower-trophic animal prey present.
        for t in 0..sim.tile_count() {
            let mut mam_apex_on_tile = 0usize;
            let mut any_apex_on_tile = 0usize;
            let mut non_apex_animals_on_tile = 0usize;
            for &sp_id in &bio.tile_all_species[t] {
                let sp = &bio.species[sp_id];
                if sp.kind == SpeciesKind::Animal {
                    if sp.trophic_level == 4 {
                        any_apex_on_tile += 1;
                        if !sp.sealife && (sp.subkind == "large mammal" || sp.subkind == "giant mammal") {
                            mam_apex_on_tile += 1;
                        }
                    } else {
                        non_apex_animals_on_tile += 1;
                    }
                }
            }
            assert!(
                mam_apex_on_tile <= 3,
                "Tile {t} has {mam_apex_on_tile} mammalian apex predators (expected <= 3)"
            );
            if any_apex_on_tile > 0 {
                assert!(
                    non_apex_animals_on_tile >= 2,
                    "Tile {t} has {any_apex_on_tile} apex predators but only {non_apex_animals_on_tile} prey species"
                );
            }
        }
    }
}
