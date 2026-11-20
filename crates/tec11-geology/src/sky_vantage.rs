use std::collections::VecDeque;

use tec11_math_grid::{normal, Pcg64Mcg, SphericalGrid};

use crate::sun_sky::{Constellation, HomeStar, PlanetaryOptics, SolarSystemPlanet};
use crate::tile::TileGeology;

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

pub(crate) fn json_escape(s: &str) -> String {
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

pub(crate) fn json_str_array(items: &[String]) -> String {
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
    pub family_id: Option<u32>,
}

impl DemographicShareItem {
    pub fn to_json(&self) -> String {
        let fid: i64 = match self.family_id {
            Some(f) => f as i64,
            None => -1,
        };
        format!(
            "{{\"id\":{},\"name\":{},\"popText\":{},\"percentText\":{},\"thousands\":{:.4},\"familyId\":{}}}",
            self.id,
            json_escape(&self.name),
            json_escape(&format_thousands_compact(self.thousands)),
            json_escape(&self.percent_text),
            self.thousands,
            fid
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
    pub has_atmosphere: bool,
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
    pub galactic_pole: [f32; 3],
    pub galactic_center_dir: [f32; 3],
    pub galaxy_band_mode: u8,
    pub galaxy_summary: String,
    pub field_stars: Vec<[f32; 4]>,
    pub moons: Vec<TileSkyMoon>,
    pub planets: Vec<TileSkyPlanet>,
    pub constellations: Vec<TileSkyConstellation>,
}

impl TileSkyView {
    pub fn to_json(&self) -> String {
        let moons_json: Vec<String> = self.moons.iter().map(|m| m.to_json()).collect();
        let planets_json: Vec<String> = self.planets.iter().map(|p| p.to_json()).collect();
        let consts_json: Vec<String> = self.constellations.iter().map(|c| c.to_json()).collect();
        let fstars_json: Vec<String> = self
            .field_stars
            .iter()
            .map(|s| format!("[{:.1},{:.1},{:.2},{:.2}]", s[0], s[1], s[2], s[3]))
            .collect();
        format!(
            "{{\"latDeg\":{:.2},\"lonDeg\":{:.2},\"tiltDeg\":{:.2},\"daysPerYear\":{},\"dayHours\":{:.2},\"currentHistoryYear\":{},\"sunName\":{},\"sunRgb\":[{:.3},{:.3},{:.3}],\"skyZenithRgb\":[{:.3},{:.3},{:.3}],\"skyHorizonRgb\":[{:.3},{:.3},{:.3}],\"sunsetRgb\":[{:.3},{:.3},{:.3}],\"twilightRgb\":[{:.3},{:.3},{:.3}],\"homeOrbitAu\":{:.4},\"homePeriodDays\":{:.4},\"homeInitAngleRad\":{:.4},\"galacticPole\":[{:.4},{:.4},{:.4}],\"galacticCenterDir\":[{:.4},{:.4},{:.4}],\"galaxyBandMode\":{},\"galaxySummary\":{},\"fieldStars\":[{}],\"moons\":[{}],\"planets\":[{}],\"constellations\":[{}],\"hasAtmosphere\":{}}}",
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
            self.galactic_pole[0],
            self.galactic_pole[1],
            self.galactic_pole[2],
            self.galactic_center_dir[0],
            self.galactic_center_dir[1],
            self.galactic_center_dir[2],
            self.galaxy_band_mode,
            json_escape(&self.galaxy_summary),
            fstars_json.join(","),
            moons_json.join(","),
            planets_json.join(","),
            consts_json.join(","),
            if self.has_atmosphere { "true" } else { "false" }
        )
    }
}

/// Immediate adjacent neighbor step for the local rectangular vantage/horizon view (`display_idx`).
#[derive(Clone, Debug, PartialEq)]
pub struct TileVantageNeighbor {
    pub display_idx: usize,
    pub az_deg: f32,
    pub compass: String,
    pub label: String,
    pub elev_km: f32,
    pub mountainosity_norm: f32,
    pub is_land: bool,
    pub is_river: bool,
}

impl TileVantageNeighbor {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"displayIdx\":{},\"azDeg\":{:.2},\"compass\":{},\"label\":{},\"elevKm\":{:.3},\"mountainosityNorm\":{:.3},\"isLand\":{},\"isRiver\":{}}}",
            self.display_idx,
            self.az_deg,
            json_escape(&self.compass),
            json_escape(&self.label),
            self.elev_km,
            self.mountainosity_norm,
            if self.is_land { "true" } else { "false" },
            if self.is_river { "true" } else { "false" }
        )
    }
}

/// Visible surrounding tile sample in the local horizon neighborhood around `display_idx`.
#[derive(Clone, Debug, PartialEq)]
pub struct TileVantageSample {
    pub display_idx: usize,
    pub is_adjacent: bool,
    pub az_deg: f32,
    pub half_width_deg: f32,
    pub dist_km: f32,
    pub elev_km: f32,
    pub mountainosity_norm: f32,
    pub is_land: bool,
    pub is_river: bool,
    pub koeppen: u8,
    pub veg_frac: f32,
    pub felsity: f32,
    pub rock_name: String,
    pub rock_kind: String,
}

impl TileVantageSample {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"displayIdx\":{},\"isAdjacent\":{},\"azDeg\":{:.2},\"halfWidthDeg\":{:.2},\"distKm\":{:.2},\"elevKm\":{:.3},\"mountainosityNorm\":{:.3},\"isLand\":{},\"isRiver\":{},\"koeppen\":{},\"vegFrac\":{:.3},\"felsity\":{:.3},\"rockName\":{},\"rockKind\":{}}}",
            self.display_idx,
            if self.is_adjacent { "true" } else { "false" },
            self.az_deg,
            self.half_width_deg,
            self.dist_km,
            self.elev_km,
            self.mountainosity_norm,
            if self.is_land { "true" } else { "false" },
            if self.is_river { "true" } else { "false" },
            self.koeppen,
            self.veg_frac,
            self.felsity,
            json_escape(&self.rock_name),
            json_escape(&self.rock_kind)
        )
    }
}

