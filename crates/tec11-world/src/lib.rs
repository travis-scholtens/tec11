use tec11_math_grid::{Pcg64Mcg, SphericalGrid};
use tec11_proto::PlanetSnapshotExt;

use tec11_history::HistorySimulation;
use tec11_prehistory::population::tile_population_thousands_with_water_tech;
use tec11_prehistory::prehistory::PrehistorySimulation;
use tec11_prehistory::racination::Heritage;
use tec11_cosmos::{
    build_star_map_buffers_from_sky, format_solar_system_summary,
    generate_solar_system_with_rng, sample_atmosphere_with_rng, sample_galactic_home_with_rng,
    sample_home_star_with_rng, substitute_simulated_planet,
    synthesize_galactic_sky_constellations, synthesize_galactic_sky_stars_and_constellations,
    synthesize_galaxy_cloud_normalized, AtmosphereType, Constellation, GalacticVantage,
    GalaxyType, HomeStar, PlanetaryOptics, SolarSystemPlanet,
};
use tec11_tectonics::tectonics::TectonicsSimulation;
use tec11_geology::tile::TileGeology;

pub use tec11_koeppen::{koeppen_climate_name, koeppen_rgb};

/// Format integer with comma thousands separators (`worldsimulation.py:56`).
pub fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

/// Triangular-weighted random integer in `[min_val..=max_val]` stepping by `step` and favoring `mode_val`
/// (`randomspinvalue` in `worldpresenter.py:29-41`).
pub fn random_spin_value(
    min_val: i32,
    max_val: i32,
    step: i32,
    mode_val: i32,
    rng: &mut Pcg64Mcg,
) -> i32 {
    let step_usize = step.max(1) as usize;
    let vals: Vec<i32> = (min_val..=max_val).step_by(step_usize).collect();
    if vals.is_empty() {
        return mode_val;
    }
    let first = vals[0] as f64;
    let last = *vals.last().unwrap() as f64;
    let m = mode_val as f64;
    let mut cumulative = 0.0;
    let ws: Vec<f64> = vals
        .iter()
        .map(|&val| {
            let vf = val as f64;
            let dw = if vf < m {
                (vf - first) / (m - first).max(1.0)
            } else {
                (last - vf) / (last - m).max(1.0)
            };
            cumulative += 1.0 + dw;
            cumulative
        })
        .collect();
    let f = rng.uniform(0.0, cumulative);
    ws.iter()
        .zip(&vals)
        .find_map(|(&w, &val)| (f < w).then_some(val))
        .unwrap_or_else(|| *vals.last().unwrap())
}

/// Geometric distribution `5 * (1 + geom(0.05))` in Myr (`randomtime` in `worldpresenter.py:43-51`).
pub fn random_time_myr(rng: &mut Pcg64Mcg) -> u32 {
    let n = (0u32..199)
        .take_while(|_| rng.uniform(0.0, 1.0) >= 0.05)
        .count() as u32;
    5 * (1 + n)
}

#[derive(Clone, Copy, Debug)]
pub struct WorldCreateParams {
    pub grid_size: u32,
    pub day_hours: u32,
    pub tilt_deg: u32,
    pub land_pct: u32,
    pub atm_dt_myr: u32,
    pub life_dt_myr: u32,
    pub people_dt_myr: u32,
    pub moon_count: u32,
    pub moon_mass: f32,
    pub moon_dist_radii: u32,
    pub meteor_freq: f32,
    /// Minimum number of full Pleistocene glaciation cycles (`32` Prehistory ticks = `128 kyr`
    /// each) before agriculture can be invented (normally distributed around `3` in random mode).
    pub prehistory_glaciation_cycles: u32,
}

impl Default for WorldCreateParams {
    fn default() -> Self {
        Self {
            grid_size: 5,
            day_hours: 24,
            tilt_deg: 23,
            land_pct: 29,
            atm_dt_myr: 125,
            life_dt_myr: 125,
            people_dt_myr: 125,
            moon_count: 1,
            moon_mass: 1.0,
            moon_dist_radii: 60,
            meteor_freq: 1.0,
            prehistory_glaciation_cycles: 3,
        }
    }
}

pub enum WorldStageState {
    Tectonics(TectonicsSimulation),
    Prehistory(PrehistorySimulation),
    History(HistorySimulation),
}

/// Complete World Simulation (`worldsimulation.py`, `worlddisplay.py`, `worldpresenter.py`).
pub struct WorldSimulation {
    params: WorldCreateParams,
    stage: WorldStageState,
    people_ticks_remaining: u32,
    tectonics_ticks: u32,
    prehistory_ticks: u32,
    history_ticks: u32,
    galaxy_type: GalaxyType,
    galactic_vantage: GalacticVantage,
    star_filter: u32,
    atmosphere_id: u32,
    home_star: HomeStar,
    atmosphere: AtmosphereType,
    optics: PlanetaryOptics,
    solar_system: Vec<SolarSystemPlanet>,
    constellations: Vec<Constellation>,
    home_planet_index: usize,
    constellation_seed: u64,
    sys_rng_bytes: [u8; 16],
    is_sol_system: bool,
    rng: Pcg64Mcg,
    cached_tectonics_biosphere: std::cell::RefCell<Option<(u32, tec11_prehistory::SettledBiosphere)>>,
}

impl WorldSimulation {
    pub fn planet_radius_km(grid_size: u32) -> f32 {
        6400.0 * (grid_size.max(1) as f32 / 5.0)
    }

