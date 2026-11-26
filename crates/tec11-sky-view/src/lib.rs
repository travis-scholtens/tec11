pub mod sky_vantage;
pub mod sun_viewer;

pub use sky_vantage::*;
pub use sun_viewer::SunSandbox;

use tec11_cosmos::{
    generate_default_constellations, generate_solar_system_with_rng, substitute_simulated_planet,
    AtmosphereType, Constellation, HomeStar, PlanetaryOptics, ReconstructedSkyEnvironment,
    SolarSystemPlanet,
};
use tec11_rng::Pcg64Mcg;

/// Reconstruct the sky environment from an optional saved `PlanetaryEnvironment` proto,
/// falling back to a deterministic Sol/Earth-like system when absent.
#[allow(clippy::too_many_arguments)]
pub fn resolve_sky_environment(
    saved_env: Option<&tec11_proto::PlanetaryEnvironment>,
    grid_size: u32,
    radius_km: f32,
    spin: f32,
    tilt_deg: f32,
    land_percent: u32,
    default_moon_count: u32,
    fallback_seed: u64,
) -> (
    HomeStar,
    PlanetaryOptics,
    Vec<SolarSystemPlanet>,
    Vec<Constellation>,
    u32,
) {
    if let Some(env_proto) = saved_env {
        let dummy_snap = tec11_proto::PlanetSnapshot {
            rng_state_16b: Vec::new(),
            grid_size,
            radius_km,
            spin,
            tilt_deg,
            stage: None,
            environment: Some(env_proto.clone()),
        };
        let sky = ReconstructedSkyEnvironment::from_snapshot(&dummy_snap, radius_km, land_percent);
        (
            sky.home_star,
            sky.optics,
            sky.solar_system,
            sky.constellations,
            sky.moon_count,
        )
    } else {
        let home_star = HomeStar::sol();
        let optics = PlanetaryOptics::compute(&home_star, AtmosphereType::EarthLikeN2O2);
        let mut sys_rng = Pcg64Mcg::from_seed_u64(fallback_seed);
        let ecliptic_pole = [0.0, -0.398, 0.917];
        let mut solar_system =
            generate_solar_system_with_rng(&mut sys_rng, &home_star, ecliptic_pole, true);
        substitute_simulated_planet(
            &mut solar_system,
            &home_star,
            radius_km,
            land_percent,
            default_moon_count,
            ecliptic_pole,
        );
        let constellations = generate_default_constellations(42);
        (
            home_star,
            optics,
            solar_system,
            constellations,
            default_moon_count,
        )
    }
}

/// Blend sea-ice reflectance into a base surface RGB color.
pub fn blend_sea_ice_rgb(base: [f32; 3], sea_ice: f32) -> [f32; 3] {
    let ice = sea_ice.clamp(0.0, 1.0);
    if ice <= 0.02 {
        return base;
    }
    let t = (ice * 0.90).clamp(0.0, 0.92);
    let ice_rgb = [0.84f32, 0.91, 0.97];
    [
        base[0] * (1.0 - t) + ice_rgb[0] * t,
        base[1] * (1.0 - t) + ice_rgb[1] * t,
        base[2] * (1.0 - t) + ice_rgb[2] * t,
    ]
}

/// Shaded natural terrain RGB color given elevation, mountainosity, felsity, Köppen code, life,
/// atmosphere flag, and planetary sky/foliage optics.
pub fn natural_color_shaded(
    h: f32,
    mountainosity: f32,
    felsity: f32,
    koeppen: u8,
    life: f32,
    has_atm: bool,
    optics: &PlanetaryOptics,
) -> [f32; 3] {
    if h <= 0.0 {
        return natural_color_with_optics(h, felsity, koeppen, life, has_atm, optics);
    }
    let u_elev = ((h - 0.45).max(0.0) / 4.8).clamp(0.0, 1.0);
    let u_mount = (mountainosity / 12.0).clamp(0.0, 1.0);
    if has_atm && (koeppen == 12 || koeppen == 13) {
        let shade = (1.0 - 0.30 * u_elev - 0.08 * u_mount).clamp(0.58, 1.0);
        [0.96 * shade, 0.98 * shade, 1.0 * (shade + 0.02).min(1.0)]
    } else {
        let rock_r = (0.5 * 128.0) / 255.0;
        let rock_g = (0.5 * (64.0 + 64.0 * felsity)) / 255.0;
        let rock_b = (0.5 * (128.0 * felsity)) / 255.0;
        let alpine_taper = (1.0 - ((h - 1.8).max(0.0) / 2.6)).clamp(0.0, 1.0);
        let veg = life.clamp(0.0, 1.0) * alpine_taper;
        let pf = optics.plant_foliage_rgb;
        let shade = (1.0 - 0.52 * u_elev.powf(0.85) - 0.12 * u_mount).clamp(0.32, 1.0);
        [
            ((rock_r * (1.0 - 0.55 * veg) + pf[0] * 0.85 * veg) * shade).clamp(0.0, 1.0),
            ((rock_g * (1.0 - 0.55 * veg) + pf[1] * 0.85 * veg) * shade).clamp(0.0, 1.0),
            ((rock_b * (1.0 - 0.55 * veg) + pf[2] * 0.85 * veg) * shade).clamp(0.0, 1.0),
        ]
    }
}

pub fn natural_color_with_optics(
    h: f32,
    felsity: f32,
    koeppen: u8,
    life: f32,
    has_atm: bool,
    optics: &PlanetaryOptics,
) -> [f32; 3] {
    if h <= 0.0 {
        if has_atm {
            let d = (-h / 6.0).clamp(0.0, 1.0);
            let sky = optics.sky_zenith_rgb;
            [
                ((0.02 + 0.36 * sky[0]) * (1.0 - 0.75 * d)).clamp(0.0, 1.0),
                ((0.05 + 0.44 * sky[1]) * (1.0 - 0.70 * d)).clamp(0.0, 1.0),
                ((0.15 + 0.58 * sky[2]) * (1.0 - 0.50 * d)).clamp(0.0, 1.0),
            ]
        } else {
            let s = ((7.5 + h) / 7.5).clamp(0.12, 1.0);
            [0.24 * s, 0.27 * s, 0.34 * s]
        }
    } else if has_atm && (koeppen == 12 || koeppen == 13) {
        [1.0, 1.0, 1.0]
    } else {
        let rock_r = (0.5 * 128.0) / 255.0;
        let rock_g = (0.5 * (64.0 + 64.0 * felsity)) / 255.0;
        let rock_b = (0.5 * (128.0 * felsity)) / 255.0;
        let veg = life.clamp(0.0, 1.0);
        let pf = optics.plant_foliage_rgb;
        [
            (rock_r * (1.0 - 0.55 * veg) + pf[0] * 0.85 * veg).clamp(0.0, 1.0),
            (rock_g * (1.0 - 0.55 * veg) + pf[1] * 0.85 * veg).clamp(0.0, 1.0),
            (rock_b * (1.0 - 0.55 * veg) + pf[2] * 0.85 * veg).clamp(0.0, 1.0),
        ]
    }
}