/// Complete local rectangular vantage / horizon dataset for a clicked tile (`display_idx`).
#[derive(Clone, Debug, PartialEq)]
pub struct TileVantageView {
    pub obs_is_coarse: bool,
    pub planet_radius_km: f32,
    pub ring1_dist_km: f32,
    pub obs_elev_km: f32,
    pub obs_mountainosity_norm: f32,
    pub obs_is_land: bool,
    pub obs_is_river: bool,
    pub obs_koeppen: u8,
    pub obs_veg_frac: f32,
    pub obs_felsity: f32,
    pub obs_rock_name: String,
    pub obs_rock_kind: String,
    pub plant_foliage_rgb: [f32; 3],
    pub plant_highlight_rgb: [f32; 3],
    pub neighbors: Vec<TileVantageNeighbor>,
    pub samples: Vec<TileVantageSample>,
}

impl TileVantageView {
    pub fn to_json(&self) -> String {
        let nbs_json: Vec<String> = self.neighbors.iter().map(|n| n.to_json()).collect();
        let smp_json: Vec<String> = self.samples.iter().map(|s| s.to_json()).collect();
        format!(
            "{{\"obsIsCoarse\":{},\"planetRadiusKm\":{:.2},\"ring1DistKm\":{:.2},\"obsElevKm\":{:.3},\"obsMountainosityNorm\":{:.3},\"obsIsLand\":{},\"obsIsRiver\":{},\"obsKoeppen\":{},\"obsVegFrac\":{:.3},\"obsFelsity\":{:.3},\"obsRockName\":{},\"obsRockKind\":{},\"plantFoliageRgb\":[{:.3},{:.3},{:.3}],\"plantHighlightRgb\":[{:.3},{:.3},{:.3}],\"neighbors\":[{}],\"samples\":[{}]}}",
            if self.obs_is_coarse { "true" } else { "false" },
            self.planet_radius_km,
            self.ring1_dist_km,
            self.obs_elev_km,
            self.obs_mountainosity_norm,
            if self.obs_is_land { "true" } else { "false" },
            if self.obs_is_river { "true" } else { "false" },
            self.obs_koeppen,
            self.obs_veg_frac,
            self.obs_felsity,
            json_escape(&self.obs_rock_name),
            json_escape(&self.obs_rock_kind),
            self.plant_foliage_rgb[0],
            self.plant_foliage_rgb[1],
            self.plant_foliage_rgb[2],
            self.plant_highlight_rgb[0],
            self.plant_highlight_rgb[1],
            self.plant_highlight_rgb[2],
            nbs_json.join(","),
            smp_json.join(",")
        )
    }
}

/// Optional live hourly environment payload (`TidesScreen` & `WeatherScreen`) driving synchronized
/// clock locking, coastal tide/surge waterlines, cloud decks, precipitation particles, snowpack, and waves.
#[derive(Clone, Debug, PartialEq)]
pub struct TileLiveEnvironment {
    pub sim_locked: bool,
    pub sim_year: u32,
    pub sim_day: u32,
    pub sim_hour_frac: f32,
    pub sim_clock_label: String,
    pub tide_offset_m: f32,
    pub tidal_range_m: f32,
    pub cloud_low: f32,
    pub cloud_high: f32,
    pub rain_mm_hr: f32,
    pub snow_mm_hr: f32,
    pub snow_depth_m: f32,
    pub wind_speed_mps: f32,
    pub wind_az_deg: f32,
    pub storm_darkening: f32,
}

