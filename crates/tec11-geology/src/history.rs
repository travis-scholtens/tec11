use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

use tec11_math_grid::{
    normal, DisplayFaceSource, DisplayMesh, FineParentage, Pcg64Mcg, PointTree, SphericalGrid,
    SubdividedTerrainGrid, Vec3,
};
use tec11_proto::PlanetSnapshotExt;

use crate::language::{Language, SoundChangeKind, Word, WordOrigin};
use crate::lifeforms::{SettledSpecies, SpeciesKind};
use crate::population::{
    is_farmable, is_habitable, is_passable, main_ocean, tile_population_thousands,
    PlanetaryGeography,
};
use crate::prehistory::PrehistorySimulation;
use crate::rivers::trace_coarse_rivers;
use crate::sun_sky::{
    generate_default_constellations, generate_solar_system_with_rng, AtmosphereType, Constellation,
    HomeStar, PlanetaryOptics, SolarSystemPlanet,
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

pub const BASIC_WORD_GLOSSES: [&str; 16] = [
    "mother",
    "father",
    "child",
    "person",
    "water",
    "fire",
    "earth / soil",
    "stone",
    "sky",
    "night",
    "day",
    "hand",
    "eye",
    "heart",
    "blood",
    "name",
];

pub const CULTURE_FORAGING: u32 = 0;
pub const CULTURE_FARMING: u32 = 1;
pub const CULTURE_RIVER_FISHING: u32 = 2;
pub const CULTURE_SEA_FISHING: u32 = 3;
pub const CULTURE_TRADING: u32 = 4;
pub const CULTURE_WARFARE: u32 = 5;
pub const CULTURE_CITY: u32 = 6;

pub const CULTURE_CONCEPT_GLOSSES: [&str; 7] = [
    "foraging / gathering",
    "farming / agriculture",
    "river fishing / boating",
    "sea fishing / seafaring",
    "trading / commerce",
    "warfare / battle",
    "city / citadel",
];

pub const SEASON_SUMMER: u32 = 0;
pub const SEASON_WINTER: u32 = 1;
pub const SEASON_SPRING: u32 = 2;
pub const SEASON_AUTUMN: u32 = 3;
pub const SEASON_MONSOON: u32 = 4;
pub const SEASON_WET: u32 = 5;
pub const SEASON_DRY: u32 = 6;

pub const SEASON_CONCEPT_GLOSSES: [&str; 7] = [
    "summer (warm season)",
    "winter (cold season)",
    "spring (growing season)",
    "autumn (harvest season)",
    "monsoon (rainy season)",
    "wet season (savanna rains)",
    "dry season",
];

pub const WEATHER_WIND: u32 = 0;
pub const WEATHER_CLOUD: u32 = 1;
pub const WEATHER_RAIN: u32 = 2;
pub const WEATHER_THUNDERSTORM: u32 = 3;
pub const WEATHER_DROUGHT: u32 = 4;
pub const WEATHER_SANDSTORM: u32 = 5;
pub const WEATHER_FROST: u32 = 6;
pub const WEATHER_SNOW: u32 = 7;
pub const WEATHER_ICE: u32 = 8;

pub const WEATHER_CONCEPT_GLOSSES: [&str; 9] = [
    "wind",
    "cloud / mist",
    "rain",
    "thunderstorm",
    "drought / parched heat",
    "sandstorm / dust wind",
    "frost",
    "snow",
    "ice / glacier",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConceptKey {
    BasicWord(u32),
    Culture(u32),
    Season(u32),
    Weather(u32),
    Sun(u32),
    Moon(u32),
    Planet(u32),
    Constellation(u32),
    Continent(u32),
    Ocean(u32),
    Sea(u32),
    River(u32),
    MountainRange(u32),
    Species(u32),
    Nation(u32),
    State(u32),
    Language(u32),
}

impl ConceptKey {
    pub fn to_proto(self) -> tec11_proto::Concept {
        match self {
            Self::BasicWord(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::BasicWord as i32,
                id,
            },
            Self::Culture(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Culture as i32,
                id,
            },
            Self::Season(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Season as i32,
                id,
            },
            Self::Weather(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Weather as i32,
                id,
            },
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
            Self::Sea(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::Sea as i32,
                id,
            },
            Self::River(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::River as i32,
                id,
            },
            Self::MountainRange(id) => tec11_proto::Concept {
                kind: tec11_proto::ConceptKind::MountainRange as i32,
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
            tec11_proto::ConceptKind::BasicWord => Self::BasicWord(c.id),
            tec11_proto::ConceptKind::Culture => Self::Culture(c.id),
            tec11_proto::ConceptKind::Season => Self::Season(c.id),
            tec11_proto::ConceptKind::Weather => Self::Weather(c.id),
            tec11_proto::ConceptKind::Sun => Self::Sun(c.id),
            tec11_proto::ConceptKind::Moon => Self::Moon(c.id),
            tec11_proto::ConceptKind::Planet => Self::Planet(c.id),
            tec11_proto::ConceptKind::Constellation => Self::Constellation(c.id),
            tec11_proto::ConceptKind::Continent => Self::Continent(c.id),
            tec11_proto::ConceptKind::Ocean => Self::Ocean(c.id),
            tec11_proto::ConceptKind::Sea => Self::Sea(c.id),
            tec11_proto::ConceptKind::River => Self::River(c.id),
            tec11_proto::ConceptKind::MountainRange => Self::MountainRange(c.id),
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
    pub rivergoing: bool,
    pub seafaring: bool,
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
        self.rivergoing = self.rivergoing || other.rivergoing;
        self.seafaring = self.seafaring || other.seafaring;
        self.racial_mix = new_mix;
        self.normalize_racial_mix();
    }
}

pub fn format_thousands_compact(thousands: f32) -> String {
    if thousands >= 1000.0 {
        format!("{:.2}M", thousands / 1000.0)
    } else if thousands >= 10.0 {
        format!("{:.1}K", thousands)
    } else if thousands > 1e-4 {
        format!("{:.2}K", thousands)
    } else {
        "0".to_string()
    }
}

pub fn format_share_percent(thousands: f32, total: f32) -> String {
    if thousands <= 1e-4 || total <= 1e-4 {
        return "0%".to_string();
    }
    let pct = (thousands * 100.0 / total).clamp(0.0, 100.0);
    if pct >= 1.0 {
        format!("{:.0}%", pct)
    } else if pct >= 0.1 {
        format!("{:.1}%", pct)
    } else {
        "<0.1%".to_string()
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_str_array(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| json_escape(s)).collect();
    format!("[{}]", inner.join(","))
}

/// Clickable demographic share item (nationality or language) with population and percentage share.
#[derive(Clone, Debug, PartialEq)]
pub struct DemographicShareItem {
    pub id: u32,
    pub name: String,
    pub thousands: f32,
    pub percent_text: String,
}

impl DemographicShareItem {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"id\":{},\"name\":{},\"popText\":{},\"percentText\":{}}}",
            self.id,
            json_escape(&self.name),
            json_escape(&format_thousands_compact(self.thousands)),
            json_escape(&self.percent_text)
        )
    }
}

/// Projected 2D polygon for the zoomed-in, centered map of a state.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoomedMapPolygon {
    pub display_idx: usize,
    pub fine_idx: usize,
    pub state_id: Option<u32>,
    pub points_xy: Vec<[f32; 2]>,
    pub rgb: [f32; 3],
    pub is_in_selected_state: bool,
    pub is_selected_tile: bool,
    pub is_capital: bool,
    pub is_river: bool,
}

impl ZoomedMapPolygon {
    pub fn to_json(&self) -> String {
        let pts: Vec<String> = self
            .points_xy
            .iter()
            .map(|&[x, y]| format!("[{x:.4},{y:.4}]"))
            .collect();
        let sid_val = self.state_id.map(|s| s as i32).unwrap_or(-1);
        format!(
            "{{\"displayIdx\":{},\"fineIdx\":{},\"stateId\":{},\"points\":[{}],\"rgb\":[{:.3},{:.3},{:.3}],\"isInSelectedState\":{},\"isSelectedTile\":{},\"isCapital\":{},\"isRiver\":{}}}",
            self.display_idx,
            self.fine_idx,
            sid_val,
            pts.join(","),
            self.rgb[0],
            self.rgb[1],
            self.rgb[2],
            self.is_in_selected_state,
            self.is_selected_tile,
            self.is_capital,
            self.is_river
        )
    }
}

/// Level-1 State inspection view (zoomed-in centered state map + state demographics, economy & diplomacy).
#[derive(Clone, Debug, PartialEq)]
pub struct StateInspectionView {
    pub state_id: u32,
    pub state_name: String,
    pub color_rgb: [f32; 3],
    pub continents: String,
    pub technology: String,
    pub population_text: String,
    pub capacity_text: String,
    pub tile_count: usize,
    pub asabiya_text: String,
    pub nationalities: Vec<DemographicShareItem>,
    pub languages: Vec<DemographicShareItem>,
    pub exports: Vec<String>,
    pub imports: Vec<String>,
    pub trade_partners: Vec<String>,
    pub conflicts: Vec<String>,
    pub water_routes: Vec<String>,
    pub map_polygons: Vec<ZoomedMapPolygon>,
}

impl StateInspectionView {
    pub fn to_json(&self) -> String {
        let nats: Vec<String> = self.nationalities.iter().map(|n| n.to_json()).collect();
        let langs: Vec<String> = self.languages.iter().map(|l| l.to_json()).collect();
        let polys: Vec<String> = self.map_polygons.iter().map(|p| p.to_json()).collect();
        let r255 = (self.color_rgb[0] * 255.0).round().clamp(0.0, 255.0) as u8;
        let g255 = (self.color_rgb[1] * 255.0).round().clamp(0.0, 255.0) as u8;
        let b255 = (self.color_rgb[2] * 255.0).round().clamp(0.0, 255.0) as u8;
        let color_css = format!("rgb({r255}, {g255}, {b255})");
        format!(
            "{{\"hasState\":true,\"stateId\":{},\"stateName\":{},\"colorCss\":{},\"continents\":{},\"technology\":{},\"populationText\":{},\"capacityText\":{},\"tileCount\":{},\"asabiyaText\":{},\"nationalities\":[{}],\"languages\":[{}],\"exports\":{},\"imports\":{},\"tradePartners\":{},\"conflicts\":{},\"waterRoutes\":{},\"mapPolygons\":[{}]}}",
            self.state_id,
            json_escape(&self.state_name),
            json_escape(&color_css),
            json_escape(&self.continents),
            json_escape(&self.technology),
            json_escape(&self.population_text),
            json_escape(&self.capacity_text),
            self.tile_count,
            json_escape(&self.asabiya_text),
            nats.join(","),
            langs.join(","),
            json_str_array(&self.exports),
            json_str_array(&self.imports),
            json_str_array(&self.trade_partners),
            json_str_array(&self.conflicts),
            json_str_array(&self.water_routes),
            polys.join(",")
        )
    }
}

/// Ephemeris parameters for a moon visible from a clicked tile's local sky.
#[derive(Clone, Debug, PartialEq)]
pub struct TileSkyMoon {
    pub index: u32,
    pub name: String,
    pub period_days: f32,
    pub inclination_deg: f32,
    pub init_angle_rad: f32,
    pub node_asc_rad: f32,
}

impl TileSkyMoon {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"index\":{},\"name\":{},\"periodDays\":{:.4},\"inclinationDeg\":{:.3},\"initAngleRad\":{:.4},\"nodeAscRad\":{:.4}}}",
            self.index,
            json_escape(&self.name),
            self.period_days,
            self.inclination_deg,
            self.init_angle_rad,
            self.node_asc_rad
        )
    }
}

/// Ephemeris parameters for a solar-system planet visible from a clicked tile's local sky.
#[derive(Clone, Debug, PartialEq)]
pub struct TileSkyPlanet {
    pub index: u32,
    pub name: String,
    pub orbit_au: f32,
    pub period_days: f32,
    pub init_angle_rad: f32,
    pub inclination_deg: f32,
    pub rgb: [f32; 3],
    pub has_rings: bool,
}

impl TileSkyPlanet {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"index\":{},\"name\":{},\"orbitAu\":{:.4},\"periodDays\":{:.4},\"initAngleRad\":{:.4},\"inclinationDeg\":{:.3},\"rgb\":[{:.3},{:.3},{:.3}],\"hasRings\":{}}}",
            self.index,
            json_escape(&self.name),
            self.orbit_au,
            self.period_days,
            self.init_angle_rad,
            self.inclination_deg,
            self.rgb[0],
            self.rgb[1],
            self.rgb[2],
            if self.has_rings { "true" } else { "false" }
        )
    }
}

/// Constellation asterism figure for a clicked tile's local sky.
#[derive(Clone, Debug, PartialEq)]
pub struct TileSkyConstellation {
    pub index: u32,
    pub name: String,
    pub ra_deg: f32,
    pub dec_deg: f32,
    pub stars: Vec<[f32; 3]>,
    pub edges: Vec<[usize; 2]>,
}

impl TileSkyConstellation {
    pub fn to_json(&self) -> String {
        let stars_json: Vec<String> = self
            .stars
            .iter()
            .map(|s| format!("[{:.2},{:.2},{:.2}]", s[0], s[1], s[2]))
            .collect();
        let edges_json: Vec<String> = self
            .edges
            .iter()
            .map(|e| format!("[{},{}]", e[0], e[1]))
            .collect();
        format!(
            "{{\"index\":{},\"name\":{},\"raDeg\":{:.2},\"decDeg\":{:.2},\"stars\":[{}],\"edges\":[{}]}}",
            self.index,
            json_escape(&self.name),
            self.ra_deg,
            self.dec_deg,
            stars_json.join(","),
            edges_json.join(",")
        )
    }
}

/// Complete local 2D sky view configuration and ephemeris for a clicked tile (`display_idx`).
#[derive(Clone, Debug, PartialEq)]
pub struct TileSkyView {
    pub lat_deg: f32,
    pub lon_deg: f32,
    pub tilt_deg: f32,
    pub days_per_year: u32,
    pub day_hours: f32,
    pub current_history_year: u32,
    pub sun_name: String,
    pub sun_rgb: [f32; 3],
    pub sky_zenith_rgb: [f32; 3],
    pub sky_horizon_rgb: [f32; 3],
    pub sunset_rgb: [f32; 3],
    pub twilight_rgb: [f32; 3],
    pub home_orbit_au: f32,
    pub home_period_days: f32,
    pub home_init_angle_rad: f32,
    pub moons: Vec<TileSkyMoon>,
    pub planets: Vec<TileSkyPlanet>,
    pub constellations: Vec<TileSkyConstellation>,
}

impl TileSkyView {
    pub fn to_json(&self) -> String {
        let moons_json: Vec<String> = self.moons.iter().map(|m| m.to_json()).collect();
        let planets_json: Vec<String> = self.planets.iter().map(|p| p.to_json()).collect();
        let consts_json: Vec<String> = self.constellations.iter().map(|c| c.to_json()).collect();
        format!(
            "{{\"latDeg\":{:.2},\"lonDeg\":{:.2},\"tiltDeg\":{:.2},\"daysPerYear\":{},\"dayHours\":{:.2},\"currentHistoryYear\":{},\"sunName\":{},\"sunRgb\":[{:.3},{:.3},{:.3}],\"skyZenithRgb\":[{:.3},{:.3},{:.3}],\"skyHorizonRgb\":[{:.3},{:.3},{:.3}],\"sunsetRgb\":[{:.3},{:.3},{:.3}],\"twilightRgb\":[{:.3},{:.3},{:.3}],\"homeOrbitAu\":{:.4},\"homePeriodDays\":{:.4},\"homeInitAngleRad\":{:.4},\"moons\":[{}],\"planets\":[{}],\"constellations\":[{}]}}",
            self.lat_deg,
            self.lon_deg,
            self.tilt_deg,
            self.days_per_year,
            self.day_hours,
            self.current_history_year,
            json_escape(&self.sun_name),
            self.sun_rgb[0],
            self.sun_rgb[1],
            self.sun_rgb[2],
            self.sky_zenith_rgb[0],
            self.sky_zenith_rgb[1],
            self.sky_zenith_rgb[2],
            self.sky_horizon_rgb[0],
            self.sky_horizon_rgb[1],
            self.sky_horizon_rgb[2],
            self.sunset_rgb[0],
            self.sunset_rgb[1],
            self.sunset_rgb[2],
            self.twilight_rgb[0],
            self.twilight_rgb[1],
            self.twilight_rgb[2],
            self.home_orbit_au,
            self.home_period_days,
            self.home_init_angle_rad,
            moons_json.join(","),
            planets_json.join(","),
            consts_json.join(",")
        )
    }
}

/// Level-2 Tile inspection view (individual tile's nationalities, languages, native flora & fauna, and local 2D sky view).
#[derive(Clone, Debug, PartialEq)]
pub struct TileInspectionView {
    pub display_idx: usize,
    pub fine_idx: Option<usize>,
    pub state_id: Option<u32>,
    pub state_name: Option<String>,
    pub title: String,
    pub location_summary: String,
    pub population_summary: String,
    pub technology: String,
    pub nationalities: Vec<DemographicShareItem>,
    pub languages: Vec<DemographicShareItem>,
    pub flora: Vec<String>,
    pub fauna: Vec<String>,
    pub sky: TileSkyView,
}

impl TileInspectionView {
    pub fn to_json(&self) -> String {
        let nats: Vec<String> = self.nationalities.iter().map(|n| n.to_json()).collect();
        let langs: Vec<String> = self.languages.iter().map(|l| l.to_json()).collect();
        format!(
            "{{\"hasTile\":true,\"displayIdx\":{},\"stateId\":{},\"title\":{},\"locationSummary\":{},\"populationSummary\":{},\"technology\":{},\"nationalities\":[{}],\"languages\":[{}],\"flora\":{},\"fauna\":{},\"sky\":{}}}",
            self.display_idx,
            self.state_id.map(|s| s as i32).unwrap_or(-1),
            json_escape(&self.title),
            json_escape(&self.location_summary),
            json_escape(&self.population_summary),
            json_escape(&self.technology),
            nats.join(","),
            langs.join(","),
            json_str_array(&self.flora),
            json_str_array(&self.fauna),
            self.sky.to_json()
        )
    }
}