    pub fn new(params: WorldCreateParams, seed: u64) -> Self {
        let _span_init = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StageWorldInit);
        let mut tec = TectonicsSimulation::new_with_full_params(
            params.grid_size,
            params.day_hours as f64,
            params.tilt_deg as f64,
            params.land_pct as f64,
            params.atm_dt_myr,
            params.life_dt_myr,
            params.people_dt_myr,
            params.moon_count,
            params.moon_mass as f64,
            params.moon_dist_radii as f64,
            seed,
        );
        tec.set_meteor_freq(params.meteor_freq as f64);
        let galaxy_type = GalaxyType::Spiral;
        let galactic_vantage = GalacticVantage::SpiralArm;
        let home_star = HomeStar::sol();
        let atmosphere = AtmosphereType::EarthLikeN2O2;
        let optics = PlanetaryOptics::compute(&home_star, atmosphere);
        let mut sys_rng = Pcg64Mcg::from_seed_u64(seed ^ 0x517C_C1B7_2722_0A95);
        let sys_rng_bytes = sys_rng.to_bytes();
        let ecliptic_pole = [0.0, -0.398, 0.917];
        let mut solar_system =
            generate_solar_system_with_rng(&mut sys_rng, &home_star, ecliptic_pole, true);
        let home_planet_index = substitute_simulated_planet(
            &mut solar_system,
            &home_star,
            Self::planet_radius_km(params.grid_size),
            params.land_pct,
            params.moon_count,
            ecliptic_pole,
        );
        let constellations = synthesize_galactic_sky_constellations(
            seed,
            true,
            galaxy_type,
            galactic_vantage,
            home_star.gal_pos_kpc,
        );
        let mut sim = Self {
            params,
            stage: WorldStageState::Tectonics(tec),
            people_ticks_remaining: (params.people_dt_myr / 5).max(1),
            tectonics_ticks: 0,
            prehistory_ticks: 0,
            history_ticks: 0,
            galaxy_type,
            galactic_vantage,
            star_filter: 2,
            atmosphere_id: 0,
            home_star,
            atmosphere,
            optics,
            solar_system,
            constellations,
            home_planet_index,
            constellation_seed: seed,
            sys_rng_bytes,
            is_sol_system: true,
            rng: Pcg64Mcg::from_seed_u64(seed.wrapping_add(1000)),
            cached_tectonics_biosphere: std::cell::RefCell::new(None),
        };
        sim.sync_counters();
        sim
    }

    /// Configure the world's Host Star (`star_filter`: `0` = Random Habitable, `1` = F, `2` = G Sol-like, `3` = K, `4` = M, `5` = A, `6` = Subgiant)
    /// and Atmospheric Gas Composition (`atmosphere_id`: `0..=5` explicit, `99` = Random).
    pub fn set_sun_and_atmosphere(&mut self, star_filter: u32, atmosphere_id: u32) {
        let mut sky_rng = self.rng.clone();
        let is_sol = star_filter == 2 && atmosphere_id == 0;
        let (gal_type, gal_vantage, gal_pos) = sample_galactic_home_with_rng(&mut sky_rng, is_sol);
        self.galaxy_type = gal_type;
        self.galactic_vantage = gal_vantage;
        let is_old_pop = matches!(
            gal_type,
            GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
        ) || gal_vantage == GalacticVantage::GalacticBulge;
        self.home_star = if is_sol {
            HomeStar::sol()
        } else {
            sample_home_star_with_rng(&mut sky_rng, gal_pos, star_filter, is_old_pop)
        };
        self.atmosphere = sample_atmosphere_with_rng(&mut sky_rng, atmosphere_id);
        self.star_filter = self.home_star.star_filter_id();
        self.atmosphere_id = self.atmosphere.id();
        self.optics = PlanetaryOptics::compute(&self.home_star, self.atmosphere);
        self.sys_rng_bytes = sky_rng.to_bytes();
        self.is_sol_system = is_sol;
        let ecliptic_pole = [0.0, -0.398, 0.917];
        self.solar_system = generate_solar_system_with_rng(
            &mut sky_rng,
            &self.home_star,
            ecliptic_pole,
            is_sol,
        );
        self.home_planet_index = substitute_simulated_planet(
            &mut self.solar_system,
            &self.home_star,
            Self::planet_radius_km(self.params.grid_size),
            self.params.land_pct,
            self.params.moon_count,
            ecliptic_pole,
        );
        self.constellations = synthesize_galactic_sky_constellations(
            self.constellation_seed,
            is_sol,
            self.galaxy_type,
            self.galactic_vantage,
            self.home_star.gal_pos_kpc,
        );
        if let WorldStageState::History(hist) = &mut self.stage {
            hist.set_solar_system(self.solar_system.clone());
            hist.set_constellations(self.constellations.clone());
            hist.set_sky_optics(self.home_star.clone(), self.optics.clone());
        }
    }

    pub fn home_star(&self) -> &HomeStar {
        &self.home_star
    }

    pub fn atmosphere(&self) -> AtmosphereType {
        self.atmosphere
    }

    pub fn optics(&self) -> &PlanetaryOptics {
        &self.optics
    }

    pub fn solar_system(&self) -> &[SolarSystemPlanet] {
        &self.solar_system
    }

    pub fn home_planet_index(&self) -> usize {
        self.home_planet_index
    }

    pub fn planets_summary_text(&self) -> String {
        let [gx, gy, gz] = self.home_star.gal_pos_kpc;
        let r_xy = (gx * gx + gy * gy).sqrt();
        format!(
            "[{} · {}, R={r_xy:.1} kpc, z={gz:+.2} kpc] {}",
            self.galaxy_type.name(),
            self.galactic_vantage.name(),
            format_solar_system_summary(&self.solar_system)
        )
    }

    /// Number of synodic months of the main moon (Moon 1, biggest in the sky) per orbital year,
    /// or `0.0` on moonless worlds.
    pub fn main_moon_synodic_months_per_year(&self) -> f64 {
        if self.params.moon_count == 0 {
            return 0.0;
        }
        let year_earth_days = (self.home_star.orbital_period_days as f64).max(5.0);
        let a1 = self.current_moon_dist_radii().max(8.0) as f64;
        let m_mass = self.params.moon_mass.max(0.05) as f64;
        let p1_sid_earth = 27.3217
            * (a1 / 60.0).powf(1.5)
            * ((1.0 + 0.0123) / (1.0 + 0.0123 * m_mass)).sqrt();
        let denom = (year_earth_days - p1_sid_earth).abs();
        let p1_syn_earth = if denom > 0.25 {
            (p1_sid_earth * year_earth_days) / denom
        } else {
            p1_sid_earth
        };
        year_earth_days / p1_syn_earth.max(0.5)
    }

    /// Human-readable local calendar summary for the `Cosmos` modal:
    /// year length in local days, synodic month length(s) in local days, and day length in hours.
    pub fn calendar_summary_text(&self) -> String {
        let day_hours = (self.current_day_hours() as f64).max(1.0);
        let year_earth_days = (self.home_star.orbital_period_days as f64).max(5.0);
        let year_local_days = year_earth_days * (24.0 / day_hours);

        let month_text = if self.params.moon_count == 0 {
            "Month: None (no moons)".to_string()
        } else {
            let a1 = self.current_moon_dist_radii().max(8.0) as f64;
            let m_mass = self.params.moon_mass.max(0.05) as f64;
            let p1_sid_earth = 27.3217
                * (a1 / 60.0).powf(1.5)
                * ((1.0 + 0.0123) / (1.0 + 0.0123 * m_mass)).sqrt();
            let count = self.params.moon_count.min(4);
            let mut synodic_months = Vec::with_capacity(count as usize);
            for k in 0..count {
                let ratio = (1u32 << k) as f64;
                let p_sid_earth = (p1_sid_earth * ratio).max(1.2);
                let denom = (year_earth_days - p_sid_earth).abs();
                let p_syn_earth = if denom > 0.25 {
                    (p_sid_earth * year_earth_days) / denom
                } else {
                    p_sid_earth
                };
                let p_syn_local = p_syn_earth * (24.0 / day_hours);
                synodic_months.push(format!("{p_syn_local:.1}"));
            }
            if count == 1 {
                format!("Month: {} days", synodic_months[0])
            } else {
                format!("Months: {} days", synodic_months.join(" / "))
            }
        };

        format!("Year: {year_local_days:.1} days · {month_text} · Day: {day_hours:.1} hours")
    }

    /// Un-tilted 3D galaxy point cloud (`1,800` stars, `6` floats per star `[nx, ny, nz, r, g, b]`)
    /// for the rotating Host Galaxy canvas inside the World `Cosmos` modal.
    pub fn galaxy_cloud_data(&self) -> Vec<f32> {
        synthesize_galaxy_cloud_normalized(self.galaxy_type, self.constellation_seed)
    }

    /// Flat star buffer `[ra_rad, dec_deg, app_mag, r, g, b, ...]` (`6` floats per star)
    /// for the 2D Northern & Southern Celestial Hemisphere Star Map in the `Cosmos` modal.
    pub fn star_map_stars_data(&self) -> Vec<f32> {
        let (stars, _) = synthesize_galactic_sky_stars_and_constellations(
            self.constellation_seed,
            self.is_sol_system,
            self.galaxy_type,
            self.galactic_vantage,
            self.home_star.gal_pos_kpc,
        );
        let (stars_buf, _, _) = build_star_map_buffers_from_sky(
            self.is_sol_system,
            self.galaxy_type,
            self.galactic_vantage,
            &self.home_star,
            &self.solar_system,
            &stars,
            &self.constellations,
        );
        stars_buf
    }

    /// Flat constellation edge buffer `[star_idx_a, star_idx_b, constellation_idx, ...]` (`3` `u32`s per edge).
    pub fn star_map_constellation_edges(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for c in &self.constellations {
            for &(ia, ib) in &c.edges {
                out.extend_from_slice(&[ia as u32, ib as u32, c.index]);
            }
        }
        out
    }

    /// Star map metadata JSON matching `SunSandbox::star_map_metadata_json`.
    pub fn star_map_metadata_json(&self) -> String {
        let (_, _, meta_json) = build_star_map_buffers_from_sky(
            self.is_sol_system,
            self.galaxy_type,
            self.galactic_vantage,
            &self.home_star,
            &self.solar_system,
            &[],
            &self.constellations,
        );
        meta_json
    }

    /// Structured JSON for the `Cosmos` modal's rotating 3D Host Galaxy view and live
    /// Solar System + Harmonic Moons animation.
    pub fn cosmos_overview_json(&self) -> String {
        let hs = &self.home_star;
        let [gx, gy, gz] = hs.gal_pos_kpc;
        let max_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
            0.020f32
        } else {
            15.0f32
        };
        let r_gal = (gx * gx + gy * gy + gz * gz).sqrt();

        let mut planet_entries = Vec::with_capacity(self.solar_system.len());
        for p in &self.solar_system {
            planet_entries.push(format!(
                "{{\"number\":{},\"label\":\"{}\",\"orbitAu\":{:.3},\"periodDays\":{:.2},\"radiusEarth\":{:.2},\"hasRings\":{},\"moonCount\":{},\"isHomeWorld\":{},\"initAngleRad\":{:.5},\"rgb\":[{:.3},{:.3},{:.3}]}}",
                p.index + 1,
                p.kind.appearance_label(),
                p.orbit_au,
                p.orbital_period_days.max(4.0),
                p.radius_earth,
                p.has_rings,
                p.moon_count,
                p.is_home_world,
                p.orbit_angle_rad,
                p.rgb[0],
                p.rgb[1],
                p.rgb[2]
            ));
        }

        let a1 = self.current_moon_dist_radii().max(8.0) as f64;
        let m_mass = self.params.moon_mass.max(0.05) as f64;
        let p1_days = if self.params.moon_count > 0 {
            27.3217 * (a1 / 60.0).powf(1.5) / (1.0 + 0.0123 * m_mass).sqrt()
        } else {
            0.0
        };
        let mut moon_entries = Vec::new();
        for k in 0..self.params.moon_count.min(4) {
            let ratio = 1u32 << k;
            let scale_a = 2.0f64.powf((2 * k) as f64 / 3.0);
            let ak = a1 * scale_a;
            let pk_days = (p1_days * (ratio as f64)).max(1.2);
            let phase_off = (k as f64) * 0.95;
            moon_entries.push(format!(
                "{{\"number\":{},\"resonanceRatio\":{},\"distRp\":{:.2},\"periodDays\":{:.2},\"massLunar\":{:.2},\"phaseOffsetRad\":{:.3}}}",
                k + 1,
                ratio,
                ak,
                pk_days,
                self.params.moon_mass,
                phase_off
            ));
        }

        format!(
            "{{\"galaxy\":{{\"typeName\":\"{}\",\"vantageName\":\"{}\",\"maxKpc\":{:.4},\"homePosKpc\":[{:.4},{:.4},{:.4}],\"homeRadiusKpc\":{:.3}}},\
              \"homeStar\":{{\"spectralType\":\"{}\",\"classDesc\":\"{}\",\"tempK\":{:.0},\"massSolar\":{:.3},\"radiusSolar\":{:.3},\"luminositySolar\":{:.3},\"habitableAu\":{:.3},\"yearDays\":{:.2},\"rgb\":[{:.3},{:.3},{:.3}]}},\
              \"homePlanetIndex\":{},\"dayHours\":{},\"tiltDeg\":{},\"planets\":[{}],\"homeMoons\":[{}]}}",
            self.galaxy_type.name(),
            self.galactic_vantage.name(),
            max_kpc,
            gx,
            gy,
            gz,
            r_gal,
            hs.spectral_type,
            hs.class_description,
            hs.temp_k,
            hs.mass_solar,
            hs.radius_solar,
            hs.luminosity_solar,
            hs.habitable_au,
            hs.orbital_period_days.max(5.0),
            hs.rgb[0],
            hs.rgb[1],
            hs.rgb[2],
            self.home_planet_index,
            self.params.day_hours,
            self.params.tilt_deg,
            planet_entries.join(","),
            moon_entries.join(",")
        )
    }

    pub fn star_filter(&self) -> u32 {
        self.star_filter
    }

    pub fn atmosphere_id(&self) -> u32 {
        self.atmosphere_id
    }

    pub fn create(&mut self, mut params: WorldCreateParams, randomize: bool) -> WorldCreateParams {
        if randomize {
            params.tilt_deg = random_spin_value(0, 45, 1, 23, &mut self.rng) as u32;
            params.land_pct = random_spin_value(1, 50, 1, 29, &mut self.rng) as u32;
            params.atm_dt_myr = random_time_myr(&mut self.rng);
            params.life_dt_myr = random_time_myr(&mut self.rng);
            params.people_dt_myr = random_time_myr(&mut self.rng);
            params.prehistory_glaciation_cycles =
                self.rng.gauss(3.0, 1.2).round().clamp(0.0, 8.0) as u32;

            // Randomize number of moons (0 or more, weighted toward 1 Earth-like moon or 2 harmonic moons):
            let moon_count_choices = [0u32, 0, 1, 1, 1, 1, 2, 2, 3, 4];
            params.moon_count =
                moon_count_choices[(self.rng.next_u64() as usize) % moon_count_choices.len()];
            if params.moon_count == 0 {
                params.moon_mass = 0.0;
                params.moon_dist_radii = 60;
                // Moonless worlds experience no lunar rotational braking, favoring faster rotation (6..=28h, mode 12h):
                params.day_hours = random_spin_value(6, 28, 1, 12, &mut self.rng) as u32;
            } else {
                // Reasonably sized moon mass in Lunar masses (0.2x..=2.5x, mode 1.0x):
                let mass_tenths = random_spin_value(2, 25, 1, 10, &mut self.rng);
                params.moon_mass = (mass_tenths as f32) / 10.0;
                // Target moon distance at Prehistory/History start in planetary radii (30..=90 Rp, mode 60 Rp):
                params.moon_dist_radii = random_spin_value(30, 90, 5, 60, &mut self.rng) as u32;
                // Continuous target day length (6..=48h) weighted by the moon's angular momentum braking:
                let h_ang =
                    TectonicsSimulation::harmonic_angular_momentum_multiplier(params.moon_count);
                let braking_factor = (params.moon_mass as f64)
                    * h_ang
                    * ((params.moon_dist_radii as f64) / 60.0).sqrt();
                let mode_day_h = (12.0 + 12.0 * braking_factor).round().clamp(8.0, 44.0) as i32;
                params.day_hours = random_spin_value(6, 48, 1, mode_day_h, &mut self.rng) as u32;
            }
            // Randomize meteor bombardment frequency in Earth-like flux multiples (0.0x..=3.6x, mode 1.0x):
            let freq_tenths = random_spin_value(0, 36, 2, 10, &mut self.rng);
            params.meteor_freq = (freq_tenths as f32) / 10.0;
        }
        let seed = self.rng.next_u64();
        let mut sky_rng = Pcg64Mcg::from_seed_u64(seed ^ 0x9E37_79B9_7F4A_7C15);
        let eff_star_filter = if randomize { 0 } else { self.star_filter };
        let eff_atm_id = if randomize { 99 } else { self.atmosphere_id };
        let is_sol = eff_star_filter == 2 && eff_atm_id == 0 && !randomize;
        let (gal_type, gal_vantage, gal_pos) = sample_galactic_home_with_rng(&mut sky_rng, is_sol);
        self.galaxy_type = gal_type;
        self.galactic_vantage = gal_vantage;
        let is_old_pop = matches!(
            gal_type,
            GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
        ) || gal_vantage == GalacticVantage::GalacticBulge;
        self.home_star = if is_sol {
            HomeStar::sol()
        } else {
            sample_home_star_with_rng(&mut sky_rng, gal_pos, eff_star_filter, is_old_pop)
        };
        self.atmosphere = sample_atmosphere_with_rng(&mut sky_rng, eff_atm_id);
        self.star_filter = self.home_star.star_filter_id();
        self.atmosphere_id = self.atmosphere.id();
        self.optics = PlanetaryOptics::compute(&self.home_star, self.atmosphere);
        self.sys_rng_bytes = sky_rng.to_bytes();
        self.is_sol_system = is_sol;
        self.constellation_seed = seed;
        let ecliptic_pole = [0.0, -0.398, 0.917];
        self.solar_system = generate_solar_system_with_rng(
            &mut sky_rng,
            &self.home_star,
            ecliptic_pole,
            is_sol,
        );
        self.home_planet_index = substitute_simulated_planet(
            &mut self.solar_system,
            &self.home_star,
            Self::planet_radius_km(params.grid_size),
            params.land_pct,
            params.moon_count,
            ecliptic_pole,
        );
        self.constellations = synthesize_galactic_sky_constellations(
            seed,
            is_sol,
            self.galaxy_type,
            self.galactic_vantage,
            self.home_star.gal_pos_kpc,
        );
        self.params = params;
        let mut tec = TectonicsSimulation::new_with_full_params(
            params.grid_size,
            params.day_hours as f64,
            params.tilt_deg as f64,
            params.land_pct as f64,
            params.atm_dt_myr,
            params.life_dt_myr,
            params.people_dt_myr,
            params.moon_count,
            params.moon_mass as f64,
            params.moon_dist_radii as f64,
            seed,
        );
        tec.set_meteor_freq(params.meteor_freq as f64);
        self.stage = WorldStageState::Tectonics(tec);
        self.people_ticks_remaining = (params.people_dt_myr / 5).max(1);
        self.tectonics_ticks = 0;
        self.prehistory_ticks = 0;
        self.history_ticks = 0;
        params
    }

    /// Advance the world simulation by 1 tick (`worldsimulation.py:77-87`).
    pub fn step(&mut self) {
        enum StageTransition {
            None,
            ToPrehistory,
            ToHistory,
        }

        let transition = match &mut self.stage {
            WorldStageState::Tectonics(tec) => {
                let _span = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StageTectonics);
                tec.step();
                self.tectonics_ticks = tec.geoticks();
                if tec.has_life() {
                    if self.people_ticks_remaining == 0 {
                        StageTransition::ToPrehistory
                    } else {
                        self.people_ticks_remaining -= 1;
                        StageTransition::None
                    }
                } else {
                    StageTransition::None
                }
            }
            WorldStageState::Prehistory(pre) => {
                let _span = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StagePrehistory);
                pre.step();
                self.prehistory_ticks = pre.prehistory_ticks();
                // Rivergoing and seafaring let prehistoric peoples disperse along rivers and across seas,
                // while transitioning to urban History happens as soon as agriculture is invented.
                if pre.active_agricultural_race_count() > 0 {
                    StageTransition::ToHistory
                } else {
                    StageTransition::None
                }
            }
            WorldStageState::History(hist) => {
                let _span = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StageHistory);
                hist.step();
                self.history_ticks = hist.history_ticks();
                StageTransition::None
            }
        };

        match transition {
            StageTransition::ToPrehistory => self.enter_prehistory_stage(),
            StageTransition::ToHistory => self.enter_history_stage(),
            StageTransition::None => {}
        }
    }

    /// Explicitly advance to the next evolutionary stage (`Tectonics -> Prehistory -> History`).
    pub fn advance_stage(&mut self) {
        match &self.stage {
            WorldStageState::Tectonics(_) => self.enter_prehistory_stage(),
            WorldStageState::Prehistory(_) => self.enter_history_stage(),
            WorldStageState::History(_) => {}
        }
    }

    fn enter_prehistory_stage(&mut self) {
        let _span = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StageToPrehistory);
        if let WorldStageState::Tectonics(tec) = &mut self.stage {
            let _sub_seed = self.rng.next_u64();
            let mut pre = PrehistorySimulation::from_tectonics_parts(
                tec.saved_environment().cloned(),
                tec.take_parts_for_prehistory(),
            );
            pre.set_min_glaciation_cycles(self.params.prehistory_glaciation_cycles);
            pre.step();
            self.prehistory_ticks = pre.prehistory_ticks();
            self.stage = WorldStageState::Prehistory(pre);
        }
    }

    fn enter_history_stage(&mut self) {
        let _span = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StageToHistory);
        if let WorldStageState::Prehistory(pre) = &mut self.stage {
            let sub_seed = self.rng.next_u64();
            let pre_copy = {
                let _span_copy =
                    tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::HistInitPrehistoryCopy);
                pre.take_for_history()
            };
            let mpy = self.main_moon_synodic_months_per_year();
            let mut hist = HistorySimulation::from_prehistory_with_calendar(
                pre_copy,
                self.params.moon_count,
                mpy,
                self.solar_system.clone(),
                self.constellations.clone(),
                sub_seed as u128,
            );
            hist.set_sky_optics(self.home_star.clone(), self.optics.clone());
            self.history_ticks = hist.history_ticks();
            self.stage = WorldStageState::History(hist);
        }
    }

    pub fn tectonics_ticks(&self) -> u32 {
        self.tectonics_ticks
    }

    pub fn prehistory_ticks(&self) -> u32 {
        self.prehistory_ticks
    }

    pub fn history_ticks(&self) -> u32 {
        self.history_ticks
    }

    fn sync_counters(&mut self) {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                self.tectonics_ticks = tec.geoticks();
            }
            WorldStageState::Prehistory(pre) => {
                self.tectonics_ticks = pre.tectonics_ticks();
                self.prehistory_ticks = pre.prehistory_ticks();
            }
            WorldStageState::History(hist) => {
                self.history_ticks = hist.history_ticks();
            }
        }
    }

    pub fn stage_index(&self) -> u32 {
        match &self.stage {
            WorldStageState::Tectonics(_) => 0,
            WorldStageState::Prehistory(_) => 1,
            WorldStageState::History(_) => 2,
        }
    }

    pub fn has_atmosphere(&self) -> bool {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.has_atmosphere(),
            WorldStageState::Prehistory(_) | WorldStageState::History(_) => true,
        }
    }

    pub fn stage_name(&self) -> &'static str {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                if tec.has_life() {
                    "Tectonics (Biosphere Active)"
                } else if tec.has_atmosphere() {
                    "Tectonics (Atmosphere Active)"
                } else {
                    "Tectonics (Hadean / Early Crust)"
                }
            }
            WorldStageState::Prehistory(_) => "Prehistory (Ice Ages & Migration)",
            WorldStageState::History(_) => "History (States, Trade & Languages)",
        }
    }

    pub fn grid(&self) -> &SphericalGrid {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.grid(),
            WorldStageState::Prehistory(pre) => pre.grid(),
            WorldStageState::History(hist) => hist.grid(),
        }
    }

    pub fn tile_count(&self) -> usize {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.tile_count(),
            WorldStageState::Prehistory(pre) => pre.tile_count(),
            WorldStageState::History(hist) => hist.tile_count(),
        }
    }

    pub fn stage(&self) -> &WorldStageState {
        &self.stage
    }

    pub fn total_years(&self) -> u64 {
        (self.tectonics_ticks as u64) * 5_000_000
            + (self.prehistory_ticks as u64) * 4_000
            + (self.history_ticks as u64) * 200
    }

    pub fn years_formatted(&self) -> String {
        format_with_commas(self.total_years())
    }

    pub fn total_population_thousands(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(_) => 0.0,
            WorldStageState::Prehistory(pre) => pre.total_population_thousands(),
            WorldStageState::History(hist) => hist.total_population_thousands(),
        }
    }

    pub fn params(&self) -> WorldCreateParams {
        self.params
    }

    pub fn current_day_hours(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_day_hours() as f32,
            _ => self.params.day_hours as f32,
        }
    }

    pub fn current_tilt_deg(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_tilt_deg() as f32,
            _ => self.params.tilt_deg as f32,
        }
    }

    pub fn current_moon_dist_radii(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_moon_dist_radii() as f32,
            _ => {
                if self.params.moon_count == 0 {
                    0.0
                } else {
                    self.params.moon_dist_radii as f32
                }
            }
        }
    }

    pub fn current_tidal_strength(&self) -> f32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.current_tidal_strength() as f32,
            _ => {
                if self.params.moon_count == 0 || self.params.moon_mass <= 0.0 {
                    0.15
                } else {
                    let a = (self.params.moon_dist_radii.max(12)) as f64;
                    let h_tide =
                        TectonicsSimulation::harmonic_tidal_multiplier(self.params.moon_count);
                    let lunar = (self.params.moon_mass as f64) * (60.0 / a).powi(3) * h_tide;
                    (0.15 + 0.85 * lunar) as f32
                }
            }
        }
    }

    pub fn impact_count(&self) -> u32 {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.impact_count(),
            WorldStageState::Prehistory(pre) => pre
                .tiles()
                .iter()
                .filter(|t| t.impact_signature() >= 0.65)
                .count() as u32,
            WorldStageState::History(hist) => {
                let snap = hist.to_snapshot();
                if let Some(pre) = PrehistorySimulation::from_snapshot(&snap, 42) {
                    pre.tiles()
                        .iter()
                        .filter(|t| t.impact_signature() >= 0.65)
                        .count() as u32
                } else {
                    0
                }
            }
        }
    }

    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let Some(tile) = tec.tiles().get(tile_idx) else {
                    return String::new();
                };
                Self::format_geology_and_climate_tree(
                    tile_idx,
                    tile,
                    tec.has_atmosphere(),
                    tec.tile_koeppen_ids()[tile_idx],
                    Some(tec.tile_temp_c()[tile_idx]),
                    Some(tec.tile_precip_norm()[tile_idx]),
                )
            }
            WorldStageState::Prehistory(pre) => {
                let Some(tile) = pre.tiles().get(tile_idx) else {
                    return String::new();
                };
                let isl_k = pre
                    .subgrid_island_koeppen_ids()
                    .get(tile_idx)
                    .copied()
                    .unwrap_or(0);
                let eff_k = if tile.elevation() <= 0.0 && isl_k != 0 {
                    isl_k
                } else {
                    pre.tile_koeppen_ids()[tile_idx]
                };
                let mut text = Self::format_geology_and_climate_tree(
                    tile_idx,
                    tile,
                    true,
                    eff_k,
                    None,
                    Some(pre.tile_precip_norm()[tile_idx]),
                );
                if let Some(&hid) = pre.populated().get(&tile_idx) {
                    let is_ag = pre.agricultural_heritages().contains(&hid);
                    let is_riv = pre.rivergoing_heritages().contains(&hid);
                    let is_sea = pre.seafaring_heritages().contains(&hid);
                    let pop_k = pre.tile_population_for_heritage_thousands(tile_idx, hid);
                    let mut tech_labels = Vec::new();
                    if is_ag {
                        tech_labels.push("Agricultural");
                    }
                    if is_riv {
                        tech_labels.push("Rivergoing");
                    }
                    if is_sea {
                        tech_labels.push("Seafaring");
                    }
                    if tech_labels.is_empty() {
                        tech_labels.push("Paleolithic");
                    }
                    text.push_str(&format!(
                        "\nPopulation: {:.1}K ({})\n",
                        pop_k,
                        tech_labels.join(" · ")
                    ));
                    text.push_str(&Self::format_heritage_ancestry(hid, pre.heritages(), 0));
                }
                text
            }
            WorldStageState::History(hist) => hist.inspect_tile(tile_idx),
        }
    }

    pub fn inspect_state_for_tile(
        &self,
        tile_idx: usize,
    ) -> Option<tec11_history::StateInspectionView> {
        match &self.stage {
            WorldStageState::History(hist) => hist.inspect_state_for_tile(tile_idx),
            _ => None,
        }
    }

    pub fn inspect_state(
        &self,
        state_id: u32,
        selected_fine_idx: Option<usize>,
    ) -> Option<tec11_history::StateInspectionView> {
        match &self.stage {
            WorldStageState::History(hist) => hist.inspect_state(state_id, selected_fine_idx),
            _ => None,
        }
    }

    pub fn inspect_tile_structured(
        &self,
        tile_idx: usize,
    ) -> Option<tec11_sky_view::TileInspectionView> {
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let tile = tec.tiles().get(tile_idx)?;
                let elev = tile.elevation();
                let has_atm = tec.has_atmosphere();
                let has_life = tec.has_life();
                let loc_summary = if !has_atm {
                    format!("Pre-atmosphere Crust | Elevation: {elev:+.2} km")
                } else if elev <= 0.0 {
                    format!(
                        "{} | Bathymetry: {elev:.2} km",
                        tec11_tectonics::tectonics::classify_submarine_tile(tile)
                    )
                } else {
                    let k_code = tec.tile_koeppen_ids().get(tile_idx).copied().unwrap_or(0);
                    format!(
                        "{} | Elevation: +{elev:.2} km",
                        koeppen_climate_name(k_code)
                    )
                };
                let surf_infos: Vec<tec11_sky_view::TileSurfaceInfo> = tec
                    .tiles()
                    .iter()
                    .map(|t| tec11_sky_view::TileSurfaceInfo::from_tuple(t.surface_info_tuple()))
                    .collect();
                let mut view = tec11_sky_view::build_coarse_tile_inspection_view(
                    tile_idx,
                    tec11_sky_view::CoarseTileSkyParams {
                        grid: tec.grid(),
                        adjacency: tec.adjacency(),
                        tiles: &surf_infos,
                        koeppen_ids: tec.tile_koeppen_ids(),
                        glacier_cover: tec.climate_model().tile_glacier_cover(),
                        tile_life: tec.tile_life(),
                        river_tiles: None,
                        radius_km: tec.radius_km(),
                        day_hours: tec.current_day_hours(),
                        tilt_deg: tec.current_tilt_deg(),
                        has_atmosphere: has_atm,
                        has_life,
                        current_year: 0,
                        moon_count: self.params.moon_count,
                        home_star: &self.home_star,
                        optics: &self.optics,
                        solar_system: &self.solar_system,
                        constellations: &self.constellations,
                        title: format!("Coarse Tile #{tile_idx}"),
                        location_summary: loc_summary,
                        population_summary: "Pre-human Geological Epoch".to_string(),
                        technology: if has_life {
                            "Biosphere".to_string()
                        } else if has_atm {
                            "Hydrosphere".to_string()
                        } else {
                            "Hadean".to_string()
                        },
                        stratigraphy_json: tec.inspect_tile_stratigraphy_json(tile_idx),
                    },
                )?;
                if has_atm && has_life {
                    let mut guard = self.cached_tectonics_biosphere.borrow_mut();
                    if !guard.as_ref().is_some_and(|(gt, _)| *gt == tec.geoticks()) {
                        let elevations: Vec<f32> =
                            tec.tiles().iter().map(|t| t.elevation()).collect();
                        let bio = tec11_prehistory::settle_species_on_climate(
                            &elevations,
                            tec.climate_model(),
                        );
                        *guard = Some((tec.geoticks(), bio));
                    }
                    if let Some((_, bio)) = guard.as_ref() {
                        let (flora, fauna) =
                            bio.select_salient_flora_and_fauna_names(tile_idx, 6, 6);
                        view.flora = flora;
                        view.fauna = fauna;
                    }
                }
                Some(view)
            }
            WorldStageState::Prehistory(pre) => {
                let tile = pre.tiles().get(tile_idx)?;
                let elev = tile.elevation();
                let koeppen = pre.tile_koeppen_ids().get(tile_idx).copied().unwrap_or(0);
                let loc_summary = if elev <= 0.0 {
                    format!(
                        "{} | Bathymetry: {elev:.2} km",
                        tec11_tectonics::tectonics::classify_submarine_tile(tile)
                    )
                } else {
                    format!(
                        "{} | Elevation: +{elev:.2} km",
                        koeppen_climate_name(koeppen)
                    )
                };
                let (pop_summary, tech_str) = if let Some(&hid) = pre.populated().get(&tile_idx) {
                    let is_ag = pre.agricultural_heritages().contains(&hid);
                    let is_riv = pre.rivergoing_heritages().contains(&hid);
                    let is_sea = pre.seafaring_heritages().contains(&hid);
                    let has_river = pre.river_tiles().get(tile_idx).copied().unwrap_or(false);
                    let is_coastal = pre
                        .adjacency()
                        .get(tile_idx)
                        .is_some_and(|nbs| nbs.iter().any(|&nb| pre.tiles()[nb].elevation() <= 0.0));
                    let pop_k = tile_population_thousands_with_water_tech(
                        koeppen,
                        is_ag,
                        is_riv && has_river,
                        is_sea && is_coastal,
                    );
                    let mut tech = Vec::new();
                    if is_ag {
                        tech.push("Agricultural");
                    }
                    if is_riv {
                        tech.push("Rivergoing");
                    }
                    if is_sea {
                        tech.push("Seafaring");
                    }
                    if tech.is_empty() {
                        tech.push("Paleolithic");
                    }
                    (format!("{pop_k:.1}K (Heritage #{hid})"), tech.join(" · "))
                } else {
                    ("Uninhabited".to_string(), "Prehistory".to_string())
                };
                let surf_infos: Vec<tec11_sky_view::TileSurfaceInfo> = pre
                    .tiles()
                    .iter()
                    .map(|t| tec11_sky_view::TileSurfaceInfo::from_tuple(t.surface_info_tuple()))
                    .collect();
                let mut view = tec11_sky_view::build_coarse_tile_inspection_view(
                    tile_idx,
                    tec11_sky_view::CoarseTileSkyParams {
                        grid: pre.grid(),
                        adjacency: pre.adjacency(),
                        tiles: &surf_infos,
                        koeppen_ids: pre.tile_koeppen_ids(),
                        glacier_cover: pre.climate_model().tile_glacier_cover(),
                        tile_life: pre.tile_life(),
                        river_tiles: Some(pre.river_tiles()),
                        radius_km: pre.radius_km(),
                        day_hours: 24.0 / pre.spin().max(0.1),
                        tilt_deg: pre.tilt_deg(),
                        has_atmosphere: true,
                        has_life: true,
                        current_year: 0,
                        moon_count: self.params.moon_count,
                        home_star: &self.home_star,
                        optics: &self.optics,
                        solar_system: &self.solar_system,
                        constellations: &self.constellations,
                        title: format!("Coarse Tile #{tile_idx}"),
                        location_summary: loc_summary,
                        population_summary: pop_summary,
                        technology: tech_str,
                        stratigraphy_json: tile.to_stratigraphy_json(tile_idx, None),
                    },
                )?;
                let (flora, fauna) = pre.salient_flora_and_fauna_for_tile(tile_idx, 6, 6);
                view.flora = flora;
                view.fauna = fauna;
                Some(view)
            }
            WorldStageState::History(hist) => hist.inspect_tile_structured(tile_idx),
        }
    }

    pub fn inspect_language(
        &self,
        lang_id: u32,
    ) -> Option<tec11_history::LanguageDetailView> {
        match &self.stage {
            WorldStageState::History(hist) => hist.inspect_language(lang_id),
            _ => None,
        }
    }

    pub fn inspect_nation(&self, nat_id: u32) -> Option<tec11_history::NationDetailView> {
        match &self.stage {
            WorldStageState::History(hist) => hist.inspect_nation(nat_id),
            _ => None,
        }
    }

    fn format_geology_and_climate_tree(
        tile_idx: usize,
        tile: &TileGeology,
        has_atm: bool,
        koeppen: u8,
        temp_c: Option<f32>,
        precip_norm: Option<f32>,
    ) -> String {
        let mut lines = Vec::new();
        let elev = tile.elevation();
        lines.push(format!(
            "Tile #{tile_idx} | Elev: {elev:+.2} km | Crust: {:.1} km",
            tile.thickness()
        ));
        if has_atm && elev > 0.0 {
            let mut clim_line = format!("Climate: {}", koeppen_climate_name(koeppen));
            if let Some(tc) = temp_c {
                clim_line.push_str(&format!(" | Temp: {tc:+.1} °C"));
            }
            if let Some(p) = precip_norm {
                clim_line.push_str(&format!(" | Precip: {:.0}%", p * 100.0));
            }
            lines.push(clim_line);
        }
        let sig = tile.impact_signature();
        if sig >= 0.65 {
            lines.push("Meteor Scar: Impact Crater Basin (Shocked Hornfels / Melt)".to_string());
        } else if sig >= 0.35 {
            lines.push("Meteor Scar: Impact Ejecta Rim (Breccia Conglomerate)".to_string());
        }
        lines.push("Layers (surface -> Moho):".to_string());
        let mut depth_from_surface = 0.0f32;
        for (idx, layer) in tile.layers.iter().enumerate().rev() {
            let r = &layer.rock;
            let deposits = tile.derive_layer_deposits_with_elev(idx, depth_from_surface, elev);
            depth_from_surface += layer.thickness;
            let dep_suffix = if deposits.is_empty() {
                String::new()
            } else {
                format!(" → {}", deposits.join(" + "))
            };
            lines.push(format!(
                "  • {} ({:.2} km, {}, felsity {:.2}, tough {:.2}){dep_suffix}",
                r.name,
                layer.thickness,
                r.kind.code(),
                r.felsity,
                r.toughness
            ));
            if idx > 0 {
                let intf =
                    tile.derive_interface_occurrences_with_elev(idx - 1, depth_from_surface, elev);
                if !intf.is_empty() {
                    let labels: Vec<&str> = intf.iter().map(|o| o.label.as_str()).collect();
                    lines.push(format!(
                        "    ┄ Interface ({depth_from_surface:.2} km): {}",
                        labels.join(" + ")
                    ));
                }
            }
        }
        lines.join("\n")
    }

    pub fn inspect_tile_stratigraphy_json(&self, tile_idx: usize) -> String {
        match &self.stage {
            WorldStageState::Tectonics(tec) => tec.inspect_tile_stratigraphy_json(tile_idx),
            WorldStageState::Prehistory(pre) => pre
                .tiles()
                .get(tile_idx)
                .map(|t| t.to_stratigraphy_json(tile_idx, None))
                .unwrap_or_else(|| "{\"hasColumn\":false}".to_string()),
            WorldStageState::History(hist) => hist.inspect_tile_stratigraphy_json(tile_idx),
        }
    }

    fn format_heritage_ancestry(
        hid: u32,
        heritages: &std::collections::BTreeMap<u32, Heritage>,
        depth: usize,
    ) -> String {
        if depth > 5 {
            return String::new();
        }
        let indent = "  ".repeat(depth);
        let Some(h) = heritages.get(&hid) else {
            return format!("{indent}• people_{}", hid + 1);
        };
        let mut out = format!("{indent}• {}", h.name);
        for &p in &h.ancestry {
            let child = Self::format_heritage_ancestry(p, heritages, depth + 1);
            if !child.is_empty() {
                out.push('\n');
                out.push_str(&child);
            }
        }
        out
    }

    /// Compute RGB vertex/tile colors for the four World globe modes:
    /// `"color"` (physical terrain with elevation darkening and no political borders),
    /// `"climate"` (Köppen climate zones + annual-mean ocean current/SST shading),
    /// `"borders"` (sovereign states + distinct terracotta for stateless populations),
    /// and `"population"` (log-density population heatmap).
    /// Blend an ocean tile's RGB color toward bright polar sea-ice pack (`[0.84, 0.91, 0.97]`)
    /// based on fractional `sea_ice` cover in `[0.0, 1.0]`.
    #[inline]
    pub fn blend_sea_ice_rgb(base: [f32; 3], sea_ice: f32) -> [f32; 3] {
        tec11_sky_view::blend_sea_ice_rgb(base, sea_ice)
    }

    pub fn tile_colors_rgb(&self, aspect: &str, selected_tile: Option<usize>) -> Vec<f32> {
        let mut out = match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let n = tec.tile_count();
                let tiles = tec.tiles();
                let koeppen = tec.tile_koeppen_ids();
                let life = tec.tile_life();
                let scars = tec.tile_impact_scars();
                let has_atm = tec.has_atmosphere();
                let ocean = has_atm.then(|| tec.climate_model().try_ocean()).flatten();
                (0..n)
                    .flat_map(|t| {
                        let h = tiles[t].elevation();
                        match aspect {
                            "climate" => {
                                if !has_atm {
                                    if h > 0.0 {
                                        [0.45, 0.45, 0.48]
                                    } else {
                                        [0.05, 0.07, 0.12]
                                    }
                                } else if h > 0.0 {
                                    koeppen_rgb(koeppen[t])
                                } else {
                                    let sst = ocean
                                        .and_then(|o| o.sst_anomaly.get(t).copied())
                                        .unwrap_or(0.0);
                                    let spd = ocean
                                        .and_then(|o| o.speed.get(t).copied())
                                        .unwrap_or(0.0);
                                    let ice = ocean
                                        .and_then(|o| o.sea_ice_cover.get(t).copied())
                                        .unwrap_or(0.0);
                                    Self::blend_sea_ice_rgb(Self::climate_ocean_rgb(sst, spd), ice)
                                }
                            }
                            "population" => {
                                if h > 0.0 {
                                    [0.13, 0.16, 0.19]
                                } else {
                                    [0.03, 0.06, 0.14]
                                }
                            }
                            "borders" | "nations" => {
                                let base = Self::natural_color_shaded(
                                    h,
                                    tiles[t].mountainosity,
                                    tiles[t].top_felsity(),
                                    koeppen[t],
                                    life[t],
                                    has_atm,
                                    &self.optics,
                                );
                                if h > 0.0 {
                                    [base[0] * 0.72, base[1] * 0.72, base[2] * 0.72]
                                } else {
                                    base
                                }
                            }
                            "meteors" => {
                                let base = Self::natural_color_shaded(
                                    h,
                                    tiles[t].mountainosity,
                                    tiles[t].top_felsity(),
                                    koeppen[t],
                                    life[t],
                                    has_atm,
                                    &self.optics,
                                );
                                let scar = scars[t].max(tiles[t].impact_signature());
                                Self::blend_impact_scar_color(base, scar)
                            }
                            _ => {
                                let base = Self::natural_color_shaded(
                                    h,
                                    tiles[t].mountainosity,
                                    tiles[t].top_felsity(),
                                    koeppen[t],
                                    life[t],
                                    has_atm,
                                    &self.optics,
                                );
                                if h <= 0.0 && has_atm {
                                    let ice = ocean
                                        .and_then(|o| o.sea_ice_cover.get(t).copied())
                                        .unwrap_or(0.0);
                                    Self::blend_sea_ice_rgb(base, ice)
                                } else {
                                    base
                                }
                            }
                        }
                    })
                    .collect()
            }
            WorldStageState::Prehistory(pre) => {
                let n = pre.tile_count();
                let tiles = pre.tiles();
                let koeppen = pre.tile_koeppen_ids();
                let island_koeppen = pre.subgrid_island_koeppen_ids();
                let life = pre.tile_life();
                let rivers = pre.river_tiles();
                let populated = pre.populated();
                let elevs = pre.tile_elevations();
                let ocean = pre.climate_model().try_ocean();
                (0..n)
                    .flat_map(|t| {
                        let h = elevs[t];
                        let island_k = island_koeppen.get(t).copied().unwrap_or(0);
                        match aspect {
                            "climate" => {
                                if h > 0.0 {
                                    koeppen_rgb(koeppen[t])
                                } else if island_k != 0 {
                                    koeppen_rgb(island_k)
                                } else {
                                    let sst = ocean
                                        .and_then(|o| o.sst_anomaly.get(t).copied())
                                        .unwrap_or(0.0);
                                    let spd = ocean
                                        .and_then(|o| o.speed.get(t).copied())
                                        .unwrap_or(0.0);
                                    let ice = ocean
                                        .and_then(|o| o.sea_ice_cover.get(t).copied())
                                        .unwrap_or(0.0);
                                    Self::blend_sea_ice_rgb(Self::climate_ocean_rgb(sst, spd), ice)
                                }
                            }
                            "population" => {
                                if let Some(&hid) = populated.get(&t) {
                                    let pop_k = pre.tile_population_for_heritage_thousands(t, hid);
                                    if pop_k > 0.01 {
                                        let s = ((1.0 + pop_k).ln() / (1.0 + 220.0_f32).ln())
                                            .clamp(0.05, 1.0);
                                        HistorySimulation::population_heat_rgb(s)
                                    } else if h > 0.0 || island_k != 0 {
                                        [0.13, 0.16, 0.19]
                                    } else {
                                        [0.03, 0.06, 0.14]
                                    }
                                } else if h > 0.0 || island_k != 0 {
                                    [0.13, 0.16, 0.19]
                                } else {
                                    [0.03, 0.06, 0.14]
                                }
                            }
                            "borders" | "nations" => {
                                if populated.contains_key(&t) {
                                    // Stateless prehistoric populations: distinct warm terracotta
                                    [0.82, 0.56, 0.28]
                                } else if h > 0.0 && rivers.get(t).copied().unwrap_or(false) {
                                    [0.30, 0.78, 0.88]
                                } else {
                                    let base = Self::natural_color_shaded(
                                        h,
                                        tiles[t].mountainosity,
                                        tiles[t].top_felsity(),
                                        koeppen[t],
                                        life[t],
                                        true,
                                        &self.optics,
                                    );
                                    if h > 0.0 {
                                        [base[0] * 0.75, base[1] * 0.75, base[2] * 0.75]
                                    } else {
                                        base
                                    }
                                }
                            }
                            "meteors" => {
                                let base = Self::natural_color_shaded(
                                    h,
                                    tiles[t].mountainosity,
                                    tiles[t].top_felsity(),
                                    koeppen[t],
                                    life[t],
                                    true,
                                    &self.optics,
                                );
                                Self::blend_impact_scar_color(base, tiles[t].impact_signature())
                            }
                            _ => {
                                if h > 0.0 && rivers.get(t).copied().unwrap_or(false) {
                                    [0.35, 0.88, 0.96]
                                } else {
                                    let base = Self::natural_color_shaded(
                                        h,
                                        tiles[t].mountainosity,
                                        tiles[t].top_felsity(),
                                        koeppen[t],
                                        life[t],
                                        true,
                                        &self.optics,
                                    );
                                    if h <= 0.0 {
                                        let ice = ocean
                                            .and_then(|o| o.sea_ice_cover.get(t).copied())
                                            .unwrap_or(0.0);
                                        Self::blend_sea_ice_rgb(base, ice)
                                    } else {
                                        base
                                    }
                                }
                            }
                        }
                    })
                    .collect()
            }
            WorldStageState::History(hist) => match aspect {
                "color" => hist.tile_colors_rgb_with_optics(
                    "color",
                    true,
                    selected_tile,
                    Some(&self.optics),
                ),
                "climate" => {
                    let pre = hist.prehistory();
                    let coarse_elevs = pre.tile_elevations();
                    let koeppen = hist.coarse_koeppen();
                    let island_koeppen = pre.subgrid_island_koeppen_ids();
                    let ocean = pre.climate_model().try_ocean();
                    let terrain = hist.terrain();
                    terrain
                        .display_faces
                        .iter()
                        .flat_map(|face| match face.source {
                            tec11_terrain::DisplayFaceSource::Fine(f_idx) => {
                                let c = terrain.primary_coarse_tile(f_idx, &coarse_elevs);
                                if terrain.fine_elevations[f_idx] > 0.0 {
                                    let k = if koeppen[c] != 0 {
                                        koeppen[c]
                                    } else {
                                        island_koeppen.get(c).copied().unwrap_or(0)
                                    };
                                    koeppen_rgb(k)
                                } else {
                                    let sst = ocean
                                        .and_then(|o| o.sst_anomaly.get(c).copied())
                                        .unwrap_or(0.0);
                                    let spd = ocean
                                        .and_then(|o| o.speed.get(c).copied())
                                        .unwrap_or(0.0);
                                    let ice = ocean
                                        .and_then(|o| o.sea_ice_cover.get(c).copied())
                                        .unwrap_or(0.0);
                                    Self::blend_sea_ice_rgb(Self::climate_ocean_rgb(sst, spd), ice)
                                }
                            }
                            tec11_terrain::DisplayFaceSource::OceanCoarse(c_idx) => {
                                let sst = ocean
                                    .and_then(|o| o.sst_anomaly.get(c_idx).copied())
                                    .unwrap_or(0.0);
                                let spd = ocean
                                    .and_then(|o| o.speed.get(c_idx).copied())
                                    .unwrap_or(0.0);
                                let ice = ocean
                                    .and_then(|o| o.sea_ice_cover.get(c_idx).copied())
                                    .unwrap_or(0.0);
                                Self::blend_sea_ice_rgb(Self::climate_ocean_rgb(sst, spd), ice)
                            }
                        })
                        .collect()
                }
                "population" => hist.tile_colors_rgb_with_optics(
                    "population",
                    false,
                    selected_tile,
                    Some(&self.optics),
                ),
                "borders" | "nations" => hist.tile_colors_rgb_with_optics(
                    "borders",
                    true,
                    selected_tile,
                    Some(&self.optics),
                ),
                "travel" => hist.tile_colors_rgb_with_optics(
                    "travel",
                    true,
                    selected_tile,
                    Some(&self.optics),
                ),
                "metallurgy" => hist.tile_colors_rgb_with_optics(
                    "metallurgy",
                    true,
                    selected_tile,
                    Some(&self.optics),
                ),
                _ => hist.tile_colors_rgb_with_optics(
                    "color",
                    true,
                    selected_tile,
                    Some(&self.optics),
                ),
            },
        };

        if let Some(sel) = selected_tile {
            if sel * 3 + 2 < out.len() {
                out[sel * 3] = 1.0;
                out[sel * 3 + 1] = 0.0;
                out[sel * 3 + 2] = 0.0;
            }
        }
        out
    }

    /// Annual-mean ocean surface color in `"climate"` mode, combining current speed `[0, 1]`
    /// and sea-surface temperature anomaly `[-1, 1]` (warm poleward vs. cold equatorward/upwelling).
    pub fn climate_ocean_rgb(sst_anom: f32, speed: f32) -> [f32; 3] {
        let spd = speed.clamp(0.0, 1.0);
        let base_r = 0.03 + 0.06 * spd;
        let base_g = 0.08 + 0.12 * spd;
        let base_b = 0.18 + 0.16 * spd;
        if sst_anom >= 0.0 {
            let warm = sst_anom.clamp(0.0, 1.0) * (0.35 + 0.65 * spd);
            [
                (base_r + 0.42 * warm).clamp(0.0, 1.0),
                (base_g + 0.18 * warm).clamp(0.0, 1.0),
                (base_b + 0.04 * warm).clamp(0.0, 1.0),
            ]
        } else {
            let cold = (-sst_anom).clamp(0.0, 1.0) * (0.35 + 0.65 * spd);
            [
                (base_r + 0.02 * cold).clamp(0.0, 1.0),
                (base_g + 0.28 * cold).clamp(0.0, 1.0),
                (base_b + 0.42 * cold).clamp(0.0, 1.0),
            ]
        }
    }

    /// Colored 3D line-segment buffer (`[x0, y0, z0, x1, y1, z1, r, g, b, ...]`, 9 floats per segment)
    /// for annual-mean surface ocean current arrows (shaft + 2 barbs per active coarse ocean tile)
    /// rendered on the 3D globe in `"climate"` mode.
    pub fn ocean_current_segments_colored(&self) -> Vec<f32> {
        let (grid, ocean, elevs): (&SphericalGrid, &tec11_climate::OceanCirculation, Vec<f32>) =
            match &self.stage {
                WorldStageState::Tectonics(tec) => {
                    if !tec.has_atmosphere() {
                        return Vec::new();
                    }
                    let Some(ocean) = tec.climate_model().try_ocean() else {
                        return Vec::new();
                    };
                    (
                        tec.grid(),
                        ocean,
                        tec.tiles().iter().map(|t| t.elevation()).collect(),
                    )
                }
                WorldStageState::Prehistory(pre) => {
                    let Some(ocean) = pre.climate_model().try_ocean() else {
                        return Vec::new();
                    };
                    (pre.grid(), ocean, pre.tile_elevations())
                }
                WorldStageState::History(hist) => {
                    let pre = hist.prehistory();
                    let Some(ocean) = pre.climate_model().try_ocean() else {
                        return Vec::new();
                    };
                    (pre.grid(), ocean, pre.tile_elevations())
                }
            };

        let rotate_axes = |v: [f64; 3]| -> [f32; 3] { [-v[1] as f32, v[2] as f32, v[0] as f32] };

        let n = grid.faces.len();
        let mut out = Vec::with_capacity(n * 27);
        for v in 0..n {
            if elevs.get(v).copied().unwrap_or(1.0) > 0.0 {
                continue;
            }
            let spd = ocean.speed.get(v).copied().unwrap_or(0.0);
            if spd < 0.05 {
                continue;
            }
            let vel = ocean.velocity_xyz[v];
            let vmag = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
            if vmag < 1e-6 {
                continue;
            }
            let dx = (vel[0] / vmag) as f64;
            let dy = (vel[1] / vmag) as f64;
            let dz = (vel[2] / vmag) as f64;

            let ([nx, ny, nz], _) = grid.faces[v];
            let px = ny * dz - nz * dy;
            let py = nz * dx - nx * dz;
            let pz = nx * dy - ny * dx;

            let r_lift = 1.004;
            let cx = nx * r_lift;
            let cy = ny * r_lift;
            let cz = nz * r_lift;

            let half_len = 0.007 + 0.013 * (spd as f64).sqrt();
            let tail = [
                cx - half_len * dx,
                cy - half_len * dy,
                cz - half_len * dz,
            ];
            let tip = [
                cx + half_len * dx,
                cy + half_len * dy,
                cz + half_len * dz,
            ];
            let barb_back = 0.65 * half_len;
            let barb_wing = 0.42 * half_len;
            let left = [
                tip[0] - barb_back * dx + barb_wing * px,
                tip[1] - barb_back * dy + barb_wing * py,
                tip[2] - barb_back * dz + barb_wing * pz,
            ];
            let right = [
                tip[0] - barb_back * dx - barb_wing * px,
                tip[1] - barb_back * dy - barb_wing * py,
                tip[2] - barb_back * dz - barb_wing * pz,
            ];

            let sst = ocean.sst_anomaly.get(v).copied().unwrap_or(0.0);
            let rgb = if sst >= 0.08 {
                let w = ((sst - 0.08) / 0.72).clamp(0.0, 1.0);
                [
                    0.88 + 0.12 * w,
                    0.68 - 0.20 * w,
                    0.34 - 0.18 * w,
                ]
            } else if sst <= -0.08 {
                let c = ((-sst - 0.08) / 0.72).clamp(0.0, 1.0);
                [
                    0.42 - 0.22 * c,
                    0.82 + 0.12 * c,
                    0.96 + 0.04 * c,
                ]
            } else {
                [0.68, 0.82, 0.92]
            };

            for (a, b) in [(tail, tip), (tip, left), (tip, right)] {
                let ra = rotate_axes(a);
                let rb = rotate_axes(b);
                out.extend_from_slice(&[
                    ra[0], ra[1], ra[2], rb[0], rb[1], rb[2], rgb[0], rgb[1], rgb[2],
                ]);
            }
        }
        out
    }

    fn blend_impact_scar_color(base: [f32; 3], scar: f32) -> [f32; 3] {
        if scar <= 0.05 {
            return [base[0] * 0.72, base[1] * 0.72, base[2] * 0.72];
        }
        if scar >= 0.60 {
            // Crater basin bullseye (shocked hornfels / impact melt): glowing crimson-orange
            let w = ((scar - 0.50) / 0.50).clamp(0.65, 0.96);
            [
                base[0] * (1.0 - w) + 0.98 * w,
                base[1] * (1.0 - w) + 0.24 * w,
                base[2] * (1.0 - w) + 0.10 * w,
            ]
        } else if scar >= 0.32 {
            // Raised ejecta breccia rim (conglomerate): bright amber-gold
            let w = ((scar - 0.25) / 0.35).clamp(0.55, 0.90);
            [
                base[0] * (1.0 - w) + 0.96 * w,
                base[1] * (1.0 - w) + 0.76 * w,
                base[2] * (1.0 - w) + 0.10 * w,
            ]
        } else {
            // Outer shock ring / weathered crater halo: warm copper
            let w = (scar * 2.5).clamp(0.25, 0.75);
            [
                base[0] * (1.0 - w) + 0.85 * w,
                base[1] * (1.0 - w) + 0.45 * w,
                base[2] * (1.0 - w) + 0.20 * w,
            ]
        }
    }

    /// Build a tangent-plane regional polygon map JSON centered dead-on `center_idx` (`u = 0.5, v = 0.5`),
    /// rendered using the active globe mode (`"color"`, `"climate"`, `"borders"`, `"population"`),
    /// including 2D ocean current arrows in `"climate"` mode and active travel routes in `"borders"` mode.
    pub fn regional_tile_map_json(&self, center_idx: usize, aspect: &str) -> String {
        let dot = |a: [f64; 3], b: [f64; 3]| -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] };
        let cross = |a: [f64; 3], b: [f64; 3]| -> [f64; 3] {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let norm3 = |v: [f64; 3]| -> [f64; 3] {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 1e-12 {
                [v[0] / l, v[1] / l, v[2] / l]
            } else {
                [1.0, 0.0, 0.0]
            }
        };

        let (center, n_coarse) = match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let Some((c, _)) = tec.grid().faces.get(center_idx) else {
                    return "{\"hasTile\":false}".to_string();
                };
                (norm3(*c), tec.tile_count().max(1))
            }
            WorldStageState::Prehistory(pre) => {
                let Some((c, _)) = pre.grid().faces.get(center_idx) else {
                    return "{\"hasTile\":false}".to_string();
                };
                (norm3(*c), pre.tile_count().max(1))
            }
            WorldStageState::History(hist) => {
                let Some(df) = hist.terrain().display_faces.get(center_idx) else {
                    return "{\"hasTile\":false}".to_string();
                };
                (norm3(df.center), hist.prehistory().tile_count().max(1))
            }
        };

        let ref_up = if center[2].abs() < 0.95 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let east = norm3(cross(ref_up, center));
        let north = norm3(cross(center, east));

        let is_history = matches!(self.stage, WorldStageState::History(_));
        let half_span: f32 = if is_history {
            (2.25 / (n_coarse as f32).sqrt()).clamp(0.14, 0.36)
        } else {
            (2.90 / (n_coarse as f32).sqrt()).clamp(0.18, 0.44)
        };
        let span = 2.0 * half_span;
        let cos_cutoff = ((half_span * 1.65).min(1.15) as f64).cos();

        let project_uv = |v: [f64; 3]| -> [f32; 2] {
            let x = -dot(v, east) as f32;
            let y = -dot(v, north) as f32;
            [0.5 + x / span, 0.5 + y / span]
        };

        // Compute true mode colors without overwriting `center_idx` with solid selection red:
        let raw_unselected = self.tile_colors_rgb(aspect, None);
        let mut colors = if is_history && (aspect == "borders" || aspect == "nations") {
            let mut c = self.tile_colors_rgb(aspect, Some(center_idx));
            if center_idx * 3 + 2 < c.len() && center_idx * 3 + 2 < raw_unselected.len() {
                // Keep diplomatic green/yellow/red state shading while restoring `center_idx` from `[1,0,0]`
                // if `center_idx` belongs to a state, or its true terrain/stateless color otherwise:
                if let WorldStageState::History(hist) = &self.stage {
                    let is_in_state = hist
                        .terrain()
                        .display_faces
                        .get(center_idx)
                        .is_some_and(|df| match df.source {
                            tec11_terrain::DisplayFaceSource::Fine(f) => {
                                hist.state_at_tile(f).is_some()
                            }
                            _ => false,
                        });
                    if is_in_state {
                        c[center_idx * 3] = 0.15;
                        c[center_idx * 3 + 1] = 0.95;
                        c[center_idx * 3 + 2] = 0.35;
                    } else {
                        c[center_idx * 3] = raw_unselected[center_idx * 3];
                        c[center_idx * 3 + 1] = raw_unselected[center_idx * 3 + 1];
                        c[center_idx * 3 + 2] = raw_unselected[center_idx * 3 + 2];
                    }
                }
            }
            c
        } else {
            raw_unselected
        };
        if colors.len() < 3 {
            colors = vec![0.2, 0.3, 0.4];
        }

        let mut poly_entries = Vec::new();
        match &self.stage {
            WorldStageState::Tectonics(tec) => {
                let grid = tec.grid();
                for (t, (fc, verts)) in grid.faces.iter().enumerate() {
                    if dot(*fc, center) < cos_cutoff || verts.len() < 3 {
                        continue;
                    }
                    let mut u_min = f32::INFINITY;
                    let mut u_max = f32::NEG_INFINITY;
                    let mut v_min = f32::INFINITY;
                    let mut v_max = f32::NEG_INFINITY;
                    let mut pts_str = Vec::with_capacity(verts.len());
                    for &vv in verts {
                        let [u, v] = project_uv(vv);
                        u_min = u_min.min(u);
                        u_max = u_max.max(u);
                        v_min = v_min.min(v);
                        v_max = v_max.max(v);
                        pts_str.push(format!("[{:.4},{:.4}]", u, v));
                    }
                    if u_max < -0.08 || u_min > 1.08 || v_max < -0.08 || v_min > 1.08 {
                        continue;
                    }
                    let b = t * 3;
                    let rgb = if b + 2 < colors.len() {
                        [colors[b], colors[b + 1], colors[b + 2]]
                    } else {
                        [0.2, 0.2, 0.2]
                    };
                    poly_entries.push(format!(
                        "{{\"tileIdx\":{},\"rgb\":[{:.3},{:.3},{:.3}],\"isSelected\":{},\"isRiver\":false,\"isCapital\":false,\"stateId\":-1,\"pts\":[{}]}}",
                        t,
                        rgb[0],
                        rgb[1],
                        rgb[2],
                        t == center_idx,
                        pts_str.join(",")
                    ));
                }
            }
            WorldStageState::Prehistory(pre) => {
                let grid = pre.grid();
                let rivers = pre.river_tiles();
                for (t, (fc, verts)) in grid.faces.iter().enumerate() {
                    if dot(*fc, center) < cos_cutoff || verts.len() < 3 {
                        continue;
                    }
                    let mut u_min = f32::INFINITY;
                    let mut u_max = f32::NEG_INFINITY;
                    let mut v_min = f32::INFINITY;
                    let mut v_max = f32::NEG_INFINITY;
                    let mut pts_str = Vec::with_capacity(verts.len());
                    for &vv in verts {
                        let [u, v] = project_uv(vv);
                        u_min = u_min.min(u);
                        u_max = u_max.max(u);
                        v_min = v_min.min(v);
                        v_max = v_max.max(v);
                        pts_str.push(format!("[{:.4},{:.4}]", u, v));
                    }
                    if u_max < -0.08 || u_min > 1.08 || v_max < -0.08 || v_min > 1.08 {
                        continue;
                    }
                    let b = t * 3;
                    let rgb = if b + 2 < colors.len() {
                        [colors[b], colors[b + 1], colors[b + 2]]
                    } else {
                        [0.2, 0.2, 0.2]
                    };
                    let is_riv = rivers.get(t).copied().unwrap_or(false) && aspect != "population";
                    poly_entries.push(format!(
                        "{{\"tileIdx\":{},\"rgb\":[{:.3},{:.3},{:.3}],\"isSelected\":{},\"isRiver\":{},\"isCapital\":false,\"stateId\":-1,\"pts\":[{}]}}",
                        t,
                        rgb[0],
                        rgb[1],
                        rgb[2],
                        t == center_idx,
                        is_riv,
                        pts_str.join(",")
                    ));
                }
            }
            WorldStageState::History(hist) => {
                let terrain = hist.terrain();
                let rivers = hist.river_tiles();
                for (d_idx, df) in terrain.display_faces.iter().enumerate() {
                    if dot(df.center, center) < cos_cutoff || df.vertices.len() < 3 {
                        continue;
                    }
                    let mut u_min = f32::INFINITY;
                    let mut u_max = f32::NEG_INFINITY;
                    let mut v_min = f32::INFINITY;
                    let mut v_max = f32::NEG_INFINITY;
                    let mut pts_str = Vec::with_capacity(df.vertices.len());
                    for &vv in &df.vertices {
                        let [u, v] = project_uv(vv);
                        u_min = u_min.min(u);
                        u_max = u_max.max(u);
                        v_min = v_min.min(v);
                        v_max = v_max.max(v);
                        pts_str.push(format!("[{:.4},{:.4}]", u, v));
                    }
                    if u_max < -0.08 || u_min > 1.08 || v_max < -0.08 || v_min > 1.08 {
                        continue;
                    }
                    let b = d_idx * 3;
                    let rgb = if b + 2 < colors.len() {
                        [colors[b], colors[b + 1], colors[b + 2]]
                    } else {
                        [0.2, 0.2, 0.2]
                    };
                    let (state_id, is_riv, is_cap) = match df.source {
                        tec11_terrain::DisplayFaceSource::Fine(f) => (
                            hist.state_at_tile(f).map(|s| s as i32).unwrap_or(-1),
                            rivers.get(f).copied().unwrap_or(false) && aspect != "population",
                            (aspect == "borders" || aspect == "nations")
                                && hist.is_state_capital_fine_face(f),
                        ),
                        tec11_terrain::DisplayFaceSource::OceanCoarse(_) => (-1, false, false),
                    };
                    poly_entries.push(format!(
                        "{{\"tileIdx\":{},\"rgb\":[{:.3},{:.3},{:.3}],\"isSelected\":{},\"isRiver\":{},\"isCapital\":{},\"stateId\":{},\"pts\":[{}]}}",
                        d_idx,
                        rgb[0],
                        rgb[1],
                        rgb[2],
                        d_idx == center_idx,
                        is_riv,
                        is_cap,
                        state_id,
                        pts_str.join(",")
                    ));
                }
            }
        }

        // Mode-specific 2D vector overlays:
        let mut current_entries = Vec::new();
        if aspect == "climate" {
            let opt_ocean = match &self.stage {
                WorldStageState::Tectonics(tec) => {
                    if tec.has_atmosphere() {
                        tec.climate_model().try_ocean().map(|ocean| {
                            (
                                tec.grid(),
                                ocean,
                                tec.tiles().iter().map(|t| t.elevation()).collect::<Vec<_>>(),
                            )
                        })
                    } else {
                        None
                    }
                }
                WorldStageState::Prehistory(pre) => pre
                    .climate_model()
                    .try_ocean()
                    .map(|ocean| (pre.grid(), ocean, pre.tile_elevations())),
                WorldStageState::History(hist) => {
                    let pre = hist.prehistory();
                    pre.climate_model()
                        .try_ocean()
                        .map(|ocean| (pre.grid(), ocean, pre.tile_elevations()))
                }
            };
            if let Some((c_grid, ocean, elevs)) = opt_ocean {
                for (c_idx, (fc, _)) in c_grid.faces.iter().enumerate() {
                    if elevs.get(c_idx).copied().unwrap_or(1.0) > 0.0 || dot(*fc, center) < cos_cutoff {
                        continue;
                    }
                    let spd = ocean.speed.get(c_idx).copied().unwrap_or(0.0);
                    if spd < 0.05 {
                        continue;
                    }
                    let [u, v] = project_uv(*fc);
                    if !(-0.02..=1.02).contains(&u) || !(-0.02..=1.02).contains(&v) {
                        continue;
                    }
                    let vel = ocean.velocity_xyz[c_idx];
                    let v3 = [vel[0] as f64, vel[1] as f64, vel[2] as f64];
                    let vx = -dot(v3, east) as f32;
                    let vy = -dot(v3, north) as f32;
                    let vlen = (vx * vx + vy * vy).sqrt();
                    if vlen < 1e-5 {
                        continue;
                    }
                    let sst = ocean.sst_anomaly.get(c_idx).copied().unwrap_or(0.0);
                    current_entries.push(format!(
                        "[{:.4},{:.4},{:.4},{:.4},{:.3},{:.3}]",
                        u,
                        v,
                        vx / vlen,
                        vy / vlen,
                        spd.clamp(0.0, 1.0),
                        sst.clamp(-1.0, 1.0)
                    ));
                }
            }
        }

        let mut route_entries = Vec::new();
        if aspect == "borders" || aspect == "nations" {
            if let WorldStageState::History(hist) = &self.stage {
                for (pts, rgb) in hist.active_route_polylines_grid() {
                    for seg in pts.windows(2) {
                        let a = seg[0];
                        let b = seg[1];
                        if dot(a, center) < cos_cutoff && dot(b, center) < cos_cutoff {
                            continue;
                        }
                        let [u0, v0] = project_uv(a);
                        let [u1, v1] = project_uv(b);
                        if (u0.max(u1) < -0.05 || u0.min(u1) > 1.05)
                            || (v0.max(v1) < -0.05 || v0.min(v1) > 1.05)
                        {
                            continue;
                        }
                        route_entries.push(format!(
                            "[{:.4},{:.4},{:.4},{:.4},{:.3},{:.3},{:.3}]",
                            u0, v0, u1, v1, rgb[0], rgb[1], rgb[2]
                        ));
                    }
                }
            }
        }

        format!(
            "{{\"hasTile\":true,\"centerTileIdx\":{},\"aspect\":\"{}\",\"polygons\":[{}],\"currents\":[{}],\"routes\":[{}]}}",
            center_idx,
            aspect,
            poly_entries.join(","),
            current_entries.join(","),
            route_entries.join(",")
        )
    }

    /// Physical surface color with elevation and ruggedness darkening so mountain ranges
    /// and high cordilleras stand out clearly in `"color"` mode across all stages.
    pub fn natural_color_shaded(
        h: f32,
        mountainosity: f32,
        felsity: f32,
        koeppen: u8,
        life: f32,
        has_atm: bool,
        optics: &PlanetaryOptics,
    ) -> [f32; 3] {
        tec11_sky_view::natural_color_shaded(
            h,
            mountainosity,
            felsity,
            koeppen,
            life,
            has_atm,
            optics,
        )
    }

    pub fn build_environment_proto(&self) -> tec11_proto::PlanetaryEnvironment {
        let (has_atmosphere, has_life) = match &self.stage {
            WorldStageState::Tectonics(tec) => (tec.has_atmosphere(), tec.has_life()),
            _ => (true, true),
        };
        tec11_proto::PlanetaryEnvironment {
            home_star: Some(self.home_star.to_proto()),
            star_filter: self.star_filter,
            atmosphere_id: self.atmosphere_id,
            has_atmosphere,
            has_life,
            moon_count: self.params.moon_count,
            moon_mass_lunar: self.params.moon_mass,
            current_moon_dist_radii: self.current_moon_dist_radii(),
            target_moon_dist_radii: self.params.moon_dist_radii,
            constellation_seed: self.constellation_seed,
            sys_rng_state_16b: self.sys_rng_bytes.to_vec(),
            is_sol_system: self.is_sol_system,
            target_day_hours: self.params.day_hours,
            target_tilt_deg: self.params.tilt_deg,
            land_pct: self.params.land_pct,
            atm_dt_myr: self.params.atm_dt_myr,
            life_dt_myr: self.params.life_dt_myr,
            people_dt_myr: self.params.people_dt_myr,
            meteor_freq: self.params.meteor_freq,
            galaxy_type: self.galaxy_type.id(),
            galactic_vantage: self.galactic_vantage.id(),
        }
    }

    pub fn encode_snapshot(&self) -> Vec<u8> {
        let _span = tec11_geology::profile::span(tec11_geology::profile::ProfilePhase::StageSnapshotEncode);
        let mut snap = match &self.stage {
            WorldStageState::Tectonics(tec) => tec.to_snapshot(),
            WorldStageState::Prehistory(pre) => pre.to_snapshot(),
            WorldStageState::History(hist) => hist.to_snapshot(),
        };
        snap.environment = Some(self.build_environment_proto());
        snap.encode_bytes()
    }

    pub fn load_snapshot_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(snap) = tec11_proto::PlanetSnapshot::decode_bytes(bytes) else {
            return false;
        };
        if let Some(ref env) = snap.environment {
            if snap.grid_size >= 2 {
                self.params.grid_size = snap.grid_size;
            }
            if env.target_day_hours > 0 {
                self.params.day_hours = env.target_day_hours;
            } else if snap.spin > 0.0 {
                self.params.day_hours = (24.0 / snap.spin).round().clamp(4.0, 240.0) as u32;
            }
            if env.target_tilt_deg <= 89 && (env.target_tilt_deg > 0 || snap.tilt_deg == 0.0) {
                self.params.tilt_deg = env.target_tilt_deg;
            } else if snap.tilt_deg >= 0.0 {
                self.params.tilt_deg = snap.tilt_deg.round().clamp(0.0, 89.0) as u32;
            }
            if env.land_pct > 0 {
                self.params.land_pct = env.land_pct;
            }
            if env.atm_dt_myr > 0 {
                self.params.atm_dt_myr = env.atm_dt_myr;
            }
            if env.life_dt_myr > 0 {
                self.params.life_dt_myr = env.life_dt_myr;
            }
            if env.people_dt_myr > 0 {
                self.params.people_dt_myr = env.people_dt_myr;
            }
            self.params.moon_count = env.moon_count;
            self.params.moon_mass = env.moon_mass_lunar;
            if env.target_moon_dist_radii >= 15 {
                self.params.moon_dist_radii = env.target_moon_dist_radii;
            }
            self.params.meteor_freq = env.meteor_freq.clamp(0.0, 10.0);

            let radius_km = if snap.radius_km > 0.0 {
                snap.radius_km
            } else {
                Self::planet_radius_km(self.params.grid_size)
            };
            let sky = tec11_cosmos::ReconstructedSkyEnvironment::from_snapshot(
                &snap,
                radius_km,
                self.params.land_pct,
            );
            self.galaxy_type = sky.galaxy_type;
            self.galactic_vantage = sky.galactic_vantage;
            self.home_star = sky.home_star;
            self.star_filter = sky.star_filter;
            self.atmosphere = sky.atmosphere;
            self.atmosphere_id = sky.atmosphere.id();
            self.optics = sky.optics;
            self.constellation_seed = sky.constellation_seed;
            self.sys_rng_bytes = sky.sys_rng_bytes;
            self.is_sol_system = sky.is_sol_system;
            self.solar_system = sky.solar_system;
            self.home_planet_index = sky.home_planet_index;
            self.constellations = sky.constellations;
        }
        match &snap.stage {
            Some(tec11_proto::PlanetStageOneof::Tectonics(_)) => {
                let Some(tec) = TectonicsSimulation::from_snapshot(&snap, 42) else {
                    return false;
                };
                self.tectonics_ticks = tec.geoticks();
                self.prehistory_ticks = 0;
                self.history_ticks = 0;
                self.stage = WorldStageState::Tectonics(tec);
                true
            }
            Some(tec11_proto::PlanetStageOneof::Prehistory(pre_proto)) => {
                let Some(pre) = PrehistorySimulation::from_snapshot(&snap, 42) else {
                    return false;
                };
                self.tectonics_ticks = pre_proto.tectonics_ticks;
                self.prehistory_ticks = pre_proto.prehistory_ticks;
                self.history_ticks = 0;
                self.stage = WorldStageState::Prehistory(pre);
                true
            }
            Some(tec11_proto::PlanetStageOneof::History(hist_proto)) => {
                let mut hist = HistorySimulation::new_earth(42);
                if !hist.load_snapshot_bytes(bytes) {
                    return false;
                }
                hist.set_solar_system(self.solar_system.clone());
                hist.set_sky_optics(self.home_star.clone(), self.optics.clone());
                if snap.environment.is_none() && !hist.constellations().is_empty() {
                    self.constellations = hist.constellations().to_vec();
                }
                if let Some(pre_base) = &hist_proto.prehistory_base {
                    self.tectonics_ticks = pre_base.tectonics_ticks;
                    self.prehistory_ticks = pre_base.prehistory_ticks;
                }
                self.history_ticks = hist_proto.history_ticks;
                self.stage = WorldStageState::History(hist);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_simulation_stages_and_snapshot() {
        let params = WorldCreateParams {
            grid_size: 4,
            day_hours: 24,
            tilt_deg: 23,
            land_pct: 29,
            atm_dt_myr: 5,
            life_dt_myr: 5,
            people_dt_myr: 5,
            moon_count: 1,
            moon_mass: 1.0,
            moon_dist_radii: 60,
            meteor_freq: 1.0,
            prehistory_glaciation_cycles: 0,
        };
        let mut world = WorldSimulation::new(params, 42);
        assert_eq!(world.stage_index(), 0);

        // Step through atmosphere, life, and people countdown to enter Prehistory
        for _ in 0..5 {
            world.step();
        }
        assert_eq!(world.stage_index(), 1);
        assert!(world.total_years() >= 15_000_000);

        // Advance into History stage
        world.advance_stage();
        assert_eq!(world.stage_index(), 2);
        world.step();
        assert!(world.total_population_thousands() > 0.0);

        let snap = world.encode_snapshot();
        let mut restored = WorldSimulation::new(WorldCreateParams::default(), 99);
        assert!(restored.load_snapshot_bytes(&snap));
        assert_eq!(restored.stage_index(), 2);
    }

    #[test]
    fn test_world_solar_system_substitution_and_history_planet_naming() {
        let params = WorldCreateParams {
            grid_size: 4,
            day_hours: 20,
            tilt_deg: 21,
            land_pct: 70, // Arid/desert world (>65% land)
            atm_dt_myr: 5,
            life_dt_myr: 5,
            people_dt_myr: 5,
            moon_count: 2,
            moon_mass: 1.0,
            moon_dist_radii: 60,
            meteor_freq: 1.0,
            prehistory_glaciation_cycles: 0,
        };
        let mut world = WorldSimulation::new(params, 77);
        world.set_sun_and_atmosphere(3, 0); // K-class orange dwarf
        assert!(world.solar_system().len() >= 4);
        let home_idx = world.home_planet_index();
        let home_planet = &world.solar_system()[home_idx];
        assert!(home_planet.is_home_world);
        assert_eq!(home_planet.moon_count, 2);
        assert!(!home_planet.has_rings);

        for _ in 0..5 {
            world.step();
        }
        world.advance_stage();
        assert_eq!(world.stage_index(), 2);

        if let WorldStageState::History(hist) = world.stage() {
            assert_eq!(hist.solar_system().len(), world.solar_system().len());
            assert!(hist.solar_system()[home_idx].is_home_world);
        }

        // Verify that randomized creation updates star_filter and atmosphere_id to concrete dropdown values:
        for _ in 0..8 {
            world.create(params, true);
            assert!((1..=5).contains(&world.star_filter()));
            assert_eq!(world.star_filter(), world.home_star().star_filter_id());
            assert!(world.atmosphere_id() <= 5);
            assert_eq!(world.atmosphere_id(), world.atmosphere().id());
        }
    }

    #[test]
    fn test_world_coarse_tile_sky_and_vantage_across_all_stages() {
        let params = WorldCreateParams {
            grid_size: 4,
            day_hours: 18,
            tilt_deg: 26,
            land_pct: 32,
            atm_dt_myr: 10,
            life_dt_myr: 5,
            people_dt_myr: 5,
            moon_count: 2,
            moon_mass: 1.0,
            moon_dist_radii: 60,
            meteor_freq: 0.5,
            prehistory_glaciation_cycles: 0,
        };
        let mut world = WorldSimulation::new(params, 123);
        assert_eq!(world.stage_index(), 0);

        // 1. Airless early Tectonics stage (tick 0, before atm_dt_myr):
        let tec_airless = world
            .inspect_tile_structured(0)
            .expect("Stage 0 airless coarse tile should inspect");
        assert!(!tec_airless.sky.has_atmosphere);
        assert_eq!(tec_airless.sky.moons.len(), 2);
        assert!(!tec_airless.sky.constellations.is_empty());
        assert!(tec_airless.vantage.obs_is_coarse);
        assert!(tec_airless.vantage.obs_is_land);
        assert!((5..=6).contains(&tec_airless.vantage.neighbors.len()));
        assert!(tec_airless.vantage.samples.is_empty());
        let stage0_constellations: Vec<(u32, f32, f32)> = tec_airless
            .sky
            .constellations
            .iter()
            .map(|c| (c.index, c.ra_deg, c.dec_deg))
            .collect();

        // 2. Step until atmosphere forms in Stage 0:
        world.step();
        world.step();
        let tec_atm = world
            .inspect_tile_structured(0)
            .expect("Stage 0 atmospheric coarse tile should inspect");
        assert!(tec_atm.sky.has_atmosphere);
        assert!(tec_atm.vantage.obs_is_coarse);
        assert!(tec_atm.vantage.samples.is_empty());

        // 3. Advance to Stage 1 (Prehistory — still un-subdivided coarse grid):
        world.advance_stage();
        assert_eq!(world.stage_index(), 1);
        let pre_view = world
            .inspect_tile_structured(0)
            .expect("Stage 1 Prehistory coarse tile should inspect");
        assert!(pre_view.sky.has_atmosphere);
        assert!(pre_view.vantage.obs_is_coarse);
        assert!((5..=6).contains(&pre_view.vantage.neighbors.len()));
        assert!(pre_view.vantage.samples.is_empty());

        // 4. Advance to Stage 2 (History — subdivided land + coarse ocean):
        world.advance_stage();
        assert_eq!(world.stage_index(), 2);
        let hist_view = world
            .inspect_tile_structured(0)
            .expect("Stage 2 History fine tile should inspect");
        assert!(hist_view.sky.has_atmosphere);
        assert!(!hist_view.vantage.obs_is_coarse);
        assert!(!hist_view.vantage.samples.is_empty());
        let stage2_constellations: Vec<(u32, f32, f32)> = hist_view
            .sky
            .constellations
            .iter()
            .map(|c| (c.index, c.ra_deg, c.dec_deg))
            .collect();
        assert_eq!(
            stage0_constellations, stage2_constellations,
            "World constellations should remain consistent from Stage 0 through Stage 2"
        );
    }

    #[test]
    fn test_world_and_weather_salient_flora_and_fauna_trophic_balance() {
        let params = WorldCreateParams {
            grid_size: 4,
            day_hours: 24,
            tilt_deg: 23,
            land_pct: 32,
            atm_dt_myr: 5,
            life_dt_myr: 5,
            people_dt_myr: 20,
            ..Default::default()
        };
        let mut world = WorldSimulation::new(params, 2026);
        for _ in 0..4 {
            world.step();
        }

        // 1. Stage 0 (Tectonics with active biosphere): habitable land tiles have settled flora & fauna
        let mut tec_flora_tiles = 0usize;
        for t in 0..world.tile_count() {
            if let Some(v) = world.inspect_tile_structured(t) {
                if !v.flora.is_empty() && !v.fauna.is_empty() {
                    tec_flora_tiles += 1;
                }
            }
        }
        assert!(
            tec_flora_tiles > 20,
            "Expected >20 biosphere Tectonics tiles with both flora and fauna, got {tec_flora_tiles}"
        );

        // 2. Stage 1 (Prehistory): both land and temperate ocean tiles have settled flora & fauna
        world.advance_stage();
        assert_eq!(world.stage_index(), 1);
        let mut pre_land_both = 0usize;
        let mut pre_sea_both = 0usize;
        for t in 0..world.tile_count() {
            if let Some(v) = world.inspect_tile_structured(t) {
                if !v.flora.is_empty() && !v.fauna.is_empty() {
                    if v.vantage.obs_is_land {
                        pre_land_both += 1;
                    } else {
                        pre_sea_both += 1;
                    }
                }
            }
        }
        assert!(
            pre_land_both > 20,
            "Expected >20 Prehistory land tiles with both flora & fauna, got {pre_land_both}"
        );
        assert!(
            pre_sea_both > 20,
            "Expected >20 Prehistory sea tiles with marine flora & fauna, got {pre_sea_both}"
        );

        // 3. Stage 2 (History): fine land faces & coarse ocean faces have BOTH flora & fauna (never 0 flora + 4 predators!)
        world.advance_stage();
        assert_eq!(world.stage_index(), 2);
        let mut hist_land_with_flora_and_fauna = 0usize;
        let mut hist_sea_with_flora_and_fauna = 0usize;
        for d_idx in 0..world.tile_count() {
            if let Some(v) = world.inspect_tile_structured(d_idx) {
                if !v.flora.is_empty() && !v.fauna.is_empty() {
                    if v.vantage.obs_is_land {
                        hist_land_with_flora_and_fauna += 1;
                    } else {
                        hist_sea_with_flora_and_fauna += 1;
                    }
                }
            }
        }
        assert!(
            hist_land_with_flora_and_fauna > 100,
            "Expected >100 History fine land tiles with both flora & fauna, got {hist_land_with_flora_and_fauna}"
        );
        assert!(
            hist_sea_with_flora_and_fauna > 20,
            "Expected >20 History sea tiles with marine flora & fauna, got {hist_sea_with_flora_and_fauna}"
        );
    }

    #[test]
    fn test_prehistory_water_tech_does_not_trigger_history_until_agriculture() {
        let params = WorldCreateParams {
            grid_size: 4,
            day_hours: 24,
            tilt_deg: 23,
            land_pct: 29,
            atm_dt_myr: 5,
            life_dt_myr: 5,
            people_dt_myr: 5,
            moon_count: 1,
            moon_mass: 1.0,
            moon_dist_radii: 60,
            meteor_freq: 1.0,
            prehistory_glaciation_cycles: 0,
        };
        let mut world = WorldSimulation::new(params, 42);
        while world.stage_index() == 0 {
            world.step();
        }
        assert_eq!(world.stage_index(), 1);

        // Build a Prehistory snapshot at tick 4 (`glaciation_tick = 32` -> `glaciation = 0.0` so `gfactor = 0.0`)
        // where all active heritages have BOTH rivergoing and seafaring, but ZERO agriculture:
        let mut snap = tec11_proto::PlanetSnapshot::decode_bytes(&world.encode_snapshot()).unwrap();
        if let Some(tec11_proto::PlanetStageOneof::Prehistory(ref mut pre_proto)) = snap.stage {
            pre_proto.prehistory_ticks = 4;
            pre_proto.glaciation_tick = 32;
            for h in &mut pre_proto.heritages {
                h.agricultural = false;
                h.rivergoing = true;
                h.seafaring = true;
            }
        }
        assert!(world.load_snapshot_bytes(&snap.encode_bytes()));
        assert_eq!(world.stage_index(), 1);

        // Stepping with active rivergoing & seafaring (and 0 agriculture)
        // must keep the world in Prehistory (`stage_index == 1`), not flip to History:
        world.step();
        assert_eq!(
            world.stage_index(),
            1,
            "Rivergoing and seafaring alone must not transition Prehistory into History"
        );
        if let WorldStageState::Prehistory(pre) = world.stage() {
            assert_eq!(pre.active_agricultural_race_count(), 0);
            assert!(pre.active_rivergoing_race_count() > 0);
            assert!(pre.active_seafaring_race_count() > 0);
        } else {
            panic!("Expected Prehistory stage");
        }

        // Once an active heritage develops agriculture, the next Prehistory step transitions
        // immediately to History (`stage_index == 2`) without waiting for 32 Prehistory ticks:
        let mut ag_snap =
            tec11_proto::PlanetSnapshot::decode_bytes(&world.encode_snapshot()).unwrap();
        if let Some(tec11_proto::PlanetStageOneof::Prehistory(ref mut pre_proto)) = ag_snap.stage {
            pre_proto.prehistory_ticks = 4;
            pre_proto.glaciation_tick = 32;
            for h in &mut pre_proto.heritages {
                h.agricultural = true;
            }
        }
        assert!(world.load_snapshot_bytes(&ag_snap.encode_bytes()));
        world.step();
        assert_eq!(
            world.stage_index(),
            2,
            "Developing agriculture must immediately transition Prehistory into History"
        );
    }

    #[test]
    fn test_world_four_globe_modes_color_climate_borders_population() {
        let mut p = WorldCreateParams::default();
        p.grid_size = 3;
        p.atm_dt_myr = 5;
        p.life_dt_myr = 5;
        p.people_dt_myr = 5;
        let mut world = WorldSimulation::new(p, 909);

        // Verify tick-0 pre-atmospheric Hadean world renders all four modes and regional maps without panicking:
        assert_eq!(world.stage_index(), 0);
        let n_tec = world.tile_count();
        for mode in ["color", "climate", "borders", "population"] {
            let c0 = world.tile_colors_rgb(mode, Some(0));
            assert_eq!(c0.len(), n_tec * 3);
            let r0 = world.regional_tile_map_json(0, mode);
            assert!(r0.contains("\"hasTile\":true"));
        }
        assert!(world.ocean_current_segments_colored().is_empty());

        // Verify higher elevations are shaded darker in natural_color_shaded:
        let low = WorldSimulation::natural_color_shaded(
            0.2,
            0.5,
            0.5,
            5,
            0.7,
            true,
            world.optics(),
        );
        let high = WorldSimulation::natural_color_shaded(
            4.2,
            10.0,
            0.5,
            5,
            0.7,
            true,
            world.optics(),
        );
        assert!(
            high[0] + high[1] + high[2] < low[0] + low[1] + low[2],
            "Mountain elevations should be shaded darker than lowland tiles in Color mode"
        );

        while world.stage_index() == 0 {
            world.step();
        }
        assert_eq!(world.stage_index(), 1);
        world.step();

        let n_pre = world.tile_count();
        for mode in ["color", "climate", "borders", "population"] {
            let c = world.tile_colors_rgb(mode, None);
            assert_eq!(c.len(), n_pre * 3);
        }
        let ocean_segs = world.ocean_current_segments_colored();
        assert_eq!(ocean_segs.len() % 9, 0);
        assert!(!ocean_segs.is_empty(), "Climate mode should emit colored 3D ocean current arrows");

        // Advance into History and ensure Color mode does NOT paint sovereign state palette colors:
        let mut snap = tec11_proto::PlanetSnapshot::decode_bytes(&world.encode_snapshot()).unwrap();
        if let Some(tec11_proto::PlanetStageOneof::Prehistory(ref mut pre_proto)) = snap.stage {
            pre_proto.prehistory_ticks = 32;
            pre_proto.glaciation_tick = 32;
            for h in &mut pre_proto.heritages {
                h.agricultural = true;
            }
        }
        assert!(world.load_snapshot_bytes(&snap.encode_bytes()));
        world.step();
        assert_eq!(world.stage_index(), 2);
        for _ in 0..8 {
            world.step();
        }
        let c_color = world.tile_colors_rgb("color", None);
        let c_borders = world.tile_colors_rgb("borders", None);
        let c_pop = world.tile_colors_rgb("population", None);
        let c_clim = world.tile_colors_rgb("climate", None);
        assert_eq!(c_color.len(), world.tile_count() * 3);
        assert_eq!(c_borders.len(), world.tile_count() * 3);
        assert_eq!(c_pop.len(), world.tile_count() * 3);
        assert_eq!(c_clim.len(), world.tile_count() * 3);
        assert_ne!(
            c_color, c_borders,
            "Color mode in History must show physical terrain rather than political Borders"
        );
    }

    #[test]
    fn test_world_cosmos_modal_data_and_star_map() {
        let mut p = WorldCreateParams::default();
        p.grid_size = 2;
        p.moon_count = 2;
        let mut world = WorldSimulation::new(p.clone(), 42);

        let gal = world.galaxy_cloud_data();
        assert_eq!(gal.len(), 1800 * 6);
        let stars = world.star_map_stars_data();
        assert!(stars.len() >= 5000 * 6, "Sol star map should include Hipparcos stars");
        let edges = world.star_map_constellation_edges();
        assert_eq!(edges.len() % 3, 0);
        assert!(!edges.is_empty());
        let meta = world.star_map_metadata_json();
        assert!(meta.contains("\"constellations\":["));
        let overview = world.cosmos_overview_json();
        assert!(overview.contains("\"isHomeWorld\":true"));
        assert!(overview.contains("\"resonanceRatio\":2"));

        // Also test randomized custom cosmos:
        world.create(p, true);
        let custom_stars = world.star_map_stars_data();
        assert!(!custom_stars.is_empty());
        assert_eq!(custom_stars.len() % 6, 0);
        let custom_overview = world.cosmos_overview_json();
        assert!(custom_overview.contains("\"planets\":["));
    }

    #[test]
    fn test_world_regional_tile_map_centered_and_mode_synced() {
        let mut p = WorldCreateParams::default();
        p.grid_size = 3;
        p.atm_dt_myr = 5;
        p.life_dt_myr = 5;
        p.people_dt_myr = 5;
        let mut world = WorldSimulation::new(p, 77);
        while world.stage_index() == 0 {
            world.step();
        }
        world.step();

        for mode in ["color", "climate", "borders", "population"] {
            let rj = world.regional_tile_map_json(10, mode);
            assert!(rj.contains("\"hasTile\":true"));
            assert!(rj.contains(&format!("\"aspect\":\"{mode}\"")));
            assert!(rj.contains("\"isSelected\":true"));
        }
    }

    #[test]
    fn test_world_polar_ocean_currents_persist_under_glacial_history() {
        let mut p = WorldCreateParams::default();
        p.grid_size = 3;
        p.atm_dt_myr = 5;
        p.life_dt_myr = 5;
        p.people_dt_myr = 5;
        let mut world = WorldSimulation::new(p, 42);
        while world.stage_index() == 0 {
            world.step();
        }
        assert_eq!(world.stage_index(), 1);

        // Transition at `glaciation_tick = 32` (glacial maximum: glaciation ≈ 0.0).
        let mut snap = tec11_proto::PlanetSnapshot::decode_bytes(&world.encode_snapshot()).unwrap();
        if let Some(tec11_proto::PlanetStageOneof::Prehistory(ref mut pre_proto)) = snap.stage {
            pre_proto.glaciation_tick = 32;
            for h in &mut pre_proto.heritages {
                h.agricultural = true;
            }
        }
        assert!(world.load_snapshot_bytes(&snap.encode_bytes()));
        world.step();
        assert_eq!(world.stage_index(), 2);

        if let WorldStageState::History(hist) = world.stage() {
            assert!(
                hist.prehistory().glaciation() < 0.05,
                "History should preserve Prehistory's glacial-maximum glaciation, got {}",
                hist.prehistory().glaciation()
            );
        } else {
            panic!("Expected History stage");
        }

        // Polar ocean currents (`|y| >= sin(55 deg) ≈ 0.82`) must still flow under glacial sea ice.
        let hist_polar = world
            .ocean_current_segments_colored()
            .chunks_exact(9)
            .filter(|c| c[1].abs() >= 0.82)
            .count();
        assert!(
            hist_polar > 20,
            "Expected visible polar ocean currents in History even at glacial maximum, got {hist_polar}"
        );
    }

    #[test]
    fn test_prehistory_starts_at_random_glaciation_phase() {
        let mut p = WorldCreateParams::default();
        p.grid_size = 2;
        p.atm_dt_myr = 5;
        p.life_dt_myr = 5;
        p.people_dt_myr = 5;
        let mut phases = std::collections::BTreeSet::new();
        for seed in [11u64, 29, 42, 77, 101] {
            let mut world = WorldSimulation::new(p.clone(), seed);
            while world.stage_index() == 0 {
                world.step();
            }
            if let WorldStageState::Prehistory(pre) = world.stage() {
                phases.insert(pre.glaciation_tick());
            } else {
                panic!("Expected Prehistory stage");
            }
        }
        assert!(
            phases.len() >= 3,
            "Expected distinct initial glaciation phases across seeds, got {:?}",
            phases
        );
    }

    #[test]
    fn test_calendar_summary_text() {
        let mut p = WorldCreateParams::default();
        p.grid_size = 2;
        p.day_hours = 24;
        p.moon_count = 1;
        p.moon_mass = 1.0;
        p.moon_dist_radii = 60;
        let world = WorldSimulation::new(p.clone(), 42);
        let text = world.calendar_summary_text();
        assert!(text.contains("Year:") && text.contains("days"), "got: {text}");
        assert!(text.contains("Month:") && text.contains("days"), "got: {text}");
        assert!(text.contains("Day:") && text.contains("hours"), "got: {text}");

        // Moonless world reports no moons
        let mut p0 = p.clone();
        p0.moon_count = 0;
        let world0 = WorldSimulation::new(p0, 42);
        let text0 = world0.calendar_summary_text();
        assert!(text0.contains("Month: None (no moons)"), "got: {text0}");

        // Multi-moon world lists multiple synodic months
        let mut p2 = p;
        p2.moon_count = 2;
        let world2 = WorldSimulation::new(p2, 42);
        let text2 = world2.calendar_summary_text();
        assert!(text2.contains("Months:") && text2.contains(" / "), "got: {text2}");
    }

    #[test]
    fn test_prehistory_glaciation_cycles_gates_agriculture_and_randomizes_around_3() {
        // 1. Random mode samples prehistory_glaciation_cycles normally around 3 (clamped 0..=8)
        let mut world = WorldSimulation::new(WorldCreateParams {
            grid_size: 2,
            ..Default::default()
        }, 777);
        let mut sum = 0.0f64;
        let samples = 60usize;
        let mut distinct = std::collections::BTreeSet::new();
        for _ in 0..samples {
            let p = world.create(
                WorldCreateParams {
                    grid_size: 2,
                    ..Default::default()
                },
                true,
            );
            assert!(p.prehistory_glaciation_cycles <= 8);
            sum += p.prehistory_glaciation_cycles as f64;
            distinct.insert(p.prehistory_glaciation_cycles);
        }
        let mean = sum / (samples as f64);
        assert!(
            (2.2..=3.8).contains(&mean),
            "Expected randomized prehistory_glaciation_cycles mean near 3.0, got {mean:.2}"
        );
        assert!(
            distinct.len() >= 3,
            "Expected variation in randomized prehistory_glaciation_cycles, got {:?}",
            distinct
        );

        // 2. Gating: with 1 required glaciation cycle (32 ticks), agriculture cannot spark before tick 32
        let mut pre = PrehistorySimulation::new_earth(42);
        pre.set_min_glaciation_cycles(1);
        assert_eq!(pre.min_ag_ticks(), 2 * tec11_prehistory::prehistory::PREHISTORY_GLACIATION_STEP);
        for _ in 0..10 {
            pre.step();
            assert_eq!(
                pre.active_agricultural_race_count(),
                0,
                "Agriculture must not spark before min_ag_ticks has elapsed (tick={})",
                pre.prehistory_ticks()
            );
        }
    }
}