impl TileLiveEnvironment {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"simLocked\":{},\"simYear\":{},\"simDay\":{},\"simHourFrac\":{:.4},\"simClockLabel\":{},\"tideOffsetM\":{:.3},\"tidalRangeM\":{:.3},\"cloudLow\":{:.3},\"cloudHigh\":{:.3},\"rainMmHr\":{:.3},\"snowMmHr\":{:.3},\"snowDepthM\":{:.3},\"windSpeedMps\":{:.2},\"windAzDeg\":{:.1},\"stormDarkening\":{:.3}}}",
            if self.sim_locked { "true" } else { "false" },
            self.sim_year,
            self.sim_day,
            self.sim_hour_frac,
            json_escape(&self.sim_clock_label),
            self.tide_offset_m,
            self.tidal_range_m,
            self.cloud_low,
            self.cloud_high,
            self.rain_mm_hr,
            self.snow_mm_hr,
            self.snow_depth_m,
            self.wind_speed_mps,
            self.wind_az_deg,
            self.storm_darkening
        )
    }
}

/// Level-2 Tile inspection view (individual tile's nationalities, languages, native flora & fauna,
/// local 2D sky & 100° vantage views, and optional live tidal/weather environment).
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
    pub vantage: TileVantageView,
    pub stratigraphy_json: String,
    pub live_env: Option<TileLiveEnvironment>,
}

impl TileInspectionView {
    pub fn to_json(&self) -> String {
        let nats: Vec<String> = self.nationalities.iter().map(|n| n.to_json()).collect();
        let langs: Vec<String> = self.languages.iter().map(|l| l.to_json()).collect();
        let live_suffix = if let Some(ref env) = self.live_env {
            format!(",\"liveEnv\":{}", env.to_json())
        } else {
            String::new()
        };
        format!(
            "{{\"hasTile\":true,\"displayIdx\":{},\"stateId\":{},\"title\":{},\"locationSummary\":{},\"populationSummary\":{},\"technology\":{},\"nationalities\":[{}],\"languages\":[{}],\"flora\":{},\"fauna\":{},\"sky\":{},\"vantage\":{},\"stratigraphy\":{}{}}}",
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
            self.sky.to_json(),
            self.vantage.to_json(),
            self.stratigraphy_json,
            live_suffix
        )
    }
}

/// Convert a `Constellation` from the 3D galactic star catalog into a `TileSkyConstellation`,
/// using its true `resolved_stars` and `local_edges` when present.
pub fn constellation_to_sky_asterism(c: &Constellation, name: String) -> TileSkyConstellation {
    if !c.resolved_stars.is_empty() {
        return TileSkyConstellation {
            index: c.index,
            name,
            ra_deg: c.right_ascension_deg,
            dec_deg: c.declination_deg,
            stars: c.resolved_stars.clone(),
            edges: c.local_edges.clone(),
        };
    }

    let n_stars = (c.star_count as usize).clamp(3, 7);
    let seed_u64 = (c.index as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (c.right_ascension_deg.to_bits() as u64)
        ^ ((c.declination_deg.to_bits() as u64) << 16);
    let mut crng = Pcg64Mcg::from_seed_u64(seed_u64);
    let c_dir = normal(c.center_dir);
    let raw_east = [-c_dir[1], c_dir[0], 0.0];
    let east_norm = (raw_east[0] * raw_east[0] + raw_east[1] * raw_east[1]).sqrt();
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
        let s_ra =
            ((s_dir[1].atan2(s_dir[0]) * 180.0 / std::f64::consts::PI).rem_euclid(360.0)) as f32;
        let s_mag = (c.brightest_mag + 0.25 * (k as f32)).clamp(0.4, 3.4);
        stars.push([s_ra, s_dec, s_mag]);
    }
    let mut edges = Vec::with_capacity(n_stars);
    for k in 0..(n_stars - 1) {
        edges.push([k, k + 1]);
    }
    TileSkyConstellation {
        index: c.index,
        name,
        ra_deg: c.right_ascension_deg,
        dec_deg: c.declination_deg,
        stars,
        edges,
    }
}

/// Extract `(galactic_pole, galactic_center_dir, galaxy_band_mode, galaxy_summary, field_stars)`
/// from a planet's `Constellation` catalog (falling back to Sol's Milky Way plane if absent).
pub fn extract_galactic_backdrop_fields(
    constellations_src: &[Constellation],
) -> ([f32; 3], [f32; 3], u8, String, Vec<[f32; 4]>) {
    if let Some(bd) = constellations_src
        .first()
        .and_then(|c| c.backdrop.as_deref())
    {
        (
            bd.galactic_pole,
            bd.galactic_center_dir,
            bd.galaxy_band_mode,
            bd.summary_label(),
            bd.field_stars.clone(),
        )
    } else {
        (
            [-0.86767, -0.19808, 0.45598],
            [-0.05488, -0.87344, -0.48384],
            0,
            "Spiral Galaxy (Sb/Sc) · Spiral Arm (R=8.1 kpc, z=+0.02 kpc)".to_string(),
            Vec::new(),
        )
    }
}