/// Single lexicon word entry for a language inspection view.
#[derive(Clone, Debug, PartialEq)]
pub struct LexiconEntryView {
    pub category: String,
    pub gloss: String,
    pub orthography: String,
    pub ipa: String,
    pub etymology: String,
}

impl LexiconEntryView {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"category\":{},\"gloss\":{},\"orthography\":{},\"ipa\":{},\"etymology\":{}}}",
            json_escape(&self.category),
            json_escape(&self.gloss),
            json_escape(&self.orthography),
            json_escape(&self.ipa),
            json_escape(&self.etymology)
        )
    }
}

/// Detailed Language inspection view (full lexicon, who speaks it, and where).
#[derive(Clone, Debug, PartialEq)]
pub struct LanguageDetailView {
    pub lang_id: u32,
    pub name: String,
    pub total_speakers_text: String,
    pub phoneme_summary: String,
    pub speakers_by_location: Vec<String>,
    pub speakers_by_nation: Vec<DemographicShareItem>,
    pub lexicon: Vec<LexiconEntryView>,
}

impl LanguageDetailView {
    pub fn to_json(&self) -> String {
        let nats: Vec<String> = self.speakers_by_nation.iter().map(|n| n.to_json()).collect();
        let lex: Vec<String> = self.lexicon.iter().map(|e| e.to_json()).collect();
        format!(
            "{{\"hasLanguage\":true,\"langId\":{},\"name\":{},\"totalSpeakersText\":{},\"phonemeSummary\":{},\"speakersByLocation\":{},\"speakersByNation\":[{}],\"lexicon\":[{}]}}",
            self.lang_id,
            json_escape(&self.name),
            json_escape(&self.total_speakers_text),
            json_escape(&self.phoneme_summary),
            json_str_array(&self.speakers_by_location),
            nats.join(","),
            lex.join(",")
        )
    }
}

/// Detailed Nationality inspection view (where they originate, where they currently live, and what they speak).
#[derive(Clone, Debug, PartialEq)]
pub struct NationDetailView {
    pub nat_id: u32,
    pub name: String,
    pub total_pop_text: String,
    pub origin_summary: String,
    pub heritage_summary: String,
    pub current_locations: Vec<String>,
    pub spoken_languages: Vec<DemographicShareItem>,
}

impl NationDetailView {
    pub fn to_json(&self) -> String {
        let langs: Vec<String> = self.spoken_languages.iter().map(|l| l.to_json()).collect();
        format!(
            "{{\"hasNation\":true,\"natId\":{},\"name\":{},\"totalPopText\":{},\"originSummary\":{},\"heritageSummary\":{},\"currentLocations\":{},\"spokenLanguages\":[{}]}}",
            self.nat_id,
            json_escape(&self.name),
            json_escape(&self.total_pop_text),
            json_escape(&self.origin_summary),
            json_escape(&self.heritage_summary),
            json_str_array(&self.current_locations),
            langs.join(",")
        )
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
        // Universal basic vocabulary present in every language (mother, father, water, fire, etc.)
        for bw_id in 0..(BASIC_WORD_GLOSSES.len() as u32) {
            rec.coin_concept(ConceptKey::BasicWord(bw_id), rng);
        }
        // Baseline cultural concepts present at city founding
        rec.coin_concept(ConceptKey::Culture(CULTURE_FORAGING), rng);
        rec.coin_concept(ConceptKey::Culture(CULTURE_CITY), rng);
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
        self.phonology
            .refresh_from_word_iter(self.lexicon.values().map(|o| &o.word));
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
                if let Some(candidate) = change.apply_opt(&origin.word) {
                    if !self.used_words.contains(&candidate) {
                        self.used_words.remove(&origin.word);
                        self.used_wordkeys.insert(candidate.wordkey());
                        self.used_words.insert(candidate.clone());
                        let prev_origin = std::mem::replace(
                            origin,
                            WordOrigin::with_arc_parent(candidate, my_name.clone(), None),
                        );
                        origin.parent = Some(std::sync::Arc::new(prev_origin));
                    }
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
    /// Hypothetical / exploratory wind or current route from a populated port whose destination is unclaimed/unpopulated.
    HypotheticalRoute,
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
            Self::HypotheticalRoute => [0.58, 0.46, 0.88],
            Self::PotentialCorridor => [0.32, 0.42, 0.66],
        }
    }

    pub fn radius_lift(self) -> f32 {
        match self {
            Self::MaritimeTrade | Self::NavalConflict => 1.010,
            Self::ActiveSeaLane => 1.008,
            Self::HypotheticalRoute => 1.006,
            Self::PotentialCorridor => 1.004,
        }
    }