/// Build the celestial `TileSkyView` for a unit-sphere normal `center` given planetary & stellar parameters.
pub fn build_celestial_sky_view(
    center: [f64; 3],
    day_hours: f64,
    tilt_deg: f64,
    has_atmosphere: bool,
    current_year: u32,
    moon_count: u32,
    home_star: &HomeStar,
    optics: &PlanetaryOptics,
    solar_system: &[SolarSystemPlanet],
    constellations_src: &[Constellation],
) -> TileSkyView {
    let lat_deg = (center[2].clamp(-1.0, 1.0).asin() * 180.0 / std::f64::consts::PI) as f32;
    let lon_deg = (center[1].atan2(center[0]) * 180.0 / std::f64::consts::PI) as f32;
    let tilt_f32 = (tilt_deg as f32).clamp(0.0, 89.0);
    let day_h = (day_hours as f32).clamp(2.0, 240.0);
    let spin = (24.0 / day_h).max(0.1);

    let home_planet = solar_system.iter().find(|p| p.is_home_world);
    let home_orbit_au = home_planet
        .map(|p| p.orbit_au)
        .unwrap_or(home_star.habitable_au)
        .max(0.02);
    let earth_days_period = home_planet
        .map(|p| p.orbital_period_days)
        .unwrap_or(home_star.orbital_period_days)
        .max(5.0);
    let home_period_days = (earth_days_period * spin).clamp(8.0, 5000.0);
    let days_per_year = home_period_days.round().clamp(8.0, 5000.0) as u32;
    let home_init_angle_rad = home_planet.map(|p| p.orbit_angle_rad).unwrap_or(0.0);

    let base_lunar_days = (27.32166 * spin).max(2.0);
    let mut moons = Vec::with_capacity(moon_count as usize);
    for m in 0..moon_count {
        let period_days = base_lunar_days * ((1u32 << m.min(6)) as f32);
        let inclination_deg = 5.14 + (m as f32) * 1.25;
        let init_angle_rad = ((m as f32) * 1.854) % std::f32::consts::TAU;
        let node_asc_rad = ((m as f32) * 2.399) % std::f32::consts::TAU;
        moons.push(TileSkyMoon {
            index: m,
            name: format!("Moon {}", m + 1),
            period_days,
            inclination_deg,
            init_angle_rad,
            node_asc_rad,
        });
    }

    let mut planets = Vec::new();
    for p in solar_system {
        if p.is_home_world {
            continue;
        }
        let period_days = (p.orbital_period_days * spin).max(2.0);
        let inclination_deg = 1.1 + ((p.index % 5) as f32) * 0.65;
        planets.push(TileSkyPlanet {
            index: p.index,
            name: format!("Planet {}", p.index + 1),
            orbit_au: p.orbit_au.max(0.02),
            period_days,
            init_angle_rad: p.orbit_angle_rad,
            inclination_deg,
            rgb: p.rgb,
            has_rings: p.has_rings,
        });
    }

    let constellations: Vec<TileSkyConstellation> = constellations_src
        .iter()
        .map(|c| constellation_to_sky_asterism(c, String::new()))
        .collect();
    let (galactic_pole, galactic_center_dir, galaxy_band_mode, galaxy_summary, field_stars) =
        extract_galactic_backdrop_fields(constellations_src);

    TileSkyView {
        has_atmosphere,
        lat_deg,
        lon_deg,
        tilt_deg: tilt_f32,
        days_per_year,
        day_hours: day_h,
        current_history_year: current_year,
        sun_name: "Sun".to_string(),
        sun_rgb: home_star.rgb,
        sky_zenith_rgb: optics.sky_zenith_rgb,
        sky_horizon_rgb: optics.sky_horizon_rgb,
        sunset_rgb: optics.sunset_rgb,
        twilight_rgb: optics.twilight_rgb,
        home_orbit_au,
        home_period_days,
        home_init_angle_rad,
        galactic_pole,
        galactic_center_dir,
        galaxy_band_mode,
        galaxy_summary,
        field_stars,
        moons,
        planets,
        constellations,
    }
}

const COMPASS_16: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];

fn local_east_north(center: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let raw_east = [-center[1], center[0], 0.0];
    let east_len = (raw_east[0] * raw_east[0] + raw_east[1] * raw_east[1]).sqrt();
    let east = if east_len > 1e-6 {
        [raw_east[0] / east_len, raw_east[1] / east_len, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let north = normal([
        center[1] * east[2] - center[2] * east[1],
        center[2] * east[0] - center[0] * east[2],
        center[0] * east[1] - center[1] * east[0],
    ]);
    (east, north)
}

/// Parameters for building a structured sky dome & local-only coarse vantage view
/// on an un-subdivided coarse `SphericalGrid` (e.g. Tectonics & Prehistory stages).
pub struct CoarseTileSkyParams<'a> {
    pub grid: &'a SphericalGrid,
    pub adjacency: &'a [Vec<usize>],
    pub tiles: &'a [TileGeology],
    pub koeppen_ids: &'a [u8],
    pub tile_life: &'a [f32],
    pub river_tiles: Option<&'a [bool]>,
    pub radius_km: f64,
    pub day_hours: f64,
    pub tilt_deg: f64,
    pub has_atmosphere: bool,
    pub has_life: bool,
    pub current_year: u32,
    pub moon_count: u32,
    pub home_star: &'a HomeStar,
    pub optics: &'a PlanetaryOptics,
    pub solar_system: &'a [SolarSystemPlanet],
    pub constellations: &'a [Constellation],
    pub title: String,
    pub location_summary: String,
    pub population_summary: String,
    pub technology: String,
    pub stratigraphy_json: String,
}

/// Build a complete `TileInspectionView` (`sky` + local-only `vantage` with `obs_is_coarse: true`
/// and `samples: []`) for any coarse tile on an un-subdivided `SphericalGrid`.
pub fn build_coarse_tile_inspection_view(
    tile_idx: usize,
    params: CoarseTileSkyParams<'_>,
) -> Option<TileInspectionView> {
    let &(center_raw, _) = params.grid.faces.get(tile_idx)?;
    let tile = params.tiles.get(tile_idx)?;

    let center = normal(center_raw);
    let sky = build_celestial_sky_view(
        center,
        params.day_hours,
        params.tilt_deg,
        params.has_atmosphere,
        params.current_year,
        params.moon_count,
        params.home_star,
        params.optics,
        params.solar_system,
        params.constellations,
    );

    let planet_radius_km = (params.radius_km as f32).max(500.0);
    let (east, north) = local_east_north(center);

    let mut neighbors = Vec::new();
    let mut ring1_dist_sum = 0.0f32;
    if let Some(nbs) = params.adjacency.get(tile_idx) {
        for &nb_idx in nbs {
            if nb_idx == tile_idx {
                continue;
            }
            let Some(&(nb_center_raw, _)) = params.grid.faces.get(nb_idx) else {
                continue;
            };
            let q = normal(nb_center_raw);
            let dot_pq =
                (center[0] * q[0] + center[1] * q[1] + center[2] * q[2]).clamp(-1.0, 1.0);
            let ang_rad = dot_pq.acos();
            let tx = q[0] - dot_pq * center[0];
            let ty = q[1] - dot_pq * center[1];
            let tz = q[2] - dot_pq * center[2];
            let e_comp = tx * east[0] + ty * east[1] + tz * east[2];
            let n_comp = tx * north[0] + ty * north[1] + tz * north[2];
            let az_deg = ((e_comp.atan2(n_comp).to_degrees()).rem_euclid(360.0)) as f32;
            let dist_km = (ang_rad as f32) * planet_radius_km;
            ring1_dist_sum += dist_km;

            let nb_elev = params
                .tiles
                .get(nb_idx)
                .map(|t| t.elevation())
                .unwrap_or(0.0);
            let nb_mount = params
                .tiles
                .get(nb_idx)
                .map(|t| t.mountainosity_norm())
                .unwrap_or(0.0);
            let nb_is_land = !params.has_atmosphere || nb_elev > 0.0;
            let nb_is_river = params.has_atmosphere
                && params
                    .river_tiles
                    .and_then(|r| r.get(nb_idx).copied())
                    .unwrap_or(false);
            let c16_idx = (((az_deg + 11.25) / 22.5).floor() as usize) % 16;
            let compass = COMPASS_16[c16_idx].to_string();
            neighbors.push(TileVantageNeighbor {
                display_idx: nb_idx,
                az_deg,
                compass,
                label: format!("Tile #{nb_idx} (~{dist_km:.0} km away)"),
                elev_km: nb_elev,
                mountainosity_norm: nb_mount,
                is_land: nb_is_land,
                is_river: nb_is_river,
            });
        }
    }
    neighbors.sort_by(|a, b| {
        a.az_deg
            .partial_cmp(&b.az_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let ring1_dist_km = if !neighbors.is_empty() {
        (ring1_dist_sum / (neighbors.len() as f32)).max(20.0)
    } else {
        300.0
    };

    let obs_elev_km = tile.elevation();
    let obs_mountainosity_norm = tile.mountainosity_norm();
    let obs_is_land = !params.has_atmosphere || obs_elev_km > 0.0;
    let obs_koeppen = if params.has_atmosphere {
        params.koeppen_ids.get(tile_idx).copied().unwrap_or(0)
    } else {
        0
    };
    let obs_is_river = params.has_atmosphere
        && params
            .river_tiles
            .and_then(|r| r.get(tile_idx).copied())
            .unwrap_or(false);
    let obs_veg_frac = if !params.has_atmosphere
        || !params.has_life
        || obs_elev_km <= 0.0
        || obs_koeppen == 12
        || obs_koeppen == 13
    {
        0.0
    } else {
        let base_life = params
            .tile_life
            .get(tile_idx)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let alpine_atten = if obs_elev_km > 3.2 {
            ((5.4 - obs_elev_km) / 2.2).clamp(0.05, 1.0)
        } else {
            1.0
        };
        (base_life * alpine_atten).clamp(0.0, 1.0)
    };
    let obs_felsity = tile.top_felsity().clamp(0.0, 1.0);
    let (obs_rock_name, obs_rock_kind) = if let Some(top_layer) = tile.layers.last() {
        (
            top_layer.rock.name.to_string(),
            top_layer.rock.kind.code().to_string(),
        )
    } else {
        ("basalt".to_string(), "I".to_string())
    };

    let vantage = TileVantageView {
        obs_is_coarse: true,
        planet_radius_km,
        ring1_dist_km,
        obs_elev_km,
        obs_mountainosity_norm,
        obs_is_land,
        obs_is_river,
        obs_koeppen,
        obs_veg_frac,
        obs_felsity,
        obs_rock_name,
        obs_rock_kind,
        plant_foliage_rgb: params.optics.plant_foliage_rgb,
        plant_highlight_rgb: params.optics.plant_highlight_rgb,
        neighbors,
        samples: Vec::new(),
    };

    Some(TileInspectionView {
        display_idx: tile_idx,
        fine_idx: None,
        state_id: None,
        state_name: None,
        title: params.title,
        location_summary: params.location_summary,
        population_summary: params.population_summary,
        technology: params.technology,
        nationalities: Vec::new(),
        languages: Vec::new(),
        flora: Vec::new(),
        fauna: Vec::new(),
        sky,
        vantage,
        stratigraphy_json: params.stratigraphy_json,
        live_env: None,
    })
}

/// Parameters for building a full multi-ring fine `TileInspectionView` (`obs_is_coarse: false`)
/// on a uniform `coarse + 2` `SphericalGrid` (used by `TidesScreen` and `WeatherScreen`).
pub struct UniformFineTileSkyParams<'a> {
    pub fine_grid: &'a SphericalGrid,
    pub fine_adjacency: &'a [Vec<usize>],
    pub fine_elev_km: &'a [f32],
    pub fine_mountainosity_norm: &'a [f32],
    pub fine_is_land: &'a [bool],
    pub fine_is_river: &'a [bool],
    pub fine_primary_coarse: &'a [usize],
    pub coarse_tiles: &'a [TileGeology],
    pub coarse_koeppen_ids: &'a [u8],
    pub coarse_tile_life: &'a [f32],
    pub radius_km: f64,
    pub day_hours: f64,
    pub tilt_deg: f64,
    pub has_atmosphere: bool,
    pub has_life: bool,
    pub current_year: u32,
    pub moon_count: u32,
    pub home_star: &'a HomeStar,
    pub optics: &'a PlanetaryOptics,
    pub solar_system: &'a [SolarSystemPlanet],
    pub constellations: &'a [Constellation],
    pub title: String,
    pub location_summary: String,
    pub population_summary: String,
    pub technology: String,
    pub stratigraphy_json: String,
    pub live_env: Option<TileLiveEnvironment>,
}

/// Build a complete `TileInspectionView` (`sky` + 1..4-ring fine `vantage` + optional `live_env`)
/// for any fine cell `fine_idx` on a uniform `coarse + 2` `SphericalGrid`.
pub fn build_uniform_fine_tile_inspection_view(
    fine_idx: usize,
    params: UniformFineTileSkyParams<'_>,
) -> Option<TileInspectionView> {
    let &(center_raw, _) = params.fine_grid.faces.get(fine_idx)?;
    let center = normal(center_raw);

    let sky = build_celestial_sky_view(
        center,
        params.day_hours,
        params.tilt_deg,
        params.has_atmosphere,
        params.current_year,
        params.moon_count,
        params.home_star,
        params.optics,
        params.solar_system,
        params.constellations,
    );

    let planet_radius_km = (params.radius_km as f32).max(500.0);
    let (east, north) = local_east_north(center);

    let sample_props = |idx: usize| -> (f32, f32, bool, bool, u8, f32, f32, String, String) {
        let elev_km = params.fine_elev_km.get(idx).copied().unwrap_or(0.0);
        let mount_norm = params
            .fine_mountainosity_norm
            .get(idx)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let is_land = !params.has_atmosphere
            || params
                .fine_is_land
                .get(idx)
                .copied()
                .unwrap_or(elev_km > 0.0);
        let is_river = params.has_atmosphere
            && params.fine_is_river.get(idx).copied().unwrap_or(false);
        let c_idx = params.fine_primary_coarse.get(idx).copied().unwrap_or(0);
        let koeppen = if params.has_atmosphere {
            params.coarse_koeppen_ids.get(c_idx).copied().unwrap_or(0)
        } else {
            0
        };
        let veg_frac = if !params.has_atmosphere
            || !params.has_life
            || !is_land
            || koeppen == 12
            || koeppen == 13
        {
            0.0
        } else {
            let base_life = params
                .coarse_tile_life
                .get(c_idx)
                .copied()
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);
            let alpine_atten = if elev_km > 3.2 {
                ((5.4 - elev_km) / 2.2).clamp(0.05, 1.0)
            } else {
                1.0
            };
            (base_life * alpine_atten).clamp(0.0, 1.0)
        };
        let (felsity, rock_name, rock_kind) = if let Some(ct) = params.coarse_tiles.get(c_idx) {
            let f = ct.top_felsity().clamp(0.0, 1.0);
            if let Some(top_layer) = ct.layers.last() {
                (
                    f,
                    top_layer.rock.name.to_string(),
                    top_layer.rock.kind.code().to_string(),
                )
            } else {
                (f, "basalt".to_string(), "I".to_string())
            }
        } else {
            (0.5, "basalt".to_string(), "I".to_string())
        };
        (
            elev_km, mount_norm, is_land, is_river, koeppen, veg_frac, felsity, rock_name,
            rock_kind,
        )
    };

    let mut neighbors = Vec::new();
    let mut ring1_dist_sum = 0.0f32;
    if let Some(nbs) = params.fine_adjacency.get(fine_idx) {
        for &nb_idx in nbs {
            if nb_idx == fine_idx {
                continue;
            }
            let Some(&(nb_center_raw, _)) = params.fine_grid.faces.get(nb_idx) else {
                continue;
            };
            let q = normal(nb_center_raw);
            let dot_pq =
                (center[0] * q[0] + center[1] * q[1] + center[2] * q[2]).clamp(-1.0, 1.0);
            let ang_rad = dot_pq.acos();
            let tx = q[0] - dot_pq * center[0];
            let ty = q[1] - dot_pq * center[1];
            let tz = q[2] - dot_pq * center[2];
            let e_comp = tx * east[0] + ty * east[1] + tz * east[2];
            let n_comp = tx * north[0] + ty * north[1] + tz * north[2];
            let az_deg = ((e_comp.atan2(n_comp).to_degrees()).rem_euclid(360.0)) as f32;
            let dist_km = (ang_rad as f32) * planet_radius_km;
            ring1_dist_sum += dist_km;

            let (nb_elev, nb_mount, nb_is_land, nb_is_river, _, _, _, _, _) = sample_props(nb_idx);
            let c16_idx = (((az_deg + 11.25) / 22.5).floor() as usize) % 16;
            let compass = COMPASS_16[c16_idx].to_string();
            let kind_str = if nb_is_land {
                format!("{:.2} km", nb_elev.max(0.0))
            } else {
                format!("Sea {:.2} km", nb_elev)
            };
            neighbors.push(TileVantageNeighbor {
                display_idx: nb_idx,
                az_deg,
                compass,
                label: format!("Tile #{nb_idx} ({kind_str}, ~{dist_km:.0} km)"),
                elev_km: nb_elev,
                mountainosity_norm: nb_mount,
                is_land: nb_is_land,
                is_river: nb_is_river,
            });
        }
    }
    neighbors.sort_by(|a, b| {
        a.az_deg
            .partial_cmp(&b.az_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let ring1_dist_km = if !neighbors.is_empty() {
        (ring1_dist_sum / (neighbors.len() as f32)).max(8.0)
    } else {
        88.0
    };

    // BFS rings 1..=4 for smooth horizon & distant mountain peaks
    let mut visited = std::collections::BTreeSet::new();
    visited.insert(fine_idx);
    let mut queue = VecDeque::new();
    if let Some(nbs) = params.fine_adjacency.get(fine_idx) {
        for &nb in nbs {
            if visited.insert(nb) {
                queue.push_back((nb, 1u8));
            }
        }
    }
    let mut samples = Vec::new();
    while let Some((s_idx, ring)) = queue.pop_front() {
        let Some(&(s_raw, _)) = params.fine_grid.faces.get(s_idx) else {
            continue;
        };
        let q = normal(s_raw);
        let dot_pq = (center[0] * q[0] + center[1] * q[1] + center[2] * q[2]).clamp(-1.0, 1.0);
        let ang_rad = dot_pq.acos();
        let dist_km = ((ang_rad as f32) * planet_radius_km).max(4.0);
        let tx = q[0] - dot_pq * center[0];
        let ty = q[1] - dot_pq * center[1];
        let tz = q[2] - dot_pq * center[2];
        let e_comp = tx * east[0] + ty * east[1] + tz * east[2];
        let n_comp = tx * north[0] + ty * north[1] + tz * north[2];
        let az_deg = ((e_comp.atan2(n_comp).to_degrees()).rem_euclid(360.0)) as f32;
        let half_width_deg = ((0.55 * ring1_dist_km / dist_km).atan().to_degrees()).clamp(5.0, 36.0);
        let (elev_km, mountainosity_norm, is_land, is_river, koeppen, veg_frac, felsity, rock_name, rock_kind) =
            sample_props(s_idx);

        samples.push(TileVantageSample {
            display_idx: s_idx,
            is_adjacent: ring == 1,
            az_deg,
            half_width_deg,
            dist_km,
            elev_km,
            mountainosity_norm,
            is_land,
            is_river,
            koeppen,
            veg_frac,
            felsity,
            rock_name,
            rock_kind,
        });

        if ring < 4 {
            if let Some(nbs) = params.fine_adjacency.get(s_idx) {
                for &nxt in nbs {
                    if visited.insert(nxt) {
                        queue.push_back((nxt, ring + 1));
                    }
                }
            }
        }
    }
    samples.sort_by(|a, b| {
        b.dist_km
            .partial_cmp(&a.dist_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let (
        obs_elev_km,
        obs_mountainosity_norm,
        obs_is_land,
        obs_is_river,
        obs_koeppen,
        obs_veg_frac,
        obs_felsity,
        obs_rock_name,
        obs_rock_kind,
    ) = sample_props(fine_idx);

    let vantage = TileVantageView {
        obs_is_coarse: false,
        planet_radius_km,
        ring1_dist_km,
        obs_elev_km,
        obs_mountainosity_norm,
        obs_is_land,
        obs_is_river,
        obs_koeppen,
        obs_veg_frac,
        obs_felsity,
        obs_rock_name,
        obs_rock_kind,
        plant_foliage_rgb: params.optics.plant_foliage_rgb,
        plant_highlight_rgb: params.optics.plant_highlight_rgb,
        neighbors,
        samples,
    };

    Some(TileInspectionView {
        display_idx: fine_idx,
        fine_idx: Some(fine_idx),
        state_id: None,
        state_name: None,
        title: params.title,
        location_summary: params.location_summary,
        population_summary: params.population_summary,
        technology: params.technology,
        nationalities: Vec::new(),
        languages: Vec::new(),
        flora: Vec::new(),
        fauna: Vec::new(),
        sky,
        vantage,
        stratigraphy_json: params.stratigraphy_json,
        live_env: params.live_env,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sun_sky::{
        generate_default_constellations, generate_solar_system_with_rng, AtmosphereType, HomeStar,
        PlanetaryOptics,
    };

    #[test]
    fn uniform_fine_tile_inspection_builds_multi_ring_vantage_and_live_env_json() {
        let fine_grid = SphericalGrid::with_size(4);
        let fine_adj = fine_grid.adjacency();
        let n = fine_grid.faces.len();
        let fine_elev: Vec<f32> = (0..n)
            .map(|i| if i % 3 == 0 { -1.2 } else { 0.45 + ((i % 7) as f32) * 0.15 })
            .collect();
        let fine_is_land: Vec<bool> = fine_elev.iter().map(|&e| e > 0.0).collect();
        let fine_mount: Vec<f32> = (0..n).map(|i| ((i % 5) as f32) * 0.2).collect();
        let primary_coarse = vec![0usize; n];
        let coarse_tiles = vec![TileGeology::birth_seafloor()];
        let coarse_koeppen = vec![7u8];
        let coarse_life = vec![0.65f32];
        let river_tiles = vec![false; n];

        let home_star = HomeStar::sol();
        let optics = PlanetaryOptics::compute(&home_star, AtmosphereType::EarthLikeN2O2);
        let mut rng = Pcg64Mcg::from_seed_u64(12345);
        let solar_system =
            generate_solar_system_with_rng(&mut rng, &home_star, [0.0, -0.398, 0.917], true);
        let constellations = generate_default_constellations(42);

        let view = build_uniform_fine_tile_inspection_view(
            10,
            UniformFineTileSkyParams {
                fine_grid: &fine_grid,
                fine_adjacency: &fine_adj,
                fine_elev_km: &fine_elev,
                fine_mountainosity_norm: &fine_mount,
                fine_is_land: &fine_is_land,
                fine_is_river: &river_tiles,
                fine_primary_coarse: &primary_coarse,
                coarse_tiles: &coarse_tiles,
                coarse_koeppen_ids: &coarse_koeppen,
                coarse_tile_life: &coarse_life,
                radius_km: 6371.0,
                day_hours: 24.0,
                tilt_deg: 23.44,
                has_atmosphere: true,
                has_life: true,
                current_year: 1,
                moon_count: 1,
                home_star: &home_star,
                optics: &optics,
                solar_system: &solar_system,
                constellations: &constellations,
                title: "Fine Tile #10".to_string(),
                location_summary: "Temperate Coast".to_string(),
                population_summary: "Uninhabited".to_string(),
                technology: "Weather".to_string(),
                stratigraphy_json: "{\"layers\":[]}".to_string(),
                live_env: Some(TileLiveEnvironment {
                    sim_locked: true,
                    sim_year: 1,
                    sim_day: 80,
                    sim_hour_frac: 0.5,
                    sim_clock_label: "Day 80 · 12:00 UTC".to_string(),
                    tide_offset_m: 1.35,
                    tidal_range_m: 2.8,
                    cloud_low: 0.4,
                    cloud_high: 0.2,
                    rain_mm_hr: 1.5,
                    snow_mm_hr: 0.0,
                    snow_depth_m: 0.0,
                    wind_speed_mps: 8.2,
                    wind_az_deg: 245.0,
                    storm_darkening: 0.25,
                }),
            },
        )
        .expect("should build uniform fine tile inspection view");

        assert!(!view.vantage.obs_is_coarse);
        assert!(!view.vantage.neighbors.is_empty());
        assert!(view.vantage.samples.len() > view.vantage.neighbors.len());
        let json = view.to_json();
        assert!(json.contains("\"liveEnv\":{\"simLocked\":true"));
        assert!(json.contains("\"tideOffsetM\":1.350"));
    }
}