    pub fn ocean_sector_label(self) -> &'static str {
        match self {
            Self::MaritimeTrade => "TRADE ROUTE (yellow/gold)",
            Self::NavalConflict => "NAVAL CONFLICT (red)",
            Self::ActiveSeaLane => "CONNECTED SEA LANE (cyan)",
            Self::HypotheticalRoute => "HYPOTHETICAL / UNCONNECTED DRIFT (violet)",
            Self::PotentialCorridor => "Potential Wind/Current Corridor (steel-blue)",
        }
    }

    pub fn land_label(self) -> &'static str {
        match self {
            Self::MaritimeTrade => "MARITIME TRADE (yellow)",
            Self::NavalConflict => "NAVAL CONFLICT (red)",
            Self::ActiveSeaLane => "CONNECTED SEA LANE (cyan)",
            Self::HypotheticalRoute => "HYPOTHETICAL / UNCONNECTED (violet)",
            Self::PotentialCorridor => "POTENTIAL CORRIDOR",
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
    home_star: HomeStar,
    optics: PlanetaryOptics,
    solar_system: Vec<SolarSystemPlanet>,
    constellations: Vec<Constellation>,
    geography: PlanetaryGeography,
    settled_species: Vec<SettledSpecies>,
    tile_species: Vec<Vec<usize>>,
    rivers: Vec<Vec<usize>>,
    river_routes: Vec<Vec<usize>>,
    river_tiles: Vec<bool>,
    fine_face_rivers: Vec<Vec<u32>>,
    is_coastal_fine: Vec<bool>,
    fine_adj: std::sync::Arc<Vec<Vec<usize>>>,
    coarse_elevs: Vec<f32>,
    coarse_koeppen: Vec<u8>,
    fine_primary_coarse: Vec<usize>,
    fine_lat_deg: Vec<f64>,
    base_capacities: Vec<f32>,
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
        // Ensure prehistory has populated technological tiles if starting directly on Earth or an unpopulated planet.
        if prehistory.active_agricultural_race_count() == 0
            && prehistory.active_rivergoing_race_count() == 0
            && prehistory.active_seafaring_race_count() == 0
        {
            for _ in 0..8 {
                prehistory.step();
                if (prehistory.active_agricultural_race_count() > 0
                    || prehistory.active_rivergoing_race_count() > 0
                    || prehistory.active_seafaring_race_count() > 0)
                    && prehistory.populated_count() > 80
                {
                    break;
                }
            }
        }

        let _span_subdiv = crate::profile::span(crate::profile::ProfilePhase::HistInitSubdivideAndKdTree);
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
        drop(_span_subdiv);

        let home_star = HomeStar::sol();
        let optics = PlanetaryOptics::compute(&home_star, AtmosphereType::EarthLikeN2O2);
        let fine_adj = std::sync::Arc::new(terrain.terrain_adj.clone());
        let mut sim = Self {
            prehistory,
            terrain,
            tree,
            history_ticks: 0,
            moon_count,
            home_star,
            optics,
            solar_system,
            constellations,
            geography: PlanetaryGeography::default(),
            settled_species: Vec::new(),
            tile_species: vec![Vec::new(); num_fine],
            rivers: Vec::new(),
            river_routes: Vec::new(),
            river_tiles: vec![false; num_fine],
            fine_face_rivers: vec![Vec::new(); num_fine],
            is_coastal_fine: vec![false; num_fine],
            fine_adj,
            coarse_elevs,
            coarse_koeppen: Vec::new(),
            fine_primary_coarse: vec![0; num_fine],
            fine_lat_deg: vec![0.0; num_fine],
            base_capacities: vec![0.0; num_fine],
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
        let coarse_adj = self.prehistory.adjacency().to_vec();
        let coarse_elevs = self.prehistory.tile_elevations();
        let koeppen = self.prehistory.tile_koeppen_ids().to_vec();
        let precip = self.prehistory.tile_precip_norm().to_vec();
        let num_coarse = coarse_elevs.len();
        let num_fine = self.terrain.terrain_grid.faces.len();
        let fine_adj = std::sync::Arc::clone(&self.fine_adj);
        self.coarse_elevs = coarse_elevs.clone();
        self.coarse_koeppen = koeppen.clone();
        self.fine_primary_coarse = (0..num_fine)
            .map(|f_idx| self.terrain.primary_coarse_tile(f_idx, &coarse_elevs))
            .collect();
        self.fine_lat_deg = self
            .terrain
            .terrain_grid
            .faces
            .iter()
            .map(|&(c, _)| c[2].clamp(-1.0, 1.0).asin() * 180.0 / std::f64::consts::PI)
            .collect();

        // 1. Identify continents and ocean basins, settle plants & animals on the planet's coarse climate,
        //    then map culturally/economically salient flora & fauna onto subdivided fine faces (`historysimulation.py:facespecies`).
        let _span_bio = crate::profile::span(crate::profile::ProfilePhase::HistInitBiosphereAndSpecies);
        let biosphere = self.prehistory.take_or_settle_biosphere(&coarse_elevs);
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
        drop(_span_bio);

        // 2. Trace coarse rivers (`hmin = 2.0`, `pmin = 0.25`) and route them on the subdivided terrain grid (`routerivers`).
        let _span_riv_cap = crate::profile::span(crate::profile::ProfilePhase::HistInitRiversAndCapacities);
        self.rivers = trace_coarse_rivers(
            &coarse_elevs,
            &precip,
            &koeppen,
            &coarse_adj,
            HISTORY_MIN_RIVER_ELEV,
            HISTORY_MIN_RIVER_PRECIP,
        );
        self.river_routes = self.terrain.route_rivers(self.prehistory.grid(), &self.rivers);
        self.river_tiles = vec![false; num_fine];
        self.fine_face_rivers = vec![Vec::new(); num_fine];
        for (rid, route) in self.river_routes.iter().enumerate() {
            let rid_u32 = rid as u32;
            for &f_idx in route {
                if f_idx < num_fine {
                    self.river_tiles[f_idx] = true;
                    if !self.fine_face_rivers[f_idx].contains(&rid_u32) {
                        self.fine_face_rivers[f_idx].push(rid_u32);
                    }
                    for &nb in &fine_adj[f_idx] {
                        if nb < num_fine && !self.fine_face_rivers[nb].contains(&rid_u32) {
                            self.fine_face_rivers[nb].push(rid_u32);
                        }
                    }
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
            if self.terrain.fine_elevations[f_idx] <= 0.0 {
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
        self.base_capacities = self.capacities.clone();
        self.is_coastal_fine = (0..num_fine)
            .map(|f_idx| {
                if self.terrain.fine_elevations[f_idx] <= 0.0 {
                    return false;
                }
                let c = self.fine_primary_coarse[f_idx];
                fine_adj[f_idx]
                    .iter()
                    .any(|&nb| self.terrain.fine_elevations[nb] <= 0.0)
                    || coarse_adj[c].iter().any(|nb| ocean.contains(nb))
            })
            .collect();

        drop(_span_riv_cap);
        let _span_seed = crate::profile::span(crate::profile::ProfilePhase::HistInitSeedStatesAndLexicons);
        // 4. Seed cities & initial states across farmable, riverine, or coastal fine faces (`historysimulation.py:stateboundaries`)
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
                let c = self.fine_primary_coarse[f_idx];
                let fe = self.terrain.fine_elevations[f_idx];
                fe > 0.0
                    && (is_farmable(fe, koeppen[c])
                        || self.river_tiles[f_idx]
                        || self.is_coastal_fine[f_idx])
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
        let pre_pop = self.prehistory.populated().clone();
        let pre_ag = self.prehistory.agricultural_heritages().clone();
        let pre_riv = self.prehistory.rivergoing_heritages().clone();
        let pre_sea = self.prehistory.seafaring_heritages().clone();
        let mut state_init_tech: BTreeMap<u32, (bool, bool, bool)> = BTreeMap::new();
        let mut q = VecDeque::new();
        for &city_f in &city_tiles {
            let sid = self.next_state_id;
            self.next_state_id += 1;
            let lid = self.next_language_id;
            self.next_language_id += 1;

            let city_c = self.fine_primary_coarse[city_f];
            let city_hid = pre_pop.get(&city_c).copied();
            let has_ag = city_hid.is_some_and(|h| pre_ag.contains(&h))
                || is_farmable(self.terrain.fine_elevations[city_f], koeppen[city_c]);
            let has_riv =
                city_hid.is_some_and(|h| pre_riv.contains(&h)) || self.river_tiles[city_f];
            let has_sea =
                city_hid.is_some_and(|h| pre_sea.contains(&h)) || self.is_coastal_fine[city_f];
            state_init_tech.insert(sid, (has_ag, has_riv, has_sea));

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
            let (_, state_riv, _) = state_init_tech.get(&sid).copied().unwrap_or((true, false, false));
            for &nb in &fine_adj[curr] {
                let c = self.fine_primary_coarse[nb];
                let step_cost = if self.river_tiles[nb] {
                    if state_riv {
                        2
                    } else {
                        10
                    }
                } else {
                    1
                };
                let nb_elev = self.terrain.fine_elevations[nb];
                if dist + step_cost <= 16
                    && !self.tile_states.contains_key(&nb)
                    && nb_elev > 0.0
                    && is_passable(nb_elev, koeppen[c], true)
                {
                    self.tile_states.insert(nb, sid);
                    if let Some(lang) = self.languages.get_mut(&sid) {
                        if lang.lexicon.len() < 48 {
                            for &sp_idx in &self.tile_species[nb] {
                                lang.coin_concept(ConceptKey::Species(sp_idx as u32), &mut self.rng);
                            }
                        }
                    }
                    q.push_back((nb, sid, dist + step_cost));
                }
            }
        }

        // Coin all constellations visible at some point in the year, local geographic features (continents, oceans,
        // seas, rivers, mountain ranges), experienced seasons & weather, and initial cultural concepts:
        let mut init_extents: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (&f_idx, &sid) in &self.tile_states {
            init_extents.entry(sid).or_default().push(f_idx);
        }
        for (sid, tiles) in &init_extents {
            let vis = self.visible_constellations_for_tiles(tiles);
            let (conts, oceans, seas, rivers, mountains) =
                self.geographic_features_for_tiles(tiles, &coarse_elevs);
            let seasons = self.seasons_for_tiles(tiles, &coarse_elevs, &koeppen);
            let weathers = self.weather_for_tiles(tiles, &coarse_elevs, &koeppen);
            let (has_ag, has_riv, has_sea) = state_init_tech
                .get(sid)
                .copied()
                .unwrap_or((false, false, false));
            let mut temp_rng = self.rng.clone();
            if let Some(lang) = self.languages.get_mut(sid) {
                if has_ag {
                    lang.coin_concept(ConceptKey::Culture(CULTURE_FARMING), &mut temp_rng);
                }
                if has_riv {
                    lang.coin_concept(ConceptKey::Culture(CULTURE_RIVER_FISHING), &mut temp_rng);
                }
                if has_sea {
                    lang.coin_concept(ConceptKey::Culture(CULTURE_SEA_FISHING), &mut temp_rng);
                }
                for s_id in seasons {
                    lang.coin_concept(ConceptKey::Season(s_id), &mut temp_rng);
                }
                for w_id in weathers {
                    lang.coin_concept(ConceptKey::Weather(w_id), &mut temp_rng);
                }
                for cid in conts {
                    lang.coin_concept(ConceptKey::Continent(cid), &mut temp_rng);
                }
                for oid in oceans {
                    lang.coin_concept(ConceptKey::Ocean(oid), &mut temp_rng);
                }
                for sea_id in seas {
                    lang.coin_concept(ConceptKey::Sea(sea_id), &mut temp_rng);
                }
                for rid in rivers {
                    lang.coin_concept(ConceptKey::River(rid), &mut temp_rng);
                }
                for mid in mountains {
                    lang.coin_concept(ConceptKey::MountainRange(mid), &mut temp_rng);
                }
                for c_idx in vis {
                    lang.coin_concept(ConceptKey::Constellation(c_idx), &mut temp_rng);
                }
                lang.refresh_phonology();
            }
            self.rng = temp_rng;
        }

        // Initialize communities on claimed fine faces (`historysimulation.py:population`, 1/9 or 1/27 shares)
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
            let (state_ag, state_riv, state_sea) = state_init_tech
                .get(&sid)
                .copied()
                .unwrap_or((true, false, false));
            let mix_ag = mix.keys().any(|h| pre_ag.contains(h));
            let mix_riv = mix.keys().any(|h| pre_riv.contains(h));
            let mix_sea = mix.keys().any(|h| pre_sea.contains(h));
            if mix.is_empty() {
                mix.insert(0, 1.0);
            }
            let c_idx = self.fine_primary_coarse[f_idx];
            let agriculture = state_ag
                || mix_ag
                || is_farmable(self.terrain.fine_elevations[f_idx], koeppen[c_idx]);
            let rivergoing = state_riv || mix_riv || self.river_tiles[f_idx];
            let seafaring = state_sea || mix_sea || self.is_coastal_fine[f_idx];
            let pop = (self.capacities[f_idx] * 0.45).max(0.15);
            let mut comm = Community {
                thousands: pop,
                nationality_id: sid,
                language_id: sid,
                agriculture,
                rivergoing,
                seafaring,
                racial_mix: mix,
            };
            comm.normalize_racial_mix();
            self.tile_communities.insert(f_idx, vec![comm]);
        }

        drop(_span_seed);

        // 5. Apply fishing carrying capacity bonuses from rivergoing/seafaring and activate water travel routes.
        {
            let _span_routes = crate::profile::span(crate::profile::ProfilePhase::HistInitTravelRoutes);
            self.refresh_fishing_capacities();
            self.compute_potential_travel_routes(&ocean, &coarse_elevs, &koeppen, &coarse_adj, &fine_adj);
            self.update_travel_routes();
        }

        let _span_init_dip = crate::profile::span(crate::profile::ProfilePhase::HistInitDiplomacyAndTrade);
        self.recolor_states(&fine_adj);
        self.update_diplomacy_and_trade(&fine_adj);
    }

    /// Recompute `self.capacities` from `self.base_capacities`, granting a `+35%` freshwater fishing
    /// carrying capacity bonus on river tiles with `rivergoing` communities and a `+45%` marine fishing
    /// carrying capacity bonus on coastal tiles with `seafaring` communities.
    pub fn refresh_fishing_capacities(&mut self) {
        let num_fine = self.base_capacities.len();
        if self.capacities.len() != num_fine {
            self.capacities = self.base_capacities.clone();
        }
        for f_idx in 0..num_fine {
            let mut mult = 1.0f32;
            if let Some(comms) = self.tile_communities.get(&f_idx) {
                let has_riv = comms.iter().any(|c| c.thousands > 0.0 && c.rivergoing);
                let has_sea = comms.iter().any(|c| c.thousands > 0.0 && c.seafaring);
                if has_riv && self.river_tiles.get(f_idx).copied().unwrap_or(false) {
                    mult *= 1.35;
                }
                if has_sea && self.is_coastal_fine.get(f_idx).copied().unwrap_or(false) {
                    mult *= 1.45;
                }
            }
            self.capacities[f_idx] = self.base_capacities[f_idx] * mult;
        }
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
    ) {
        let fine_elevs = &self.terrain.fine_elevations;
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
                            let p_start = grid.faces[c_start].0;
                            let p_end = grid.faces[land_v].0;
                            let direct_dist = ((p_end[0] - p_start[0]).powi(2)
                                + (p_end[1] - p_start[1]).powi(2)
                                + (p_end[2] - p_start[2]).powi(2))
                            .sqrt();
                            let path_dist: f64 = full_path
                                .windows(2)
                                .map(|w| {
                                    let a = grid.faces[w[0]].0;
                                    let b = grid.faces[w[1]].0;
                                    ((b[0] - a[0]).powi(2)
                                        + (b[1] - a[1]).powi(2)
                                        + (b[2] - a[2]).powi(2))
                                    .sqrt()
                                })
                                .sum();
                            if direct_dist >= 0.52 * path_dist {
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
    /// coarse tiles currently have populated communities possessing the required water technology
    /// (`rivergoing` for `RiverCorridor` routes, `seafaring` for `OceanGyre` and `CoastalSeaLane` routes).
    pub fn update_travel_routes(&mut self) {
        let coarse_elevs = self.prehistory.tile_elevations();
        let num_coarse = coarse_elevs.len();
        let mut coarse_active_fine: Vec<Option<usize>> = vec![None; num_coarse];
        let mut coarse_has_rivergoing: Vec<bool> = vec![false; num_coarse];
        let mut coarse_has_seafaring: Vec<bool> = vec![false; num_coarse];

        for (&f_idx, &sid) in &self.tile_states {
            let (pop, has_riv, has_sea) = self
                .tile_communities
                .get(&f_idx)
                .map(|cs| {
                    let p: f32 = cs.iter().map(|c| c.thousands).sum();
                    let r = cs.iter().any(|c| c.thousands > 0.0 && c.rivergoing);
                    let s = cs.iter().any(|c| c.thousands > 0.0 && c.seafaring);
                    (p, r, s)
                })
                .unwrap_or((0.0, false, false));
            if pop > 0.0 || self.states.contains_key(&sid) {
                let c = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                if c < num_coarse {
                    coarse_has_rivergoing[c] |= has_riv;
                    coarse_has_seafaring[c] |= has_sea;
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
                let has_required_tech = match route.route_kind {
                    TravelRouteKind::RiverCorridor => {
                        coarse_has_rivergoing
                            .get(route.from_coarse_tile)
                            .copied()
                            .unwrap_or(false)
                    }
                    TravelRouteKind::OceanGyre | TravelRouteKind::CoastalSeaLane => {
                        coarse_has_seafaring
                            .get(route.from_coarse_tile)
                            .copied()
                            .unwrap_or(false)
                    }
                };
                if !has_required_tech {
                    return None;
                }
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
    /// (Turchin, *Historical Dynamics*, 2003, Ch. 4), incorporating agricultural, seafaring, and rivergoing
    /// lifeway faultlines alongside nationality and linguistic divides:
    /// `+0.35` across the agricultural/foraging divide, `+0.25` across the seafaring/landlocked divide,
    /// `+0.15` across the rivergoing/upland divide, `+0.25` across nationality boundaries, and
    /// `+0.20` across language boundaries (with `-0.05` solidarity dampening when neighboring states share
    /// the same waterborne lifeway).
    pub fn tile_metaethnic_intensity(&self, f_idx: usize, fine_adj: &[Vec<usize>]) -> f32 {
        let Some(&sid) = self.tile_states.get(&f_idx) else {
            return 0.0;
        };
        let (f_ag, f_riv, f_sea) = self
            .tile_communities
            .get(&f_idx)
            .map(|cs| {
                (
                    cs.iter().any(|c| c.thousands > 0.0 && c.agriculture),
                    cs.iter().any(|c| c.thousands > 0.0 && c.rivergoing),
                    cs.iter().any(|c| c.thousands > 0.0 && c.seafaring),
                )
            })
            .unwrap_or((false, false, false));
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
                let nb_ag = nb_comms.iter().any(|c| c.thousands > 0.0 && c.agriculture);
                let nb_riv = nb_comms.iter().any(|c| c.thousands > 0.0 && c.rivergoing);
                let nb_sea = nb_comms.iter().any(|c| c.thousands > 0.0 && c.seafaring);
                let ag_diff = if f_ag != nb_ag { 0.35f32 } else { 0.0f32 };
                let sea_diff = if f_sea != nb_sea { 0.25f32 } else { 0.0f32 };
                let riv_diff = if f_riv != nb_riv { 0.15f32 } else { 0.0f32 };
                let nat_diff = if f_nat != self.majority_nationality(nb) {
                    0.25f32
                } else {
                    0.0f32
                };
                let lang_diff = if f_lang != self.majority_language(nb) {
                    0.20f32
                } else {
                    0.0f32
                };
                let shared_water_solidarity =
                    if f_ag == nb_ag && f_riv == nb_riv && f_sea == nb_sea && (f_riv || f_sea) {
                        -0.05f32
                    } else {
                        0.0f32
                    };
                Some(
                    (ag_diff + sea_diff + riv_diff + nat_diff + lang_diff + shared_water_solidarity)
                        .clamp(0.0, 1.0),
                )
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
        let _span_trade = (self.history_ticks > 0)
            .then(|| crate::profile::span(crate::profile::ProfilePhase::HistStepTradeAndConflicts))
            .flatten();
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
        drop(_span_trade);

        // 3. Accumulate concepts and borrow missing words (`historysimulation.py:664-705`)
        let _span_lex = (self.history_ticks > 0)
            .then(|| crate::profile::span(crate::profile::ProfilePhase::HistStepLexiconConcepts))
            .flatten();
        self.accumulate_and_borrow_concepts(&extents, &state_species);
    }

    /// Geographic latitude in degrees `[-90.0, 90.0]` of fine face `f_idx`.
    #[inline]
    pub fn fine_face_latitude_deg(&self, f_idx: usize) -> f64 {
        self.fine_lat_deg.get(f_idx).copied().unwrap_or_else(|| {
            let z = self
                .terrain
                .terrain_grid
                .faces
                .get(f_idx)
                .map(|(c, _)| c[2])
                .unwrap_or(0.0)
                .clamp(-1.0, 1.0);
            z.asin() * 180.0 / std::f64::consts::PI
        })
    }

    #[inline]
    fn cached_primary_coarse(&self, f_idx: usize, coarse_elevs: &[f32]) -> usize {
        self.fine_primary_coarse
            .get(f_idx)
            .copied()
            .unwrap_or_else(|| self.terrain.primary_coarse_tile(f_idx, coarse_elevs))
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
        let (conts, oceans, _, _, _) = self.geographic_features_for_tiles(tiles, coarse_elevs);
        (conts, oceans)
    }

    /// Set of local geographic features (`continents`, `oceans`, `seas`, `rivers`, `mountain_ranges`)
    /// located on or immediately adjacent to `tiles` (fine face indices).
    pub fn geographic_features_for_tiles(
        &self,
        tiles: &[usize],
        coarse_elevs: &[f32],
    ) -> (
        BTreeSet<u32>,
        BTreeSet<u32>,
        BTreeSet<u32>,
        BTreeSet<u32>,
        BTreeSet<u32>,
    ) {
        let coarse_adj = self.prehistory.adjacency();
        let mut conts = BTreeSet::new();
        let mut oceans = BTreeSet::new();
        let mut seas = BTreeSet::new();
        let mut rivers = BTreeSet::new();
        let mut mountains = BTreeSet::new();
        for &f_idx in tiles {
            if f_idx >= self.terrain.parentage.len() {
                continue;
            }
            if let Some(rids) = self.fine_face_rivers.get(f_idx) {
                for &rid in rids {
                    rivers.insert(rid);
                }
            }
            let c = self.cached_primary_coarse(f_idx, coarse_elevs);
            if let Some(cid) = self.geography.tile_continent.get(c).copied().flatten() {
                conts.insert(cid as u32);
            }
            if let Some(oid) = self.geography.tile_ocean.get(c).copied().flatten() {
                oceans.insert(oid as u32);
            }
            if let Some(sid) = self.geography.tile_sea.get(c).copied().flatten() {
                seas.insert(sid as u32);
            }
            if let Some(mid) = self.geography.tile_mountain_range.get(c).copied().flatten() {
                mountains.insert(mid as u32);
            }
            if let Some(nbs) = coarse_adj.get(c) {
                for &nb in nbs {
                    if let Some(oid) = self.geography.tile_ocean.get(nb).copied().flatten() {
                        oceans.insert(oid as u32);
                    }
                    if let Some(sid) = self.geography.tile_sea.get(nb).copied().flatten() {
                        seas.insert(sid as u32);
                    }
                    if let Some(mid) = self.geography.tile_mountain_range.get(nb).copied().flatten()
                    {
                        mountains.insert(mid as u32);
                    }
                }
            }
        }
        (conts, oceans, seas, rivers, mountains)
    }

    /// Location-dependent seasons experienced on `tiles` based on Köppen climate zone, latitude, and axial tilt.
    /// Not every climate has a monsoon season, savanna wet/dry cycle, or thermal winter/spring/autumn!
    pub fn seasons_for_tiles(
        &self,
        tiles: &[usize],
        coarse_elevs: &[f32],
        koeppen: &[u8],
    ) -> BTreeSet<u32> {
        let has_axial_tilt = self.prehistory.tilt_deg().abs() >= 5.0;
        let mut out = BTreeSet::new();
        for &f_idx in tiles {
            if f_idx >= self.terrain.parentage.len() {
                continue;
            }
            let c = self.cached_primary_coarse(f_idx, coarse_elevs);
            let k = koeppen.get(c).copied().unwrap_or(0);
            let lat_abs = self.fine_face_latitude_deg(f_idx).abs();

            // Thermal summer & winter (temperate, continental, tundra, or mid/high-latitude with axial tilt)
            if matches!(k, 6..=11 | 13) || (has_axial_tilt && lat_abs >= 18.0 && k != 12) {
                out.insert(SEASON_SUMMER);
            }
            if matches!(k, 6..=13) || (has_axial_tilt && lat_abs >= 23.0) {
                out.insert(SEASON_WINTER);
            }
            // Four-season temperate/continental spring & autumn transitions
            if matches!(k, 6..=11)
                || (has_axial_tilt && (20.0..=66.0).contains(&lat_abs) && !matches!(k, 1..=5 | 12))
            {
                out.insert(SEASON_SPRING);
                out.insert(SEASON_AUTUMN);
            }
            // Monsoon season: strictly Am (2), Cw (7), or Dw (9) monsoon/winter-dry regimes
            if matches!(k, 2 | 7 | 9) {
                out.insert(SEASON_MONSOON);
            }
            // Tropical wet season: Am (2) or Aw (3)
            if matches!(k, 2 | 3) {
                out.insert(SEASON_WET);
            }
            // Distinct dry season: Aw (3), BS (4), Cs (6), Cw (7), Dw (9), Ds (11)
            if matches!(k, 3 | 4 | 6 | 7 | 9 | 11) {
                out.insert(SEASON_DRY);
            }
        }
        out
    }

    /// Location-dependent weather phenomena experienced on `tiles` based on Köppen climate zone, latitude, and elevation.
    /// Warm tropical/subtropical lowlands never experience snow or ice, while humid/polar regions never experience sandstorms.
    pub fn weather_for_tiles(
        &self,
        tiles: &[usize],
        coarse_elevs: &[f32],
        koeppen: &[u8],
    ) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for &f_idx in tiles {
            if f_idx >= self.terrain.parentage.len() {
                continue;
            }
            let c = self.cached_primary_coarse(f_idx, coarse_elevs);
            let k = koeppen.get(c).copied().unwrap_or(0);
            let fine_elev = self
                .terrain
                .fine_elevations
                .get(f_idx)
                .copied()
                .unwrap_or(0.0);
            let lat_abs = self.fine_face_latitude_deg(f_idx).abs();

            out.insert(WEATHER_WIND);
            if k != 5 {
                out.insert(WEATHER_CLOUD);
            }
            if k != 12 {
                out.insert(WEATHER_RAIN);
            }
            if matches!(k, 1..=3 | 7 | 8 | 10) {
                out.insert(WEATHER_THUNDERSTORM);
            }
            if matches!(k, 3..=6) {
                out.insert(WEATHER_DROUGHT);
            }
            // Sandstorms / dust winds: strictly arid/semi-arid BS (4) and BW (5)
            if matches!(k, 4 | 5) {
                out.insert(WEATHER_SANDSTORM);
            }
            // Frost: temperate/continental/polar climates or highlands >= 2.0 km
            if matches!(k, 6..=13) || fine_elev >= 2.0 {
                out.insert(WEATHER_FROST);
            }
            // Snow: continental/polar/tundra (9..=13), poleward Cfb (k == 8 && |lat| >= 42°), or mountains >= 2.5 km
            if matches!(k, 9..=13) || (k == 8 && lat_abs >= 42.0) || fine_elev >= 2.5 {
                out.insert(WEATHER_SNOW);
            }
            // Ice / glacier: freezing continental/polar climates (9..=13) or high alpine peaks >= 3.0 km
            if matches!(k, 9..=13) || fine_elev >= 3.0 {
                out.insert(WEATHER_ICE);
            }
        }
        out
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
        let koeppen = self.prehistory.tile_koeppen_ids().to_vec();
        let mut state_langs: BTreeMap<u32, Option<u32>> = BTreeMap::new();
        let mut state_visible_constellations: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_continents: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_oceans: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_seas: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_rivers: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_mountains: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_tech: BTreeMap<u32, (bool, bool, bool)> = BTreeMap::new();
        let mut state_rec_partners: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_direct_partners: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut state_rivals: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for (&sid, tiles) in extents {
            state_langs.insert(sid, self.primary_language_for_state(sid));
            state_visible_constellations.insert(sid, self.visible_constellations_for_tiles(tiles));
            let (conts, oceans, seas, rivers, mountains) =
                self.geographic_features_for_tiles(tiles, &coarse_elevs);
            state_continents.insert(sid, conts);
            state_oceans.insert(sid, oceans);
            state_seas.insert(sid, seas);
            state_rivers.insert(sid, rivers);
            state_mountains.insert(sid, mountains);
            state_rec_partners.insert(sid, self.recursive_trade_partners(sid));
            state_direct_partners.insert(sid, self.lookup_trade_partners(sid));
            state_rivals.insert(sid, self.lookup_opponents(sid));

            let mut has_ag = false;
            let mut has_riv = false;
            let mut has_sea = false;
            for &t in tiles {
                if let Some(comms) = self.tile_communities.get(&t) {
                    for c in comms {
                        if c.thousands > 0.0 {
                            has_ag |= c.agriculture;
                            has_riv |= c.rivergoing;
                            has_sea |= c.seafaring;
                        }
                    }
                }
            }
            state_tech.insert(sid, (has_ag, has_riv, has_sea));
        }

        let mut lang_concepts: BTreeMap<u32, BTreeMap<ConceptKey, Option<u32>>> = BTreeMap::new();
        let mut state_sources: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut lang_visible_constellations: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut lang_tiles: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut lang_own_tech: BTreeMap<u32, (bool, bool, bool)> = BTreeMap::new();

        for (&f, comms) in &self.tile_communities {
            let Some(&sid) = self.tile_states.get(&f) else {
                continue;
            };
            let active_comms: Vec<&Community> =
                comms.iter().filter(|c| c.thousands > 0.0).collect();
            for c1 in &active_comms {
                let lang = c1.language_id;
                lang_tiles.entry(lang).or_default().push(f);
                let tech_entry = lang_own_tech
                    .entry(lang)
                    .or_insert((false, false, false));
                tech_entry.0 |= c1.agriculture;
                tech_entry.1 |= c1.rivergoing;
                tech_entry.2 |= c1.seafaring;

                let entry = lang_concepts.entry(lang).or_default();
                for c2 in &active_comms {
                    let n = c2.nationality_id;
                    let l = c2.language_id;
                    entry.insert(ConceptKey::Language(l), Some(l));
                    entry.insert(ConceptKey::Nation(n), Some(l));
                    // Co-resident exposure to farming, river fishing, and sea fishing:
                    if c2.agriculture {
                        entry
                            .entry(ConceptKey::Culture(CULTURE_FARMING))
                            .or_insert(Some(l));
                    }
                    if c2.rivergoing {
                        entry
                            .entry(ConceptKey::Culture(CULTURE_RIVER_FISHING))
                            .or_insert(Some(l));
                    }
                    if c2.seafaring {
                        entry
                            .entry(ConceptKey::Culture(CULTURE_SEA_FISHING))
                            .or_insert(Some(l));
                    }
                }
                state_sources.entry(lang).or_default().insert(sid);
            }
        }

        for (&lang_id, concepts) in lang_concepts.iter_mut() {
            // Ensure universal basic vocabulary and baseline foraging/city concepts are present
            for bw_id in 0..(BASIC_WORD_GLOSSES.len() as u32) {
                concepts
                    .entry(ConceptKey::BasicWord(bw_id))
                    .or_insert(Some(lang_id));
            }
            concepts
                .entry(ConceptKey::Culture(CULTURE_FORAGING))
                .or_insert(Some(lang_id));
            concepts
                .entry(ConceptKey::Culture(CULTURE_CITY))
                .or_insert(Some(lang_id));

            // Own technological practices coin/retain words for farming, river fishing, and sea fishing
            if let Some(&(own_ag, own_riv, own_sea)) = lang_own_tech.get(&lang_id) {
                if own_ag {
                    concepts.insert(ConceptKey::Culture(CULTURE_FARMING), Some(lang_id));
                }
                if own_riv {
                    concepts.insert(ConceptKey::Culture(CULTURE_RIVER_FISHING), Some(lang_id));
                }
                if own_sea {
                    concepts.insert(ConceptKey::Culture(CULTURE_SEA_FISHING), Some(lang_id));
                }
            }

            // Location-dependent seasons & weather experienced on the tiles where `lang_id` is actually spoken
            if let Some(ltiles) = lang_tiles.get(&lang_id) {
                for s_id in self.seasons_for_tiles(ltiles, &coarse_elevs, &koeppen) {
                    concepts
                        .entry(ConceptKey::Season(s_id))
                        .or_insert(Some(lang_id));
                }
                for w_id in self.weather_for_tiles(ltiles, &coarse_elevs, &koeppen) {
                    concepts
                        .entry(ConceptKey::Weather(w_id))
                        .or_insert(Some(lang_id));
                }
            }

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

            let empty_usize_set = BTreeSet::new();
            let empty_u32_set = BTreeSet::new();
            for &state_id in states_for_lang {
                let native = state_species.get(&state_id).unwrap_or(&empty_usize_set);
                let vis_here = state_visible_constellations
                    .get(&state_id)
                    .unwrap_or(&empty_u32_set);
                let rec_partners = state_rec_partners
                    .get(&state_id)
                    .unwrap_or(&empty_u32_set);
                let direct_partners = state_direct_partners
                    .get(&state_id)
                    .unwrap_or(&empty_u32_set);
                let rivals = state_rivals
                    .get(&state_id)
                    .unwrap_or(&empty_u32_set);
                let s_lang = state_langs.get(&state_id).copied().flatten();

                // Exposure to trading / commerce when state participates in trade routes
                if !direct_partners.is_empty() || !rec_partners.is_empty() {
                    let donor_trade_lang = direct_partners
                        .iter()
                        .chain(rec_partners.iter())
                        .find_map(|p| {
                            let plid = state_langs.get(p).copied().flatten()?;
                            self.languages
                                .get(&plid)?
                                .describes(ConceptKey::Culture(CULTURE_TRADING))
                                .then_some(plid)
                        })
                        .or(s_lang)
                        .or(Some(lang_id));
                    concepts
                        .entry(ConceptKey::Culture(CULTURE_TRADING))
                        .or_insert(donor_trade_lang);
                }

                // Exposure to warfare / battle when state is engaged in conflict
                if !rivals.is_empty() {
                    let donor_war_lang = rivals
                        .iter()
                        .find_map(|r| {
                            let rlid = state_langs.get(r).copied().flatten()?;
                            self.languages
                                .get(&rlid)?
                                .describes(ConceptKey::Culture(CULTURE_WARFARE))
                                .then_some(rlid)
                        })
                        .or(s_lang)
                        .or(Some(lang_id));
                    concepts
                        .entry(ConceptKey::Culture(CULTURE_WARFARE))
                        .or_insert(donor_war_lang);
                }

                let mut related_states = BTreeSet::new();
                related_states.insert(state_id);
                related_states.extend(direct_partners.iter().copied());
                related_states.extend(rivals.iter().copied());
                related_states.extend(rec_partners.iter().copied());

                for s in &related_states {
                    let rel_lang = state_langs.get(s).copied().flatten();
                    concepts.insert(ConceptKey::State(*s), rel_lang);
                    // Exposure to farming, river fishing, and sea fishing from neighbors, rivals, or trading partners:
                    if let Some(&(s_ag, s_riv, s_sea)) = state_tech.get(s) {
                        if s_ag {
                            concepts
                                .entry(ConceptKey::Culture(CULTURE_FARMING))
                                .or_insert(rel_lang);
                        }
                        if s_riv {
                            concepts
                                .entry(ConceptKey::Culture(CULTURE_RIVER_FISHING))
                                .or_insert(rel_lang);
                        }
                        if s_sea {
                            concepts
                                .entry(ConceptKey::Culture(CULTURE_SEA_FISHING))
                                .or_insert(rel_lang);
                        }
                    }
                    // Share astronomical knowledge of all planets in `self.solar_system` across trading partners & neighbors:
                    if let Some(slid) = rel_lang {
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
                let mut trade_network = direct_partners.clone();
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

                // Imported species and foreign geographic features from recursive trade network (`imports`)
                let home_conts = state_continents.get(&state_id).unwrap_or(&empty_u32_set);
                let home_oceans = state_oceans.get(&state_id).unwrap_or(&empty_u32_set);
                let home_seas = state_seas.get(&state_id).unwrap_or(&empty_u32_set);
                let home_rivers = state_rivers.get(&state_id).unwrap_or(&empty_u32_set);
                let home_mountains = state_mountains.get(&state_id).unwrap_or(&empty_u32_set);
                for &partner in rec_partners {
                    let p_lang = state_langs.get(&partner).copied().flatten();
                    if let Some(psp) = state_species.get(&partner) {
                        for &sp in psp.difference(native).take(6) {
                            concepts.insert(ConceptKey::Species(sp as u32), p_lang);
                        }
                    }
                    if let Some(pconts) = state_continents.get(&partner) {
                        for &cid in pconts.difference(home_conts) {
                            concepts.entry(ConceptKey::Continent(cid)).or_insert(p_lang);
                        }
                    }
                    if let Some(poceans) = state_oceans.get(&partner) {
                        for &oid in poceans.difference(home_oceans) {
                            concepts.entry(ConceptKey::Ocean(oid)).or_insert(p_lang);
                        }
                    }
                    if let Some(pseas) = state_seas.get(&partner) {
                        for &sea_id in pseas.difference(home_seas) {
                            concepts.entry(ConceptKey::Sea(sea_id)).or_insert(p_lang);
                        }
                    }
                    if let Some(privers) = state_rivers.get(&partner) {
                        for &rid in privers.difference(home_rivers).take(4) {
                            concepts.entry(ConceptKey::River(rid)).or_insert(p_lang);
                        }
                    }
                    if let Some(pmountains) = state_mountains.get(&partner) {
                        for &mid in pmountains.difference(home_mountains).take(4) {
                            concepts
                                .entry(ConceptKey::MountainRange(mid))
                                .or_insert(p_lang);
                        }
                    }
                }

                // Native species, local geographic features (continents, oceans, seas, rivers, mountain ranges),
                // visible constellations (`vis_here`), and core celestial concepts (`Sun`, `Moons`, `Planets`) in `state_id`
                for &cid in home_conts {
                    concepts.insert(ConceptKey::Continent(cid), s_lang);
                }
                for &oid in home_oceans {
                    concepts.insert(ConceptKey::Ocean(oid), s_lang);
                }
                for &sea_id in home_seas {
                    concepts.insert(ConceptKey::Sea(sea_id), s_lang);
                }
                for &rid in home_rivers {
                    concepts.insert(ConceptKey::River(rid), s_lang);
                }
                for &mid in home_mountains {
                    concepts.insert(ConceptKey::MountainRange(mid), s_lang);
                }
                for &c_idx in vis_here {
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
                for &sp in native {
                    concepts.insert(ConceptKey::Species(sp as u32), s_lang);
                }
            }
        }

        let empty_vis = BTreeSet::new();

        // Pass 1: Coin native concepts (including all constellations visible at some point in the year)
        // so that donor languages have their visible constellations coined before trade partners borrow them.
        for (&lang_id, concepts) in &lang_concepts {
            let vis_set = lang_visible_constellations
                .get(&lang_id)
                .unwrap_or(&empty_vis);
            for (&concept, &src_lang_opt) in concepts {
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
        for (lang_id, concepts) in lang_concepts {
            let vis_set = lang_visible_constellations
                .get(&lang_id)
                .unwrap_or(&empty_vis);
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
        let c = self.cached_primary_coarse(f_idx, coarse_elevs);
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
            let (curr_ag, curr_riv, curr_sea) = self
                .tile_communities
                .get(&curr)
                .map(|cs| {
                    (
                        cs.iter().any(|c| c.agriculture),
                        cs.iter().any(|c| c.rivergoing),
                        cs.iter().any(|c| c.seafaring),
                    )
                })
                .unwrap_or((false, false, false));
            let curr_nat = self.majority_nationality(curr);
            let curr_lang = self.majority_language(curr);

            for &next in &fine_adj[curr] {
                if !tile_set.contains(&next) {
                    continue;
                }
                let (next_pop, next_ag, next_riv, next_sea) = self
                    .tile_communities
                    .get(&next)
                    .map(|cs| {
                        (
                            cs.iter().map(|c| c.thousands).sum::<f32>(),
                            cs.iter().any(|c| c.agriculture),
                            cs.iter().any(|c| c.rivergoing),
                            cs.iter().any(|c| c.seafaring),
                        )
                    })
                    .unwrap_or((0.0, false, false, false));
                if self.terrain.fine_elevations[next] <= 0.0 || next_pop <= 0.0 || curr_ag != next_ag {
                    continue;
                }
                let base_step_cost = if self.river_tiles[next] {
                    if curr_riv && next_riv {
                        1
                    } else if curr_riv || next_riv {
                        3
                    } else {
                        10
                    }
                } else if curr_nat != self.majority_nationality(next) {
                    3
                } else if curr_lang != self.majority_language(next) {
                    2
                } else {
                    1
                };
                let water_faultline =
                    u32::from(curr_riv != next_riv) * 2 + u32::from(curr_sea != next_sea) * 2;
                let step_cost = base_step_cost + water_faultline;
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
        let mut lang_locs: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (&f, comms) in &self.tile_communities {
            for c in comms {
                if c.thousands > 0.0 {
                    let vec = lang_locs.entry(c.language_id).or_default();
                    if vec.last().copied() != Some(f) {
                        vec.push(f);
                    }
                }
            }
        }

        let num_fine = fine_adj.len();
        let mut tile_stamp = vec![0u32; num_fine];
        let mut stamp_counter = 0u32;
        let mut q = VecDeque::new();

        for (lang_id, locs) in lang_locs {
            if locs.len() <= 1 {
                continue;
            }
            stamp_counter = stamp_counter.wrapping_add(2);
            if stamp_counter == 0 {
                tile_stamp.fill(0);
                stamp_counter = 2;
            }
            let present_stamp = stamp_counter - 1;
            let visited_stamp = stamp_counter;
            for &f in &locs {
                tile_stamp[f] = present_stamp;
            }

            let mut components: Vec<Vec<usize>> = Vec::new();
            for &start in &locs {
                if tile_stamp[start] == visited_stamp {
                    continue;
                }
                tile_stamp[start] = visited_stamp;
                let mut comp = Vec::new();
                q.clear();
                q.push_back(start);
                while let Some(curr) = q.pop_front() {
                    comp.push(curr);
                    for &nb in &fine_adj[curr] {
                        if tile_stamp[nb] == present_stamp {
                            tile_stamp[nb] = visited_stamp;
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
        let fine_adj = {
            let _span_clones = crate::profile::span(crate::profile::ProfilePhase::HistStepGridClones);
            std::sync::Arc::clone(&self.fine_adj)
        };

        // 1. Language sound changes (`historysimulation.py:511-515`: 2-4 cascaded per-language sound changes per 200-year tick)
        {
            let _span_lang = crate::profile::span(crate::profile::ProfilePhase::HistStepLanguageSoundChanges);
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
        }

        let _span_growth = crate::profile::span(crate::profile::ProfilePhase::HistStepGrowthSpilloverAndTech);
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
                    let c = self.fine_primary_coarse[nb];
                    let nb_pop = tile_pops.get(&nb).copied().unwrap_or(0.0);
                    let nb_elev = self.terrain.fine_elevations[nb];
                    nb_elev > 0.0
                        && is_passable(nb_elev, self.coarse_koeppen[c], true)
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

        // 2b. Maritime & riverine expansion along established wind/current travel routes (`self.travel_routes`),
        // gated by `rivergoing` on `RiverCorridor` routes and `seafaring` on `OceanGyre` / `CoastalSeaLane` routes.
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
                let source_comm = src_comms.iter().find(|c| {
                    c.thousands > 0.0
                        && match route.route_kind {
                            TravelRouteKind::RiverCorridor => c.rivergoing,
                            TravelRouteKind::OceanGyre | TravelRouteKind::CoastalSeaLane => {
                                c.seafaring
                            }
                        }
                })?;
                let c_dst = self.fine_primary_coarse[dst];
                let dst_elev = self.terrain.fine_elevations[dst];
                if dst_elev <= 0.0
                    || !is_passable(dst_elev, self.coarse_koeppen[c_dst], true)
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
                let mut migrant = source_comm.clone();
                let frac = (0.06 * route.wind_current_alignment.max(0.35)).clamp(0.02, 0.08);
                migrant.thousands = (source_comm.thousands * frac).max(0.02);
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

        // 2c. Independent invention and neighbor/route diffusion of agriculture, rivergoing, and seafaring
        // (rivergoing and seafaring develop independently of agriculture and of each other, with a 3x mutual synergy boost).
        let mut ag_tiles = BTreeSet::new();
        let mut riv_tiles = BTreeSet::new();
        let mut sea_tiles = BTreeSet::new();
        for (&t, comms) in &self.tile_communities {
            for c in comms {
                if c.thousands > 0.0 {
                    if c.agriculture {
                        ag_tiles.insert(t);
                    }
                    if c.rivergoing {
                        riv_tiles.insert(t);
                    }
                    if c.seafaring {
                        sea_tiles.insert(t);
                    }
                }
            }
        }
        let mut route_neighbors: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for r in &self.travel_routes {
            route_neighbors
                .entry(r.from_fine_face)
                .or_default()
                .push(r.to_fine_face);
            route_neighbors
                .entry(r.to_fine_face)
                .or_default()
                .push(r.from_fine_face);
        }
        let populated_keys: Vec<usize> = self.tile_communities.keys().copied().collect();
        for t in populated_keys {
            let is_riv_tile = self.river_tiles.get(t).copied().unwrap_or(false);
            let is_coast_tile = self.is_coastal_fine.get(t).copied().unwrap_or(false);
            let c_coarse = self.fine_primary_coarse[t];
            let farmable_tile =
                is_farmable(self.terrain.fine_elevations[t], self.coarse_koeppen[c_coarse]);
            let adj_has_ag = fine_adj[t].iter().any(|nb| ag_tiles.contains(nb));
            let adj_has_riv = fine_adj[t].iter().any(|nb| riv_tiles.contains(nb))
                || route_neighbors
                    .get(&t)
                    .is_some_and(|nbs| nbs.iter().any(|nb| riv_tiles.contains(nb)));
            let adj_has_sea = fine_adj[t].iter().any(|nb| sea_tiles.contains(nb))
                || route_neighbors
                    .get(&t)
                    .is_some_and(|nbs| nbs.iter().any(|nb| sea_tiles.contains(nb)));

            if let Some(comms) = self.tile_communities.get_mut(&t) {
                for c in comms.iter_mut() {
                    if c.thousands <= 0.0 {
                        continue;
                    }
                    if !c.agriculture && farmable_tile {
                        let spark_p = if is_riv_tile { 0.025 } else { 0.006 };
                        let diff_p = if adj_has_ag { 0.10 } else { 0.0 };
                        if self.rng.next_f64() < (spark_p + diff_p) {
                            c.agriculture = true;
                        }
                    }
                    if !c.rivergoing && (is_riv_tile || adj_has_riv) {
                        let synergy = if c.seafaring { 3.0f64 } else { 1.0f64 };
                        let spark_p = if is_riv_tile { 0.020f64 * synergy } else { 0.0f64 };
                        let diff_p = if adj_has_riv && is_riv_tile {
                            0.12f64 * synergy
                        } else if adj_has_riv {
                            0.04f64 * synergy
                        } else {
                            0.0f64
                        };
                        if self.rng.next_f64() < (spark_p + diff_p).min(0.85f64) {
                            c.rivergoing = true;
                        }
                    }
                    if !c.seafaring && (is_coast_tile || adj_has_sea) {
                        let synergy = if c.rivergoing { 3.0f64 } else { 1.0f64 };
                        let spark_p = if is_coast_tile { 0.016f64 * synergy } else { 0.0f64 };
                        let diff_p = if adj_has_sea && is_coast_tile {
                            0.12f64 * synergy
                        } else if adj_has_sea {
                            0.04f64 * synergy
                        } else {
                            0.0f64
                        };
                        if self.rng.next_f64() < (spark_p + diff_p).min(0.85f64) {
                            c.seafaring = true;
                        }
                    }
                }
            }
        }
        self.refresh_fishing_capacities();
        self.update_travel_routes();
        drop(_span_growth);

        let _span_assim = crate::profile::span(crate::profile::ProfilePhase::HistStepAssimilationAndDialects);
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
                let riv_pop: f32 = comms
                    .iter()
                    .filter(|c| c.rivergoing)
                    .map(|c| c.thousands)
                    .sum();
                let sea_pop: f32 = comms
                    .iter()
                    .filter(|c| c.seafaring)
                    .map(|c| c.thousands)
                    .sum();
                // Cultural assimilation (`assimilateculture`: >= 75% agricultural or >= 65% rivergoing/seafaring converts communities > 5%)
                if total_pop > 0.0 {
                    let adopt_ag = ag_pop / total_pop >= 0.75;
                    let adopt_riv = riv_pop / total_pop >= 0.65;
                    let adopt_sea = sea_pop / total_pop >= 0.65;
                    if adopt_ag || adopt_riv || adopt_sea {
                        for c in comms.iter_mut() {
                            if c.thousands / total_pop > 0.05 {
                                if adopt_ag {
                                    c.agriculture = true;
                                }
                                if adopt_riv {
                                    c.rivergoing = true;
                                }
                                if adopt_sea {
                                    c.seafaring = true;
                                }
                            }
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
        drop(_span_assim);

        let _span_war = crate::profile::span(crate::profile::ProfilePhase::HistStepWarfareConquestAndSplits);
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
                let p = self.city_prob(f, &self.coarse_elevs, &self.coarse_koeppen, &fine_adj);
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
        drop(_span_war);

        // 8. Update trade, conflicts, concept accumulation, and recolor nations
        self.history_ticks += 1;
        {
            let _span_recolor = crate::profile::span(crate::profile::ProfilePhase::HistStepLexiconConcepts);
            self.recolor_states(&fine_adj);
        }
        self.update_diplomacy_and_trade(&fine_adj);
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

    pub fn set_sky_optics(&mut self, home_star: HomeStar, optics: PlanetaryOptics) {
        self.home_star = home_star;
        self.optics = optics;
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
        let s1_opt = self.tile_states.get(&route.from_fine_face).copied();
        let s2_opt = self.tile_states.get(&route.to_fine_face).copied();
        match (s1_opt, s2_opt) {
            (Some(s1), Some(s2)) if s1 != s2 => {
                let pair = StatePair::new(s1, s2);
                if self.conflicts.contains(&pair) {
                    TravelRouteStatus::NavalConflict
                } else if self.trade_partners.contains(&pair) {
                    TravelRouteStatus::MaritimeTrade
                } else {
                    TravelRouteStatus::ActiveSeaLane
                }
            }
            (Some(_), Some(_)) => TravelRouteStatus::ActiveSeaLane,
            _ => TravelRouteStatus::HypotheticalRoute,
        }
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
                    -c[1] as f32 * radius_lift,
                    c[2] as f32 * radius_lift,
                    c[0] as f32 * radius_lift,
                ]
            };

            let interior_coarse = if route.coarse_path.len() >= 2 {
                &route.coarse_path[1..route.coarse_path.len() - 1]
            } else {
                &[][..]
            };

            let mut pts: Vec<[f32; 3]> = self
                .terrain
                .terrain_grid
                .faces
                .get(route.from_fine_face)
                .map(|&(c, _)| lift_pt(c))
                .into_iter()
                .chain(
                    interior_coarse
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

            if pts.len() >= 3 && route.route_kind != TravelRouteKind::RiverCorridor {
                let raw = pts.clone();
                for i in 1..(pts.len() - 1) {
                    let p_prev = raw[i - 1];
                    let p_curr = raw[i];
                    let p_next = raw[i + 1];
                    let sx = 0.25 * p_prev[0] + 0.50 * p_curr[0] + 0.25 * p_next[0];
                    let sy = 0.25 * p_prev[1] + 0.50 * p_curr[1] + 0.25 * p_next[1];
                    let sz = 0.25 * p_prev[2] + 0.50 * p_curr[2] + 0.25 * p_next[2];
                    let len = (sx * sx + sy * sy + sz * sz).sqrt().max(1e-6);
                    pts[i] = [
                        sx * radius_lift / len,
                        sy * radius_lift / len,
                        sz * radius_lift / len,
                    ];
                }
            }

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
            let p0_grid = [
                p[0] as f32 * r_lift,
                p[1] as f32 * r_lift,
                p[2] as f32 * r_lift,
            ];
            let p0 = [-p0_grid[1], p0_grid[2], p0_grid[0]];

            if coarse_elevs[c] <= 0.0 {
                // Ocean current vector (warm = orange-gold, cold = cyan-blue)
                let vel = ocean.velocity_xyz.get(c).copied().unwrap_or([0.0; 3]);
                let spd = ocean.speed.get(c).copied().unwrap_or(0.0);
                let sst = ocean.sst_anomaly.get(c).copied().unwrap_or(0.0);
                if spd > 0.04 {
                    let len = 0.032 * spd.sqrt().clamp(0.35, 1.0);
                    let p1_grid = [
                        p0_grid[0] + len * vel[0],
                        p0_grid[1] + len * vel[1],
                        p0_grid[2] + len * vel[2],
                    ];
                    let p1 = [-p1_grid[1], p1_grid[2], p1_grid[0]];
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
                let p1_grid = [
                    p0_grid[0] + len * w[0],
                    p0_grid[1] + len * w[1],
                    p0_grid[2] + len * w[2],
                ];
                let p1 = [-p1_grid[1], p1_grid[2], p1_grid[0]];
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
                    let gloss = self.format_concept_gloss(ck);
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

    /// Look up the capitalized word for `ck` in `preferred_lang`, or fall back to the first language in the simulation
    /// that has coined or borrowed a word for `ck`.
    pub fn concept_word_in_language(
        &self,
        ck: ConceptKey,
        preferred_lang: Option<u32>,
    ) -> Option<String> {
        preferred_lang
            .and_then(|lid| self.languages.get(&lid))
            .and_then(|l| l.lexicon.get(&ck))
            .or_else(|| self.languages.values().find_map(|l| l.lexicon.get(&ck)))
            .map(|orig| orig.word.write(true))
    }

    /// Human-readable gloss for river `rid`.
    pub fn river_gloss(&self, rid: usize) -> String {
        let cont = self
            .rivers
            .get(rid)
            .and_then(|r| r.first())
            .and_then(|&c| self.geography.continent_name(c))
            .unwrap_or("Mainland");
        let len = self
            .river_routes
            .get(rid)
            .map(|r| r.len())
            .or_else(|| self.rivers.get(rid).map(|r| r.len()))
            .unwrap_or(0);
        format!("river #{} ({cont}, {len} tiles)", rid + 1)
    }

    /// Format a continent name using the simulation language word (if named) plus its geographic region.
    pub fn continent_label_for_tile(
        &self,
        coarse_idx: usize,
        preferred_lang: Option<u32>,
    ) -> String {
        let Some(cid) = self.geography.tile_continent.get(coarse_idx).copied().flatten() else {
            return "Mainland".to_string();
        };
        let desc = self
            .geography
            .continents
            .get(cid)
            .map(|c| c.name.as_str())
            .unwrap_or("Mainland");
        if let Some(w) = self.concept_word_in_language(ConceptKey::Continent(cid as u32), preferred_lang)
        {
            format!("{w} ({desc})")
        } else {
            desc.to_string()
        }
    }

    /// Format an ocean (and coastal sea, if applicable) name using the simulation's languages.
    pub fn ocean_and_sea_label_for_tile(
        &self,
        coarse_idx: usize,
        preferred_lang: Option<u32>,
    ) -> String {
        let ocean_part = if let Some(oid) =
            self.geography.tile_ocean.get(coarse_idx).copied().flatten()
        {
            let desc = self
                .geography
                .oceans
                .get(oid)
                .map(|o| o.name.as_str())
                .unwrap_or("World Ocean");
            if let Some(w) =
                self.concept_word_in_language(ConceptKey::Ocean(oid as u32), preferred_lang)
            {
                format!("{w} ({desc})")
            } else {
                desc.to_string()
            }
        } else {
            "World Ocean".to_string()
        };

        if let Some(sid) = self.geography.tile_sea.get(coarse_idx).copied().flatten() {
            let sdesc = self
                .geography
                .seas
                .get(sid)
                .map(|s| s.name.as_str())
                .unwrap_or("Coastal Sea");
            let sea_str = if let Some(w) =
                self.concept_word_in_language(ConceptKey::Sea(sid as u32), preferred_lang)
            {
                format!("{w} ({sdesc})")
            } else {
                sdesc.to_string()
            };
            format!("Sea: {sea_str} | Ocean: {ocean_part}")
        } else {
            ocean_part
        }
    }

    /// Format a human-readable gloss for any [`ConceptKey`].
    pub fn format_concept_gloss(&self, ck: ConceptKey) -> String {
        match ck {
            ConceptKey::BasicWord(id) => BASIC_WORD_GLOSSES
                .get(id as usize)
                .copied()
                .unwrap_or("basic word")
                .to_string(),
            ConceptKey::Culture(id) => CULTURE_CONCEPT_GLOSSES
                .get(id as usize)
                .copied()
                .unwrap_or("cultural concept")
                .to_string(),
            ConceptKey::Season(id) => SEASON_CONCEPT_GLOSSES
                .get(id as usize)
                .copied()
                .unwrap_or("season")
                .to_string(),
            ConceptKey::Weather(id) => WEATHER_CONCEPT_GLOSSES
                .get(id as usize)
                .copied()
                .unwrap_or("weather")
                .to_string(),
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
            ConceptKey::Sea(sid) => self
                .geography
                .seas
                .get(sid as usize)
                .map(|s| format!("sea ({})", s.name))
                .unwrap_or_else(|| format!("sea #{}", sid + 1)),
            ConceptKey::River(rid) => self.river_gloss(rid as usize),
            ConceptKey::MountainRange(mid) => self
                .geography
                .mountain_ranges
                .get(mid as usize)
                .map(|m| format!("mountain range ({}, {:.1} km)", m.name, m.max_elevation_km))
                .unwrap_or_else(|| format!("mountain range #{}", mid + 1)),
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
            ConceptKey::State(id) => {
                let sname = self.state_name(id);
                format!("state #{} ({sname})", id + 1)
            }
            ConceptKey::Nation(id) => {
                let nname = self.nation_name(id, None);
                format!("nation #{} ({nname})", id + 1)
            }
            ConceptKey::Language(id) => {
                let lname = self
                    .languages
                    .get(&id)
                    .map(|l| l.name())
                    .unwrap_or_else(|| format!("Lang_{}", id + 1));
                format!("language #{} ({lname})", id + 1)
            }
        }
    }

    /// Category label for a [`ConceptKey`] in lexicon tables.
    pub fn concept_category_label(ck: ConceptKey) -> &'static str {
        match ck {
            ConceptKey::BasicWord(_) => "Core",
            ConceptKey::Culture(_) => "Culture",
            ConceptKey::Season(_) => "Season",
            ConceptKey::Weather(_) => "Weather",
            ConceptKey::Sun(_)
            | ConceptKey::Moon(_)
            | ConceptKey::Planet(_)
            | ConceptKey::Constellation(_) => "Sky",
            ConceptKey::Continent(_)
            | ConceptKey::Ocean(_)
            | ConceptKey::Sea(_)
            | ConceptKey::River(_)
            | ConceptKey::MountainRange(_) => "Geo",
            ConceptKey::Species(_) => "Bio",
            ConceptKey::State(_) | ConceptKey::Nation(_) | ConceptKey::Language(_) => "Polity",
        }
    }

    /// Build a tangent-plane zoomed-in 2D map projection centered on `state_id`'s tiles (plus a 2-hop context ring),
    /// immune to polar distortion near the North or South Pole.
    pub fn build_state_zoomed_map(
        &self,
        state_id: u32,
        selected_fine_idx: Option<usize>,
    ) -> Vec<ZoomedMapPolygon> {
        let state_tiles: BTreeSet<usize> = self
            .tile_states
            .iter()
            .filter_map(|(&f, &sid)| (sid == state_id).then_some(f))
            .collect();
        if state_tiles.is_empty() {
            return Vec::new();
        }

        // Collect 1-hop and 2-hop surrounding context rings for borders and coastlines
        let fine_adj = &self.terrain.terrain_adj;
        let mut ring1: BTreeSet<usize> = state_tiles.clone();
        for &f in &state_tiles {
            if let Some(nbs) = fine_adj.get(f) {
                for &nb in nbs {
                    ring1.insert(nb);
                }
            }
        }
        let mut all_faces: BTreeSet<usize> = ring1.clone();
        for &f in &ring1 {
            if let Some(nbs) = fine_adj.get(f) {
                for &nb in nbs {
                    all_faces.insert(nb);
                }
            }
        }

        // Compute normalized 3D unit centroid of the state's tiles
        let mut sum = [0.0f64; 3];
        for &f in &state_tiles {
            if let Some((c, _)) = self.terrain.terrain_grid.faces.get(f) {
                sum[0] += c[0];
                sum[1] += c[1];
                sum[2] += c[2];
            }
        }
        let center = normal(sum);

        // Construct an orthonormal tangent basis (east, north, center) immune to polar gimbal lock
        let ref_up = if center[2].abs() < 0.95 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let cross = |a: Vec3, b: Vec3| -> Vec3 {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: Vec3, b: Vec3| -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] };
        let east = normal(cross(ref_up, center));
        let north = normal(cross(center, east));

        let project_pt = |v: Vec3| -> [f32; 2] {
            let x = dot(v, east) as f32;
            let y = -dot(v, north) as f32;
            [x, y]
        };

        // Compute bounding box over state_tiles + 1-hop neighbors so the state fills the viewport cleanly
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for &f in &ring1 {
            let Some((fc, verts)) = self.terrain.terrain_grid.faces.get(f) else {
                continue;
            };
            if dot(*fc, center) <= 0.15 {
                continue;
            }
            for &v in verts {
                let [x, y] = project_pt(v);
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
        if !min_x.is_finite() || !max_x.is_finite() {
            return Vec::new();
        }

        let span_x = (max_x - min_x).max(1e-4);
        let span_y = (max_y - min_y).max(1e-4);
        let span = span_x.max(span_y) * 1.12;
        let mid_x = 0.5 * (min_x + max_x);
        let mid_y = 0.5 * (min_y + max_y);

        let capital_tile = self.states.get(&state_id).map(|s| s.capital_tile);
        let state_color_idx = self
            .states
            .get(&state_id)
            .map(|s| s.color_idx)
            .unwrap_or((state_id as usize) % NATION_PALETTE.len());
        let base_state_rgb = NATION_PALETTE[state_color_idx];

        let mut polygons = Vec::with_capacity(all_faces.len());
        for &f in &all_faces {
            let Some((fc, verts)) = self.terrain.terrain_grid.faces.get(f) else {
                continue;
            };
            if dot(*fc, center) <= 0.15 {
                continue;
            }
            let points_xy: Vec<[f32; 2]> = verts
                .iter()
                .map(|&v| {
                    let [x, y] = project_pt(v);
                    [
                        0.5 + (x - mid_x) / span,
                        0.5 + (y - mid_y) / span,
                    ]
                })
                .collect();

            let elev = self.terrain.fine_elevations.get(f).copied().unwrap_or(0.0);
            let is_river = self.river_tiles.get(f).copied().unwrap_or(false);
            let f_state = self.tile_states.get(&f).copied();
            let is_in_selected_state = f_state == Some(state_id);
            let is_selected_tile = selected_fine_idx == Some(f);
            let is_capital = capital_tile == Some(f);

            let shade = (0.82 + (elev / 8.0).clamp(0.0, 0.30)).clamp(0.75, 1.12);
            let rgb = if elev <= 0.0 {
                [0.07, 0.20, 0.44]
            } else if is_in_selected_state {
                if is_river {
                    [
                        (base_state_rgb[0] * 0.55 + 0.20).clamp(0.0, 1.0),
                        (base_state_rgb[1] * 0.55 + 0.40).clamp(0.0, 1.0),
                        (base_state_rgb[2] * 0.55 + 0.44).clamp(0.0, 1.0),
                    ]
                } else {
                    [
                        (base_state_rgb[0] * shade).clamp(0.0, 1.0),
                        (base_state_rgb[1] * shade).clamp(0.0, 1.0),
                        (base_state_rgb[2] * shade).clamp(0.0, 1.0),
                    ]
                }
            } else if let Some(other_sid) = f_state {
                let oc_idx = self
                    .states
                    .get(&other_sid)
                    .map(|s| s.color_idx)
                    .unwrap_or((other_sid as usize) % NATION_PALETTE.len());
                let oc = NATION_PALETTE[oc_idx];
                [oc[0] * 0.52, oc[1] * 0.52, oc[2] * 0.52]
            } else if is_river {
                [0.18, 0.48, 0.54]
            } else {
                [0.20 * shade, 0.29 * shade, 0.19 * shade]
            };

            polygons.push(ZoomedMapPolygon {
                display_idx: f,
                fine_idx: f,
                state_id: f_state,
                points_xy,
                rgb,
                is_in_selected_state,
                is_selected_tile,
                is_capital,
                is_river,
            });
        }
        polygons
    }

    /// Inspect the state ruling `display_idx` (if `display_idx` is a fine face belonging to a state).
    pub fn inspect_state_for_tile(&self, display_idx: usize) -> Option<StateInspectionView> {
        let face = self.terrain.display_faces.get(display_idx)?;
        let DisplayFaceSource::Fine(fine_idx) = face.source else {
            return None;
        };
        let &state_id = self.tile_states.get(&fine_idx)?;
        self.inspect_state(state_id, Some(fine_idx))
    }

    /// Structured inspection report + zoomed-in map for `state_id`.
    pub fn inspect_state(
        &self,
        state_id: u32,
        selected_fine_idx: Option<usize>,
    ) -> Option<StateInspectionView> {
        let fine_adj = &self.terrain.terrain_adj;
        let StateTopology {
            extents,
            species: state_species,
            ..
        } = self.state_topology(fine_adj);
        let tiles = extents.get(&state_id)?;
        if tiles.is_empty() {
            return None;
        }

        let st_name = self.state_name(state_id);
        let cidx = self
            .states
            .get(&state_id)
            .map(|st| st.color_idx)
            .unwrap_or((state_id as usize) % NATION_PALETTE.len());
        let color_rgb = NATION_PALETTE[cidx];

        let coarse_elevs = self.prehistory.tile_elevations();
        let dom_lang = self.primary_language_for_state(state_id);
        let mut cont_counts: BTreeMap<String, usize> = BTreeMap::new();
        for &f in tiles {
            let c = self.terrain.primary_coarse_tile(f, &coarse_elevs);
            let cname = self.continent_label_for_tile(c, dom_lang);
            *cont_counts.entry(cname).or_insert(0) += 1;
        }
        let mut sorted_conts: Vec<(String, usize)> = cont_counts.into_iter().collect();
        sorted_conts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let continents = sorted_conts
            .into_iter()
            .map(|(n, _)| n)
            .collect::<Vec<_>>()
            .join(", ");

        let mut nat_totals: BTreeMap<u32, f32> = BTreeMap::new();
        let mut lang_totals: BTreeMap<u32, f32> = BTreeMap::new();
        let mut st_pop = 0.0f32;
        let mut agri_pop = 0.0f32;
        let mut riv_pop = 0.0f32;
        let mut sea_pop = 0.0f32;
        for &t in tiles {
            if let Some(comms) = self.tile_communities.get(&t) {
                for c in comms {
                    if c.thousands > 1e-4 {
                        st_pop += c.thousands;
                        if c.agriculture {
                            agri_pop += c.thousands;
                        }
                        if c.rivergoing {
                            riv_pop += c.thousands;
                        }
                        if c.seafaring {
                            sea_pop += c.thousands;
                        }
                        *nat_totals.entry(c.nationality_id).or_insert(0.0) += c.thousands;
                        *lang_totals.entry(c.language_id).or_insert(0.0) += c.thousands;
                    }
                }
            }
        }
        let st_cap = self.state_carrying_capacity(tiles);

        let native_sp = state_species.get(&state_id).cloned().unwrap_or_default();
        let direct_partners = self.lookup_trade_partners(state_id);
        let net_partners = self.recursive_trade_partners(state_id);
        let mut avail_sp: BTreeSet<usize> = native_sp.clone();
        for &p in &net_partners {
            if let Some(psp) = state_species.get(&p) {
                avail_sp.extend(psp.iter().copied());
            }
        }
        let denom_pop = st_pop.max(1e-4);
        let mut tech_parts = Vec::new();
        if agri_pop > 0.0 {
            let pct = (agri_pop * 100.0 / denom_pop).clamp(0.0, 100.0);
            tech_parts.push(format!(
                "Agricultural ({:.0}% farming, {} domesticates)",
                pct,
                avail_sp.len()
            ));
        } else {
            tech_parts.push("Pre-Agricultural / Foraging (0% farming)".to_string());
        }
        if riv_pop > 0.0 {
            let pct = (riv_pop * 100.0 / denom_pop).clamp(0.0, 100.0);
            tech_parts.push(format!("Rivergoing ({:.0}%)", pct));
        }
        if sea_pop > 0.0 {
            let pct = (sea_pop * 100.0 / denom_pop).clamp(0.0, 100.0);
            tech_parts.push(format!("Seafaring ({:.0}%)", pct));
        }
        let technology = tech_parts.join(" · ");

        let st_asabiya = self.state_asabiya(state_id);
        let frontier_label = if st_asabiya >= 0.70 {
            "Marcher Frontier"
        } else if st_asabiya <= 0.35 {
            "Imperial Core"
        } else {
            "Cohesive"
        };
        let asabiya_text = format!("{:.0}% ({frontier_label})", st_asabiya * 100.0);

        let mut nationalities: Vec<DemographicShareItem> = nat_totals
            .into_iter()
            .filter(|&(_, p)| p > 1e-4)
            .map(|(nid, p)| DemographicShareItem {
                id: nid,
                name: self.nation_name(nid, dom_lang),
                thousands: p,
                percent_text: format_share_percent(p, st_pop),
            })
            .collect();
        nationalities.sort_by(|a, b| {
            b.thousands
                .partial_cmp(&a.thousands)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut languages: Vec<DemographicShareItem> = lang_totals
            .into_iter()
            .filter(|&(_, p)| p > 1e-4)
            .map(|(lid, p)| {
                let lname = self
                    .languages
                    .get(&lid)
                    .map(|l| l.name())
                    .unwrap_or_else(|| format!("Lang_{}", lid + 1));
                DemographicShareItem {
                    id: lid,
                    name: lname,
                    thousands: p,
                    percent_text: format_share_percent(p, st_pop),
                }
            })
            .collect();
        languages.sort_by(|a, b| {
            b.thousands
                .partial_cmp(&a.thousands)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Exports: native domesticates of this state demanded by trading partners (or available for export)
        let mut exports = Vec::new();
        for &sp_idx in &native_sp {
            let Some(sp) = self.settled_species.get(sp_idx) else {
                continue;
            };
            let kind_str = match sp.kind {
                SpeciesKind::Animal => "fauna",
                SpeciesKind::Plant => "flora",
            };
            let buyers: Vec<String> = direct_partners
                .iter()
                .chain(net_partners.iter().filter(|p| !direct_partners.contains(p)))
                .filter(|&&p| {
                    state_species
                        .get(&p)
                        .is_none_or(|psp| !psp.contains(&sp_idx))
                })
                .take(3)
                .map(|&p| self.state_name(p))
                .collect();
            if !buyers.is_empty() {
                exports.push(format!("{} ({kind_str}) → {}", sp.name, buyers.join(", ")));
            } else {
                exports.push(format!("{} ({kind_str}, domestic surplus)", sp.name));
            }
        }

        // Imports: foreign domesticates acquired via trade partners
        let mut imports = Vec::new();
        for &sp_idx in avail_sp.difference(&native_sp) {
            let Some(sp) = self.settled_species.get(sp_idx) else {
                continue;
            };
            let kind_str = match sp.kind {
                SpeciesKind::Animal => "fauna",
                SpeciesKind::Plant => "flora",
            };
            let sellers: Vec<String> = direct_partners
                .iter()
                .chain(net_partners.iter().filter(|p| !direct_partners.contains(p)))
                .filter(|&&p| {
                    state_species
                        .get(&p)
                        .is_some_and(|psp| psp.contains(&sp_idx))
                })
                .take(2)
                .map(|&p| self.state_name(p))
                .collect();
            if !sellers.is_empty() {
                imports.push(format!("{} ({kind_str}) ← {}", sp.name, sellers.join(", ")));
            } else {
                imports.push(format!("{} ({kind_str})", sp.name));
            }
        }

        let trade_partners: Vec<String> = direct_partners
            .iter()
            .map(|&p| self.state_name(p))
            .collect();
        let conflicts: Vec<String> = self
            .lookup_opponents(state_id)
            .into_iter()
            .map(|r| self.state_name(r))
            .collect();

        let mut water_routes: Vec<String> = self
            .travel_routes
            .iter()
            .filter_map(|r| {
                let s_from = self.tile_states.get(&r.from_fine_face).copied();
                let s_to = self.tile_states.get(&r.to_fine_face).copied();
                if s_from == Some(state_id) {
                    let dst_label = s_to
                        .map(|s| self.state_name(s))
                        .unwrap_or_else(|| format!("Coast #{}", r.to_coarse_tile));
                    Some(format!("→ {dst_label} ({})", r.kind_name()))
                } else if s_to == Some(state_id) {
                    let src_label = s_from
                        .map(|s| self.state_name(s))
                        .unwrap_or_else(|| format!("Coast #{}", r.from_coarse_tile));
                    Some(format!("← {src_label} ({})", r.kind_name()))
                } else {
                    None
                }
            })
            .collect();
        water_routes.dedup();

        let map_polygons = self.build_state_zoomed_map(state_id, selected_fine_idx);

        Some(StateInspectionView {
            state_id,
            state_name: st_name,
            color_rgb,
            continents,
            technology,
            population_text: format_thousands_compact(st_pop),
            capacity_text: format_thousands_compact(st_cap),
            tile_count: tiles.len(),
            asabiya_text,
            nationalities,
            languages,
            exports,
            imports,
            trade_partners,
            conflicts,
            water_routes,
            map_polygons,
        })
    }

    /// Build the local 2D sky view configuration and ephemeris as seen from `display_idx`
    /// starting at Day 0, Year 0 (`0` = beginning of history).
    pub fn build_tile_sky_view(
        &self,
        display_idx: usize,
        primary_lang: Option<u32>,
    ) -> TileSkyView {
        let center = self
            .terrain
            .display_faces
            .get(display_idx)
            .map(|f| f.center)
            .unwrap_or([1.0, 0.0, 0.0]);
        let lat_deg = (center[2].clamp(-1.0, 1.0).asin() * 180.0 / std::f64::consts::PI) as f32;
        let lon_deg = (center[1].atan2(center[0]) * 180.0 / std::f64::consts::PI) as f32;
        let tilt_deg = (self.prehistory.tilt_deg() as f32).clamp(0.0, 89.0);
        let spin = (self.prehistory.spin() as f32).max(0.1);
        let day_hours = (24.0 / spin).clamp(2.0, 240.0);

        // Home world orbital parameters
        let home_planet = self.solar_system.iter().find(|p| p.is_home_world);
        let home_orbit_au = home_planet
            .map(|p| p.orbit_au)
            .unwrap_or(self.home_star.habitable_au)
            .max(0.02);
        let earth_days_period = home_planet
            .map(|p| p.orbital_period_days)
            .unwrap_or(self.home_star.orbital_period_days)
            .max(5.0);
        let home_period_days = (earth_days_period * spin).clamp(8.0, 5000.0);
        let days_per_year = home_period_days.round().clamp(8.0, 5000.0) as u32;
        let home_init_angle_rad = home_planet.map(|p| p.orbit_angle_rad).unwrap_or(0.0);

        let lang_rec = primary_lang.and_then(|lid| self.languages.get(&lid));
        let sun_name = lang_rec
            .and_then(|l| l.lexicon.get(&ConceptKey::Sun(0)))
            .map(|orig| format!("{} (Sun)", orig.word.write(true)))
            .unwrap_or_else(|| "Sun".to_string());

        let base_lunar_days = (27.32166 * spin).max(2.0);
        let mut moons = Vec::with_capacity(self.moon_count as usize);
        for m in 0..self.moon_count {
            let period_days = base_lunar_days * ((1u32 << m.min(6)) as f32);
            let inclination_deg = 5.14 + (m as f32) * 1.25;
            let init_angle_rad = ((m as f32) * 1.854) % std::f32::consts::TAU;
            let node_asc_rad = ((m as f32) * 2.399) % std::f32::consts::TAU;
            let name = lang_rec
                .and_then(|l| l.lexicon.get(&ConceptKey::Moon(m)))
                .map(|orig| format!("{} (M{})", orig.word.write(true), m + 1))
                .unwrap_or_else(|| format!("Moon {}", m + 1));
            moons.push(TileSkyMoon {
                index: m,
                name,
                period_days,
                inclination_deg,
                init_angle_rad,
                node_asc_rad,
            });
        }

        let mut planets = Vec::new();
        for p in &self.solar_system {
            if p.is_home_world {
                continue;
            }
            let period_days = (p.orbital_period_days * spin).max(2.0);
            let inclination_deg = 1.1 + ((p.index % 5) as f32) * 0.65;
            let name = lang_rec
                .and_then(|l| l.lexicon.get(&ConceptKey::Planet(p.index)))
                .map(|orig| format!("{} (P{})", orig.word.write(true), p.index + 1))
                .unwrap_or_else(|| format!("Planet {}", p.index + 1));
            planets.push(TileSkyPlanet {
                index: p.index,
                name,
                orbit_au: p.orbit_au.max(0.02),
                period_days,
                init_angle_rad: p.orbit_angle_rad,
                inclination_deg,
                rgb: p.rgb,
                has_rings: p.has_rings,
            });
        }

        let mut constellations = Vec::with_capacity(self.constellations.len());
        for c in &self.constellations {
            let n_stars = (c.star_count as usize).clamp(3, 7);
            let seed_u64 = (c.index as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (c.right_ascension_deg.to_bits() as u64)
                ^ ((c.declination_deg.to_bits() as u64) << 16);
            let mut crng = Pcg64Mcg::from_seed_u64(seed_u64);
            let c_dir = normal(c.center_dir);
            let raw_east = [-c_dir[1], c_dir[0], 0.0];
            let east_norm =
                (raw_east[0] * raw_east[0] + raw_east[1] * raw_east[1]).sqrt();
            let e_ra = if east_norm > 1e-5 {
                [raw_east[0] / east_norm, raw_east[1] / east_norm, 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let e_dec = normal([
                c_dir[1] * e_ra[2] - c_dir[2] * e_ra[1],
                c_dir[2] * e_ra[0] - c_dir[0] * e_ra[2],
                c_dir[0] * e_ra[1] - c_dir[1] * e_ra[0],
            ]);

            let mut stars = Vec::with_capacity(n_stars);
            stars.push([
                c.right_ascension_deg,
                c.declination_deg,
                c.brightest_mag.clamp(0.1, 3.0),
            ]);
            for k in 1..n_stars {
                let ang = (k as f64) * std::f64::consts::TAU / ((n_stars - 1) as f64)
                    + crng.uniform(-0.32, 0.32);
                let rad = crng.uniform(0.055, 0.135);
                let dx = rad * (ang.cos() * e_ra[0] + ang.sin() * e_dec[0]);
                let dy = rad * (ang.cos() * e_ra[1] + ang.sin() * e_dec[1]);
                let dz = rad * (ang.cos() * e_ra[2] + ang.sin() * e_dec[2]);
                let s_dir = normal([c_dir[0] + dx, c_dir[1] + dy, c_dir[2] + dz]);
                let s_dec = (s_dir[2].clamp(-1.0, 1.0).asin() * 180.0 / std::f64::consts::PI) as f32;
                let s_ra = ((s_dir[1].atan2(s_dir[0]) * 180.0 / std::f64::consts::PI)
                    .rem_euclid(360.0)) as f32;
                let s_mag = (c.brightest_mag + 0.25 * (k as f32)).clamp(0.4, 3.4);
                stars.push([s_ra, s_dec, s_mag]);
            }

            let mut edges = Vec::with_capacity(n_stars);
            for k in 0..(n_stars - 1) {
                edges.push([k, k + 1]);
            }
            if n_stars >= 4 && c.index % 2 == 0 {
                edges.push([0, n_stars - 1]);
            } else if n_stars >= 5 {
                edges.push([0, 2]);
            }

            let ck = ConceptKey::Constellation(c.index);
            let name = lang_rec
                .and_then(|l| l.lexicon.get(&ck))
                .or_else(|| self.languages.values().find_map(|l| l.lexicon.get(&ck)))
                .map(|orig| orig.word.write(true))
                .unwrap_or_default();

            constellations.push(TileSkyConstellation {
                index: c.index,
                name,
                ra_deg: c.right_ascension_deg,
                dec_deg: c.declination_deg,
                stars,
                edges,
            });
        }

        TileSkyView {
            lat_deg,
            lon_deg,
            tilt_deg,
            days_per_year,
            day_hours,
            current_history_year: self.years(),
            sun_name,
            sun_rgb: self.home_star.rgb,
            sky_zenith_rgb: self.optics.sky_zenith_rgb,
            sky_horizon_rgb: self.optics.sky_horizon_rgb,
            sunset_rgb: self.optics.sunset_rgb,
            twilight_rgb: self.optics.twilight_rgb,
            home_orbit_au,
            home_period_days,
            home_init_angle_rad,
            moons,
            planets,
            constellations,
        }
    }

    /// Structured inspection report for a single clicked tile (`display_idx`), including tile-level
    /// nationalities, languages, native flora & fauna, and local 2D sky ephemeris.
    pub fn inspect_tile_structured(&self, display_idx: usize) -> Option<TileInspectionView> {
        let face = self.terrain.display_faces.get(display_idx)?;
        let fine_idx = match face.source {
            DisplayFaceSource::OceanCoarse(c_idx) => {
                let ocean_label = self.ocean_and_sea_label_for_tile(c_idx, None);
                let elev = self
                    .prehistory
                    .tiles()
                    .get(c_idx)
                    .map(|t| t.elevation())
                    .unwrap_or(-4.0);
                let sky = self.build_tile_sky_view(display_idx, None);
                return Some(TileInspectionView {
                    display_idx,
                    fine_idx: None,
                    state_id: None,
                    state_name: None,
                    title: format!("Ocean Tile #{c_idx}"),
                    location_summary: format!("{ocean_label} | Bathymetry: {elev:.2} km"),
                    population_summary: "Uninhabited Marine Basin".to_string(),
                    technology: "Marine".to_string(),
                    nationalities: Vec::new(),
                    languages: Vec::new(),
                    flora: Vec::new(),
                    fauna: Vec::new(),
                    sky,
                });
            }
            DisplayFaceSource::Fine(f) => f,
        };

        let fine_elev = self.terrain.fine_elevations[fine_idx];
        let coarse_elevs = self.prehistory.tile_elevations();
        let primary_c = self.terrain.primary_coarse_tile(fine_idx, &coarse_elevs);
        if fine_elev <= 0.0 {
            let ocean_label = self.ocean_and_sea_label_for_tile(primary_c, None);
            let sky = self.build_tile_sky_view(display_idx, None);
            return Some(TileInspectionView {
                display_idx,
                fine_idx: Some(fine_idx),
                state_id: None,
                state_name: None,
                title: format!("Coastal Sea Face #{fine_idx}"),
                location_summary: format!("{ocean_label} | Depth: {fine_elev:.2} km"),
                population_summary: "Coastal Waters".to_string(),
                technology: "Maritime".to_string(),
                nationalities: Vec::new(),
                languages: Vec::new(),
                flora: Vec::new(),
                fauna: Vec::new(),
                sky,
            });
        }

        let koeppen_code = self
            .prehistory
            .tile_koeppen_ids()
            .get(primary_c)
            .copied()
            .unwrap_or(0);
        let climate_str = crate::world::koeppen_climate_name(koeppen_code);
        let is_river = self.river_tiles.get(fine_idx).copied().unwrap_or(false);

        let state_id = self.tile_states.get(&fine_idx).copied();
        let state_name = state_id.map(|sid| self.state_name(sid));
        let is_capital = state_id
            .and_then(|sid| self.states.get(&sid))
            .is_some_and(|st| st.capital_tile == fine_idx);
        let dom_lang = state_id.and_then(|sid| self.primary_language_for_state(sid));

        let mut nat_totals: BTreeMap<u32, f32> = BTreeMap::new();
        let mut lang_totals: BTreeMap<u32, f32> = BTreeMap::new();
        let mut tile_pop = 0.0f32;
        let mut agri_pop = 0.0f32;
        let mut riv_pop = 0.0f32;
        let mut sea_pop = 0.0f32;
        if let Some(comms) = self.tile_communities.get(&fine_idx) {
            for c in comms {
                if c.thousands > 1e-4 {
                    tile_pop += c.thousands;
                    if c.agriculture {
                        agri_pop += c.thousands;
                    }
                    if c.rivergoing {
                        riv_pop += c.thousands;
                    }
                    if c.seafaring {
                        sea_pop += c.thousands;
                    }
                    *nat_totals.entry(c.nationality_id).or_insert(0.0) += c.thousands;
                    *lang_totals.entry(c.language_id).or_insert(0.0) += c.thousands;
                }
            }
        }
        let cap = self.capacities[fine_idx];
        let is_coast = self.is_coastal_fine.get(fine_idx).copied().unwrap_or(false);

        let technology = if tile_pop <= 1e-4 {
            "Uninhabited".to_string()
        } else {
            let denom = tile_pop.max(1e-4);
            let mut parts = Vec::new();
            if agri_pop > 0.0 {
                let pct = (agri_pop * 100.0 / denom).clamp(0.0, 100.0);
                parts.push(format!("Agricultural ({:.0}% farming)", pct));
            } else {
                parts.push("Pre-Agricultural / Foraging".to_string());
            }
            if riv_pop > 0.0 {
                let pct = (riv_pop * 100.0 / denom).clamp(0.0, 100.0);
                if is_river {
                    parts.push(format!("Rivergoing ({:.0}%, +35% river fishing)", pct));
                } else {
                    parts.push(format!("Rivergoing ({:.0}%)", pct));
                }
            }
            if sea_pop > 0.0 {
                let pct = (sea_pop * 100.0 / denom).clamp(0.0, 100.0);
                if is_coast {
                    parts.push(format!("Seafaring ({:.0}%, +45% marine fishing)", pct));
                } else {
                    parts.push(format!("Seafaring ({:.0}%)", pct));
                }
            }
            parts.join(" · ")
        };

        let mut nationalities: Vec<DemographicShareItem> = nat_totals
            .into_iter()
            .filter(|&(_, p)| p > 1e-4)
            .map(|(nid, p)| DemographicShareItem {
                id: nid,
                name: self.nation_name(nid, dom_lang),
                thousands: p,
                percent_text: format_share_percent(p, tile_pop),
            })
            .collect();
        nationalities.sort_by(|a, b| {
            b.thousands
                .partial_cmp(&a.thousands)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut languages: Vec<DemographicShareItem> = lang_totals
            .into_iter()
            .filter(|&(_, p)| p > 1e-4)
            .map(|(lid, p)| {
                let lname = self
                    .languages
                    .get(&lid)
                    .map(|l| l.name())
                    .unwrap_or_else(|| format!("Lang_{}", lid + 1));
                DemographicShareItem {
                    id: lid,
                    name: lname,
                    thousands: p,
                    percent_text: format_share_percent(p, tile_pop),
                }
            })
            .collect();
        languages.sort_by(|a, b| {
            b.thousands
                .partial_cmp(&a.thousands)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut flora = Vec::new();
        let mut fauna = Vec::new();
        for &sp_idx in &self.tile_species[fine_idx] {
            if let Some(sp) = self.settled_species.get(sp_idx) {
                match sp.kind {
                    SpeciesKind::Plant => flora.push(sp.name.clone()),
                    SpeciesKind::Animal => fauna.push(sp.name.clone()),
                }
            }
        }

        let primary_tile_lang = languages.first().map(|l| l.id).or(dom_lang);
        let cont_name = self.continent_label_for_tile(primary_c, primary_tile_lang);

        let mut geo_extras = Vec::new();
        if let Some(&rid) = self
            .fine_face_rivers
            .get(fine_idx)
            .and_then(|v| v.first())
        {
            if let Some(rw) =
                self.concept_word_in_language(ConceptKey::River(rid), primary_tile_lang)
            {
                geo_extras.push(format!("River: {rw}"));
            } else if is_river {
                geo_extras.push(format!("River #{}", rid + 1));
            }
        } else if is_river {
            geo_extras.push("River Corridor".to_string());
        }
        if let Some(mid) = self
            .geography
            .tile_mountain_range
            .get(primary_c)
            .copied()
            .flatten()
        {
            let mdesc = self
                .geography
                .mountain_ranges
                .get(mid)
                .map(|m| m.name.as_str())
                .unwrap_or("Mountains");
            if let Some(mw) = self
                .concept_word_in_language(ConceptKey::MountainRange(mid as u32), primary_tile_lang)
            {
                geo_extras.push(format!("Mountains: {mw} ({mdesc})"));
            } else {
                geo_extras.push(format!("Mountains: {mdesc}"));
            }
        }
        if is_coast {
            let coarse_adj = self.prehistory.adjacency();
            let adj_sea_id = self
                .geography
                .tile_sea
                .get(primary_c)
                .copied()
                .flatten()
                .or_else(|| {
                    coarse_adj.get(primary_c)?.iter().find_map(|&nb| {
                        self.geography.tile_sea.get(nb).copied().flatten()
                    })
                });
            if let Some(sea_id) = adj_sea_id {
                let sdesc = self
                    .geography
                    .seas
                    .get(sea_id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("Coastal Sea");
                if let Some(sw) = self
                    .concept_word_in_language(ConceptKey::Sea(sea_id as u32), primary_tile_lang)
                {
                    geo_extras.push(format!("Sea: {sw} ({sdesc})"));
                } else {
                    geo_extras.push(format!("Sea: {sdesc}"));
                }
            }
        }

        let title = match (&state_name, is_capital) {
            (Some(sname), true) => format!("Tile #{fine_idx} — ★ {sname} Capital"),
            (Some(sname), false) => format!("Tile #{fine_idx} ({sname})"),
            (None, _) => format!("Tile #{fine_idx} (Unclaimed Land)"),
        };
        let extra_suffix = if geo_extras.is_empty() {
            String::new()
        } else {
            format!(" | {}", geo_extras.join(" | "))
        };
        let location_summary =
            format!("Continent: {cont_name} | Elev: {fine_elev:.2} km | {climate_str}{extra_suffix}");
        let population_summary = format!(
            "Pop: {} / Carrying Cap: {}",
            format_thousands_compact(tile_pop),
            format_thousands_compact(cap)
        );

        let sky = self.build_tile_sky_view(display_idx, primary_tile_lang);

        Some(TileInspectionView {
            display_idx,
            fine_idx: Some(fine_idx),
            state_id,
            state_name,
            title,
            location_summary,
            population_summary,
            technology,
            nationalities,
            languages,
            flora,
            fauna,
            sky,
        })
    }

    /// Detailed inspection of a single language (`lang_id`): full lexicon, who speaks it, and where.
    pub fn inspect_language(&self, lang_id: u32) -> Option<LanguageDetailView> {
        let lang = self.languages.get(&lang_id)?;
        let coarse_elevs = self.prehistory.tile_elevations();

        let mut total_thousands = 0.0f32;
        let mut total_tiles = 0usize;
        let mut by_location: BTreeMap<(Option<u32>, String), (f32, usize)> = BTreeMap::new();
        let mut by_nation: BTreeMap<u32, f32> = BTreeMap::new();

        for (&f_idx, comms) in &self.tile_communities {
            let mut tile_lang_pop = 0.0f32;
            for c in comms {
                if c.language_id == lang_id && c.thousands > 1e-4 {
                    tile_lang_pop += c.thousands;
                    *by_nation.entry(c.nationality_id).or_insert(0.0) += c.thousands;
                }
            }
            if tile_lang_pop > 1e-4 {
                total_thousands += tile_lang_pop;
                total_tiles += 1;
                let sid_opt = self.tile_states.get(&f_idx).copied();
                let c_idx = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                let cont = self
                    .geography
                    .continent_name(c_idx)
                    .unwrap_or("Mainland")
                    .to_string();
                let entry = by_location.entry((sid_opt, cont)).or_insert((0.0, 0));
                entry.0 += tile_lang_pop;
                entry.1 += 1;
            }
        }

        let mut loc_vec: Vec<((Option<u32>, String), (f32, usize))> =
            by_location.into_iter().collect();
        loc_vec.sort_by(|a, b| {
            b.1 .0
                .partial_cmp(&a.1 .0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let speakers_by_location: Vec<String> = loc_vec
            .into_iter()
            .map(|((sid_opt, cont), (pop, tcount))| {
                let polity = sid_opt
                    .map(|s| self.state_name(s))
                    .unwrap_or_else(|| "Unclaimed Land".to_string());
                format!(
                    "{polity} ({cont}, {tcount} {}): {} ({})",
                    if tcount == 1 { "tile" } else { "tiles" },
                    format_thousands_compact(pop),
                    format_share_percent(pop, total_thousands)
                )
            })
            .collect();

        let mut speakers_by_nation: Vec<DemographicShareItem> = by_nation
            .into_iter()
            .filter(|&(_, p)| p > 1e-4)
            .map(|(nid, p)| DemographicShareItem {
                id: nid,
                name: self.nation_name(nid, Some(lang_id)),
                thousands: p,
                percent_text: format_share_percent(p, total_thousands),
            })
            .collect();
        speakers_by_nation.sort_by(|a, b| {
            b.thousands
                .partial_cmp(&a.thousands)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let phoneme_summary = format!(
            "{} consonants, {} vowels",
            lang.phonology.consonants.len(),
            lang.phonology.vowels.len()
        );

        let lexicon: Vec<LexiconEntryView> = lang
            .lexicon
            .iter()
            .map(|(&ck, orig)| LexiconEntryView {
                category: Self::concept_category_label(ck).to_string(),
                gloss: self.format_concept_gloss(ck),
                orthography: orig.word.write(false),
                ipa: orig.word.pronounce(),
                etymology: orig.format_etymology(),
            })
            .collect();

        Some(LanguageDetailView {
            lang_id,
            name: lang.name(),
            total_speakers_text: format!(
                "{} across {} {}",
                format_thousands_compact(total_thousands),
                total_tiles,
                if total_tiles == 1 { "tile" } else { "tiles" }
            ),
            phoneme_summary,
            speakers_by_location,
            speakers_by_nation,
            lexicon,
        })
    }

    /// Detailed inspection of a single nationality (`nat_id`): where they originate, where they currently live,
    /// and what languages they speak.
    pub fn inspect_nation(&self, nat_id: u32) -> Option<NationDetailView> {
        if !self.nationalities.contains(&nat_id) && !self.states.contains_key(&nat_id) {
            return None;
        }
        let coarse_elevs = self.prehistory.tile_elevations();

        let mut total_thousands = 0.0f32;
        let mut total_tiles = 0usize;
        let mut by_location: BTreeMap<(Option<u32>, String), (f32, usize)> = BTreeMap::new();
        let mut by_lang: BTreeMap<u32, f32> = BTreeMap::new();
        let mut heritage_weights: BTreeMap<u32, f32> = BTreeMap::new();

        for (&f_idx, comms) in &self.tile_communities {
            let mut tile_nat_pop = 0.0f32;
            for c in comms {
                if c.nationality_id == nat_id && c.thousands > 1e-4 {
                    tile_nat_pop += c.thousands;
                    *by_lang.entry(c.language_id).or_insert(0.0) += c.thousands;
                    for (&hid, &w) in &c.racial_mix {
                        *heritage_weights.entry(hid).or_insert(0.0) += c.thousands * w;
                    }
                }
            }
            if tile_nat_pop > 1e-4 {
                total_thousands += tile_nat_pop;
                total_tiles += 1;
                let sid_opt = self.tile_states.get(&f_idx).copied();
                let c_idx = self.terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                let cont = self
                    .geography
                    .continent_name(c_idx)
                    .unwrap_or("Mainland")
                    .to_string();
                let entry = by_location.entry((sid_opt, cont)).or_insert((0.0, 0));
                entry.0 += tile_nat_pop;
                entry.1 += 1;
            }
        }

        let mut spoken_languages: Vec<DemographicShareItem> = by_lang
            .into_iter()
            .filter(|&(_, p)| p > 1e-4)
            .map(|(lid, p)| {
                let lname = self
                    .languages
                    .get(&lid)
                    .map(|l| l.name())
                    .unwrap_or_else(|| format!("Lang_{}", lid + 1));
                DemographicShareItem {
                    id: lid,
                    name: lname,
                    thousands: p,
                    percent_text: format_share_percent(p, total_thousands),
                }
            })
            .collect();
        spoken_languages.sort_by(|a, b| {
            b.thousands
                .partial_cmp(&a.thousands)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let dom_lang = spoken_languages.first().map(|l| l.id).or(Some(nat_id));
        let name = self.nation_name(nat_id, dom_lang);

        let origin_summary = if let Some(founding_state) = self.states.get(&nat_id) {
            let cap_f = founding_state.capital_tile;
            let c_idx = self.terrain.primary_coarse_tile(cap_f, &coarse_elevs);
            let cont = self.geography.continent_name(c_idx).unwrap_or("Mainland");
            let lat_deg = self.fine_face_latitude_deg(cap_f);
            let lat_label = if lat_deg >= 0.0 {
                format!("{:.0}°N", lat_deg)
            } else {
                format!("{:.0}°S", -lat_deg)
            };
            let current_ruler = self
                .tile_states
                .get(&cap_f)
                .copied()
                .map(|sid| self.state_name(sid))
                .unwrap_or_else(|| "Unclaimed".to_string());
            let ancestral_lang = self
                .languages
                .get(&nat_id)
                .map(|l| l.name())
                .unwrap_or_else(|| format!("Lang_{}", nat_id + 1));
            format!(
                "Founded at City Tile #{cap_f} in {cont} ({lat_label}); ancestral tongue {ancestral_lang}; homeland now ruled by {current_ruler}"
            )
        } else {
            format!("Indigenous lineage #{}", nat_id + 1)
        };

        let mut sorted_heritages: Vec<(u32, f32)> = heritage_weights.into_iter().collect();
        sorted_heritages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let heritage_summary = if sorted_heritages.is_empty() {
            "Prehistoric ancestry unrecorded".to_string()
        } else {
            let parts: Vec<String> = sorted_heritages
                .into_iter()
                .take(4)
                .map(|(hid, w)| {
                    let hname = self
                        .prehistory
                        .heritages()
                        .get(&hid)
                        .map(|h| h.name.clone())
                        .unwrap_or_else(|| format!("people_{}", hid + 1));
                    format!("{hname} ({})", format_share_percent(w, total_thousands))
                })
                .collect();
            format!("Prehistoric Heritage: {}", parts.join(", "))
        };

        let mut loc_vec: Vec<((Option<u32>, String), (f32, usize))> =
            by_location.into_iter().collect();
        loc_vec.sort_by(|a, b| {
            b.1 .0
                .partial_cmp(&a.1 .0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let current_locations: Vec<String> = loc_vec
            .into_iter()
            .map(|((sid_opt, cont), (pop, tcount))| {
                let polity = sid_opt
                    .map(|s| self.state_name(s))
                    .unwrap_or_else(|| "Unclaimed Land".to_string());
                format!(
                    "{polity} ({cont}, {tcount} {}): {} ({})",
                    if tcount == 1 { "tile" } else { "tiles" },
                    format_thousands_compact(pop),
                    format_share_percent(pop, total_thousands)
                )
            })
            .collect();

        Some(NationDetailView {
            nat_id,
            name,
            total_pop_text: format!(
                "{} across {} {}",
                format_thousands_compact(total_thousands),
                total_tiles,
                if total_tiles == 1 { "tile" } else { "tiles" }
            ),
            origin_summary,
            heritage_summary,
            current_locations,
            spoken_languages,
        })
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
                .filter(|f_idx| self.tile_states.contains_key(f_idx))
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
                        rivergoing: c.rivergoing,
                        seafaring: c.seafaring,
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
        if let Ok(arr) = <[u8; 16]>::try_from(snap.rng_state_16b.as_slice()) {
            sim.rng = Pcg64Mcg::from_bytes(arr);
        }
        if let Some(tec11_proto::PlanetStageOneof::History(hist)) = &snap.stage {
            sim.history_ticks = hist.history_ticks;
            if !hist.populated_faces.is_empty() {
                sim.tile_states.clear();
                sim.tile_communities.clear();
                let num_fine = sim.fine_face_count();
                let mut active_sids = BTreeSet::new();
                for pf in &hist.populated_faces {
                    let fid = pf.fine_face_id as usize;
                    if fid < num_fine {
                        sim.tile_states.insert(fid, pf.state_id);
                        active_sids.insert(pf.state_id);
                        sim.next_state_id = sim.next_state_id.max(pf.state_id + 1);
                        if !sim.states.contains_key(&pf.state_id) {
                            sim.states.insert(
                                pf.state_id,
                                StateRecord {
                                    id: pf.state_id,
                                    capital_tile: fid,
                                    color_idx: (pf.state_id as usize) % NATION_PALETTE.len(),
                                    active: true,
                                    asabiya: 0.5,
                                },
                            );
                        }
                        let comms: Vec<Community> = pf
                            .communities
                            .iter()
                            .map(|pc| {
                                sim.nationalities.insert(pc.nationality_id);
                                sim.next_state_id = sim.next_state_id.max(pc.nationality_id + 1);
                                sim.next_language_id =
                                    sim.next_language_id.max(pc.language_id + 1);
                                Community {
                                    thousands: pc.thousands,
                                    nationality_id: pc.nationality_id,
                                    language_id: pc.language_id,
                                    agriculture: pc.agriculture,
                                    rivergoing: pc.rivergoing,
                                    seafaring: pc.seafaring,
                                    racial_mix: pc
                                        .racial_mix
                                        .iter()
                                        .map(|rc| (rc.heritage_id, rc.fraction))
                                        .collect(),
                                }
                            })
                            .collect();
                        sim.tile_communities.insert(fid, comms);
                    }
                }
                for st in sim.states.values_mut() {
                    st.active = active_sids.contains(&st.id);
                }
                for (lid_usize, proto_lang) in hist.languages.iter().enumerate() {
                    let lid = lid_usize as u32;
                    sim.next_language_id = sim.next_language_id.max(lid + 1);
                    if !sim.languages.contains_key(&lid) {
                        let mut lrec = LanguageRecord::new_for_state_with_system(
                            lid,
                            lid,
                            &[],
                            sim.moon_count,
                            &sim.solar_system,
                            &mut sim.rng,
                        );
                        for pc in &proto_lang.concepts {
                            lrec.coin_concept(ConceptKey::from_proto(pc), &mut sim.rng);
                        }
                        lrec.refresh_phonology();
                        sim.languages.insert(lid, lrec);
                    } else if let Some(lrec) = sim.languages.get_mut(&lid) {
                        for pc in &proto_lang.concepts {
                            let ck = ConceptKey::from_proto(pc);
                            if !lrec.describes(ck) {
                                lrec.coin_concept(ck, &mut sim.rng);
                            }
                        }
                        lrec.refresh_phonology();
                    }
                }
                sim.trade_partners = hist
                    .trade_partners
                    .iter()
                    .map(StatePair::from_proto)
                    .collect();
                sim.conflicts = hist.conflicts.iter().map(StatePair::from_proto).collect();
                sim.refresh_fishing_capacities();
                sim.update_travel_routes();
                let fine_adj = sim.terrain.terrain_adj.clone();
                let topo = sim.state_topology(&fine_adj);
                sim.accumulate_and_borrow_concepts(&topo.extents, &topo.species);
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

    #[test]
    fn test_two_level_state_tile_language_and_nation_inspection() {
        let mut sim = HistorySimulation::new_earth(42);
        sim.step();

        let (&sample_fine, &sample_sid) = sim
            .tile_states
            .iter()
            .next()
            .expect("Expected at least one state tile");

        let state_view = sim
            .inspect_state_for_tile(sample_fine)
            .expect("Expected StateInspectionView for claimed fine tile");
        assert_eq!(state_view.state_id, sample_sid);
        assert!(!state_view.state_name.is_empty());
        assert!(!state_view.continents.is_empty());
        assert!(state_view.technology.contains("Agricultural"));
        assert!(!state_view.nationalities.is_empty());
        assert!(!state_view.languages.is_empty());
        assert!(!state_view.map_polygons.is_empty());
        assert!(
            state_view
                .map_polygons
                .iter()
                .any(|p| p.is_selected_tile && p.fine_idx == sample_fine),
            "Zoomed state map should mark the clicked tile as selected"
        );
        assert!(state_view.to_json().contains("\"hasState\":true"));

        let tile_view = sim
            .inspect_tile_structured(sample_fine)
            .expect("Expected TileInspectionView for fine tile");
        assert_eq!(tile_view.state_id, Some(sample_sid));
        assert!(!tile_view.nationalities.is_empty());
        assert!(!tile_view.languages.is_empty());
        assert!(tile_view.sky.days_per_year >= 100);
        assert!(tile_view.sky.sun_name.contains("(Sun)"));
        assert_eq!(tile_view.sky.moons.len(), 1);
        assert!(!tile_view.sky.planets.is_empty());
        assert!(!tile_view.sky.constellations.is_empty());
        assert!(!tile_view.sky.constellations[0].stars.is_empty());
        assert!(!tile_view.sky.constellations[0].edges.is_empty());
        assert!(tile_view.to_json().contains("\"hasTile\":true"));
        assert!(tile_view.to_json().contains("\"sky\":{\"latDeg\":"));

        let lang_id = state_view.languages[0].id;
        let lang_view = sim
            .inspect_language(lang_id)
            .expect("Expected LanguageDetailView");
        assert_eq!(lang_view.lang_id, lang_id);
        assert!(!lang_view.speakers_by_location.is_empty());
        assert!(!lang_view.speakers_by_nation.is_empty());
        assert!(!lang_view.lexicon.is_empty());
        assert!(lang_view.to_json().contains("\"hasLanguage\":true"));

        let nat_id = state_view.nationalities[0].id;
        let nat_view = sim
            .inspect_nation(nat_id)
            .expect("Expected NationDetailView");
        assert_eq!(nat_view.nat_id, nat_id);
        assert!(nat_view.origin_summary.contains("Founded at City Tile"));
        assert!(!nat_view.current_locations.is_empty());
        assert!(!nat_view.spoken_languages.is_empty());
        assert!(nat_view.to_json().contains("\"hasNation\":true"));
    }

    #[test]
    fn test_rivergoing_and_seafaring_technologies_capacity_routes_and_faultlines() {
        // 1. Verify Prehistory spark probabilities & sea-neighbor requirements
        let p_riv_base = crate::prehistory::rivergoing_spark_prob(2);
        assert!(p_riv_base > 0.0);
        assert_eq!(
            crate::prehistory::rivergoing_spark_prob(12),
            0.0,
            "Rivergoing should not spark on EF ice cap tiles"
        );

        let p_sea_base = crate::prehistory::seafaring_spark_prob(3, 1);
        let p_sea_archipelago = crate::prehistory::seafaring_spark_prob(3, 3);
        assert!(p_sea_base > 0.0);
        assert!(p_sea_archipelago > p_sea_base);
        assert_eq!(
            crate::prehistory::seafaring_spark_prob(3, 0),
            0.0,
            "Seafaring requires a coastal tile with at least 1 sea neighbor"
        );

        // 2. Verify History fishing carrying capacity boosts (+35% rivergoing on rivers, +45% seafaring on coasts)
        let mut sim = HistorySimulation::new_earth(42);
        let river_face = (0..sim.fine_face_count())
            .find(|&f| {
                sim.river_tiles[f]
                    && !sim.is_coastal_fine[f]
                    && sim.base_capacities[f] > 0.1
                    && sim.tile_communities.contains_key(&f)
            })
            .expect("Expected a populated inland river fine face");
        let coast_face = (0..sim.fine_face_count())
            .find(|&f| {
                sim.is_coastal_fine[f]
                    && !sim.river_tiles[f]
                    && sim.base_capacities[f] > 0.1
                    && sim.tile_communities.contains_key(&f)
            })
            .expect("Expected a populated non-river coastal fine face");

        for c in sim.tile_communities.get_mut(&river_face).unwrap() {
            c.rivergoing = false;
            c.seafaring = false;
        }
        for c in sim.tile_communities.get_mut(&coast_face).unwrap() {
            c.rivergoing = false;
            c.seafaring = false;
        }
        sim.refresh_fishing_capacities();
        assert!((sim.capacities[river_face] - sim.base_capacities[river_face]).abs() < 1e-5);
        assert!((sim.capacities[coast_face] - sim.base_capacities[coast_face]).abs() < 1e-5);

        for c in sim.tile_communities.get_mut(&river_face).unwrap() {
            c.rivergoing = true;
        }
        for c in sim.tile_communities.get_mut(&coast_face).unwrap() {
            c.seafaring = true;
        }
        sim.refresh_fishing_capacities();
        assert!(
            (sim.capacities[river_face] - sim.base_capacities[river_face] * 1.35).abs() < 1e-4,
            "Rivergoing should grant +35% freshwater fishing capacity on river faces"
        );
        assert!(
            (sim.capacities[coast_face] - sim.base_capacities[coast_face] * 1.45).abs() < 1e-4,
            "Seafaring should grant +45% marine fishing capacity on coastal faces"
        );

        // 3. Verify travel route gating: stripping rivergoing/seafaring deactivates water routes
        for comms in sim.tile_communities.values_mut() {
            for c in comms.iter_mut() {
                c.rivergoing = false;
                c.seafaring = false;
            }
        }
        sim.update_travel_routes();
        assert_eq!(
            sim.active_travel_route_count(),
            0,
            "Water travel routes should require rivergoing or seafaring communities"
        );

        for comms in sim.tile_communities.values_mut() {
            for c in comms.iter_mut() {
                c.rivergoing = true;
                c.seafaring = true;
            }
        }
        sim.update_travel_routes();
        assert!(
            sim.active_travel_route_count() > 0,
            "Restoring rivergoing and seafaring should re-activate water travel routes"
        );

        // 4. Verify metaethnic faultline intensity from rivergoing and seafaring divides
        let fine_adj = sim.terrain.terrain_adj.clone();
        let border_pair = sim.tile_states.iter().find_map(|(&f1, &s1)| {
            fine_adj[f1].iter().find_map(|&f2| {
                let &s2 = sim.tile_states.get(&f2)?;
                (s1 != s2
                    && sim.tile_communities.contains_key(&f1)
                    && sim.tile_communities.contains_key(&f2))
                .then_some((f1, f2))
            })
        });
        if let Some((f1, f2)) = border_pair {
            for &nb in &fine_adj[f1] {
                if let Some(comms) = sim.tile_communities.get_mut(&nb) {
                    for c in comms.iter_mut() {
                        c.nationality_id = 0;
                        c.language_id = 0;
                        c.agriculture = true;
                        c.rivergoing = false;
                        c.seafaring = false;
                    }
                }
            }
            for c in sim.tile_communities.get_mut(&f1).unwrap() {
                c.nationality_id = 0;
                c.language_id = 0;
                c.agriculture = true;
                c.rivergoing = false;
                c.seafaring = false;
            }
            let base_i = sim.tile_metaethnic_intensity(f1, &fine_adj);
            assert_eq!(base_i, 0.0);

            for c in sim.tile_communities.get_mut(&f2).unwrap() {
                c.rivergoing = true;
                c.seafaring = true;
            }
            let water_divide_i = sim.tile_metaethnic_intensity(f1, &fine_adj);
            assert!(
                (water_divide_i - 0.40).abs() < 1e-4,
                "Expected +0.15 rivergoing +0.25 seafaring faultline intensity (0.40), got {water_divide_i}"
            );
        }

        // 5. Proto snapshot roundtrip preserves rivergoing and seafaring
        let bytes = sim.encode_snapshot();
        let mut restored = HistorySimulation::new_earth(99);
        assert!(restored.load_snapshot_bytes(&bytes));
        assert!(restored
            .tile_communities
            .values()
            .flatten()
            .any(|c| c.rivergoing));
        assert!(restored
            .tile_communities
            .values()
            .flatten()
            .any(|c| c.seafaring));
    }

    #[test]
    fn test_lexicon_basic_words_culture_seasons_weather_and_geographic_features() {
        let mut sim = HistorySimulation::new_earth(42);
        sim.step();

        // 1. Every language has all 16 universal basic words (mother, father, water, fire, etc.)
        assert!(!sim.languages.is_empty());
        for lang in sim.languages.values() {
            for bw_id in 0..(BASIC_WORD_GLOSSES.len() as u32) {
                assert!(
                    lang.describes(ConceptKey::BasicWord(bw_id)),
                    "Language {} should have basic word '{}' (#{})",
                    lang.name(),
                    BASIC_WORD_GLOSSES[bw_id as usize],
                    bw_id
                );
            }
            assert!(lang.describes(ConceptKey::Culture(CULTURE_FORAGING)));
            assert!(lang.describes(ConceptKey::Culture(CULTURE_CITY)));
        }

        // 2. Verify cultural words (farming, river fishing, sea fishing, trading, warfare) are coined/borrowed
        let any_farming = sim
            .languages
            .values()
            .any(|l| l.describes(ConceptKey::Culture(CULTURE_FARMING)));
        let any_river_fishing = sim
            .languages
            .values()
            .any(|l| l.describes(ConceptKey::Culture(CULTURE_RIVER_FISHING)));
        let any_sea_fishing = sim
            .languages
            .values()
            .any(|l| l.describes(ConceptKey::Culture(CULTURE_SEA_FISHING)));
        assert!(any_farming, "Expected languages exposed to farming to have CULTURE_FARMING");
        assert!(
            any_river_fishing,
            "Expected languages exposed to rivergoing to have CULTURE_RIVER_FISHING"
        );
        assert!(
            any_sea_fishing,
            "Expected languages exposed to seafaring to have CULTURE_SEA_FISHING"
        );

        // Link two states in trade and conflict and verify trading & warfare words are coined/borrowed
        let sids: Vec<u32> = sim.states.keys().copied().take(3).collect();
        if sids.len() >= 2 {
            sim.trade_partners.push(StatePair::new(sids[0], sids[1]));
            if sids.len() >= 3 {
                sim.conflicts.push(StatePair::new(sids[0], sids[2]));
            }
            let fine_adj = sim.terrain.terrain_adj.clone();
            let topo = sim.state_topology(&fine_adj);
            sim.accumulate_and_borrow_concepts(&topo.extents, &topo.species);
            if let Some(lid0) = sim.primary_language_for_state(sids[0]) {
                let l0 = &sim.languages[&lid0];
                assert!(
                    l0.describes(ConceptKey::Culture(CULTURE_TRADING)),
                    "Trading state language should coin or borrow word for trading / commerce"
                );
                if sids.len() >= 3 {
                    assert!(
                        l0.describes(ConceptKey::Culture(CULTURE_WARFARE)),
                        "Warring state language should coin or borrow word for warfare / battle"
                    );
                }
            }
        }

        // 3. Verify location-dependent seasons & weather:
        //    - Tropical rainforest / savanna lowland tile (Af=1 or Aw=3, low elevation, low latitude)
        //      has no snow/ice and (for Aw=3) has wet/dry season but no monsoon (unless Am=2/Cw=7/Dw=9).
        let coarse_elevs = sim.prehistory.tile_elevations();
        let koeppen = sim.prehistory.tile_koeppen_ids().to_vec();
        let tropical_lowland_tile = (0..sim.fine_face_count()).find(|&f| {
            let c = sim.terrain.primary_coarse_tile(f, &coarse_elevs);
            let k = koeppen[c];
            let elev = sim.terrain.fine_elevations[f];
            let lat_abs = sim.fine_face_latitude_deg(f).abs();
            elev > 0.0 && elev < 1.2 && lat_abs < 15.0 && (k == 1 || k == 3)
        });
        if let Some(tf) = tropical_lowland_tile {
            let t_seasons = sim.seasons_for_tiles(&[tf], &coarse_elevs, &koeppen);
            let t_weather = sim.weather_for_tiles(&[tf], &coarse_elevs, &koeppen);
            assert!(
                !t_seasons.contains(&SEASON_MONSOON),
                "Non-monsoon tropical Af/Aw tile should not experience monsoon season"
            );
            assert!(
                !t_seasons.contains(&SEASON_WINTER),
                "Equatorial lowland tile should not experience thermal winter"
            );
            assert!(
                !t_weather.contains(&WEATHER_SNOW) && !t_weather.contains(&WEATHER_ICE),
                "Equatorial lowland tile should not experience snow or ice"
            );
        }

        let cold_or_high_tile = (0..sim.fine_face_count()).find(|&f| {
            let c = sim.terrain.primary_coarse_tile(f, &coarse_elevs);
            let k = koeppen[c];
            let elev = sim.terrain.fine_elevations[f];
            elev > 0.0 && matches!(k, 9..=13)
        });
        if let Some(cf) = cold_or_high_tile {
            let c_seasons = sim.seasons_for_tiles(&[cf], &coarse_elevs, &koeppen);
            let c_weather = sim.weather_for_tiles(&[cf], &coarse_elevs, &koeppen);
            assert!(c_seasons.contains(&SEASON_WINTER));
            assert!(c_weather.contains(&WEATHER_SNOW));
            assert!(c_weather.contains(&WEATHER_ICE));
            assert!(!c_weather.contains(&WEATHER_SANDSTORM));
        }

        // 4. Verify local geographic features (continents, oceans, seas, rivers, mountain ranges) exist and get named
        assert!(!sim.geography().continents.is_empty());
        assert!(!sim.geography().oceans.is_empty());
        assert!(!sim.geography().seas.is_empty(), "Expected coastal seas to be identified");
        assert!(
            !sim.geography().mountain_ranges.is_empty(),
            "Expected mountain ranges to be identified"
        );
        assert!(!sim.river_routes.is_empty(), "Expected river routes to be traced");

        let any_continent_named = sim
            .languages
            .values()
            .any(|l| l.lexicon.keys().any(|k| matches!(k, ConceptKey::Continent(_))));
        let any_ocean_named = sim
            .languages
            .values()
            .any(|l| l.lexicon.keys().any(|k| matches!(k, ConceptKey::Ocean(_))));
        let any_sea_named = sim
            .languages
            .values()
            .any(|l| l.lexicon.keys().any(|k| matches!(k, ConceptKey::Sea(_))));
        let any_river_named = sim
            .languages
            .values()
            .any(|l| l.lexicon.keys().any(|k| matches!(k, ConceptKey::River(_))));
        let any_mountain_named = sim
            .languages
            .values()
            .any(|l| l.lexicon.keys().any(|k| matches!(k, ConceptKey::MountainRange(_))));
        assert!(any_continent_named);
        assert!(any_ocean_named);
        assert!(any_sea_named);
        assert!(any_river_named);
        assert!(any_mountain_named);

        // 5. Proto roundtrip for all new ConceptKey variants
        for ck in [
            ConceptKey::BasicWord(0),
            ConceptKey::Culture(CULTURE_FARMING),
            ConceptKey::Season(SEASON_MONSOON),
            ConceptKey::Weather(WEATHER_SNOW),
            ConceptKey::Sea(0),
            ConceptKey::River(0),
            ConceptKey::MountainRange(0),
        ] {
            let p = ck.to_proto();
            assert_eq!(ConceptKey::from_proto(&p), ck);
        }
    }
}
