use std::f64::consts::PI;

use tec11_cosmos::*;
use tec11_math_grid::{PointTree, SphericalGrid};
use tec11_rng::Pcg64Mcg;
use tec11_vec3::{cross, dot, length, normal, rotate, Vec3};

/// Interactive Sun, Plausible Galaxy & Celestial Sphere Star Map model (`SunSandbox`).
pub struct SunSandbox {
    grid: SphericalGrid,
    tree: PointTree,
    galaxy_type: GalaxyType,
    vantage: GalacticVantage,
    star_class_filter: u32,
    atmosphere: AtmosphereType,
    optics: PlanetaryOptics,
    is_real_hipparcos: bool,
    home_star: HomeStar,
    planets: Vec<SolarSystemPlanet>,
    home_planet_index: usize,
    stars: Vec<CelestialStar>,
    /// Unit normal of the galactic plane in the planet's sky coordinates.
    galactic_pole: Vec3,
    /// Unit direction toward the galactic center in the planet's sky coordinates.
    galactic_center_dir: Vec3,
    /// Unit normal of the planet's ecliptic plane (where the Home Sun travels).
    ecliptic_pole: Vec3,
    /// Direction of the Home Sun along the ecliptic.
    sun_dir: Vec3,
    /// Pairs of star indices forming constellation / asterism lines among bright stars.
    constellation_edges: Vec<(usize, usize)>,
    /// Extracted constellations across the celestial sphere with declinations for latitude visibility.
    constellations: Vec<Constellation>,
    /// 3D galaxy preview particles `(x, y, z, r, g, b)` normalized to `[-0.95, 0.95]` for galaxy view.
    galaxy_particles: Vec<[f32; 6]>,
    rng: Pcg64Mcg,
}

impl SunSandbox {
    pub fn new(seed: u64) -> Self {
        let grid = SphericalGrid::new();
        let indexed_pts: Vec<(Vec3, usize)> = grid
            .faces
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c, i))
            .collect();
        let tree = PointTree::new(&indexed_pts);
        let sol = HomeStar::sol();
        let atmosphere = AtmosphereType::EarthLikeN2O2;
        let optics = PlanetaryOptics::compute(&sol, atmosphere);
        let mut sb = Self {
            grid,
            tree,
            galaxy_type: GalaxyType::Spiral,
            vantage: GalacticVantage::SpiralArm,
            star_class_filter: 0,
            atmosphere,
            optics,
            is_real_hipparcos: false,
            home_star: sol,
            planets: Vec::new(),
            home_planet_index: 0,
            stars: Vec::with_capacity(6000),
            galactic_pole: [0.0, 0.0, 1.0],
            galactic_center_dir: [1.0, 0.0, 0.0],
            ecliptic_pole: [0.0, 0.39, 0.92],
            sun_dir: [1.0, 0.0, 0.0],
            constellation_edges: Vec::new(),
            constellations: Vec::new(),
            galaxy_particles: Vec::new(),
            rng: Pcg64Mcg::from_seed_u64(seed),
        };
        sb.generate_galaxy_and_sun(99, 99, 0);
        sb
    }

    pub fn grid(&self) -> &SphericalGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.grid.faces.len()
    }

    pub fn galaxy_type(&self) -> GalaxyType {
        self.galaxy_type
    }

    pub fn vantage(&self) -> GalacticVantage {
        self.vantage
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

    pub fn planets(&self) -> &[SolarSystemPlanet] {
        &self.planets
    }

    pub fn planet_count(&self) -> usize {
        self.planets.len()
    }

    pub fn home_planet_index(&self) -> usize {
        self.home_planet_index
    }

    pub fn planets_summary_text(&self) -> String {
        format_solar_system_summary(&self.planets)
    }

    pub fn constellations(&self) -> &[Constellation] {
        &self.constellations
    }

    /// Update the planet's atmospheric gas composition (`0..=5` explicit, or `99` for random)
    /// and recompute sky, sunset, and photosynthetic plant colors.
    pub fn set_atmosphere(&mut self, atmosphere_id: u32) {
        self.atmosphere = sample_atmosphere_with_rng(&mut self.rng, atmosphere_id);
        self.optics = PlanetaryOptics::compute(&self.home_star, self.atmosphere);
    }

    pub fn stars(&self) -> &[CelestialStar] {
        &self.stars
    }

    pub fn star_count(&self) -> usize {
        self.stars.len()
    }

    pub fn is_real_hipparcos(&self) -> bool {
        self.is_real_hipparcos
    }

    /// Generate a new plausible galaxy (`galaxy_type_id`: `0..=5`, or `99` for random),
    /// select a vantage (`vantage_id`: `0..=4`, or `99` for auto/weighted),
    /// choose a Home Star (`star_filter`: `0` = Auto Habitable, `1` = F, `2` = G, `3` = K, `4` = M, `5` = A, `6` = Subgiant/Giant),
    /// and synthesize the 6,000-star night sky around it.
    pub fn generate_galaxy_and_sun(
        &mut self,
        galaxy_type_id: u32,
        vantage_id: u32,
        star_filter: u32,
    ) {
        self.is_real_hipparcos = false;
        self.star_class_filter = star_filter;

        self.galaxy_type = if galaxy_type_id <= 5 {
            GalaxyType::from_id(galaxy_type_id)
        } else {
            // Plausible galaxy morphological distribution:
            let choices = [
                GalaxyType::Spiral,
                GalaxyType::Spiral,
                GalaxyType::Spiral,
                GalaxyType::BarredSpiral,
                GalaxyType::BarredSpiral,
                GalaxyType::Lenticular,
                GalaxyType::Elliptical,
                GalaxyType::Elliptical,
                GalaxyType::Irregular,
                GalaxyType::GlobularCluster,
            ];
            choices[(self.rng.next_u64() as usize) % choices.len()]
        };

        self.vantage = if vantage_id <= 4 {
            GalacticVantage::from_id(vantage_id)
        } else {
            match self.galaxy_type {
                GalaxyType::Spiral | GalaxyType::BarredSpiral => {
                    let v_choices = [
                        GalacticVantage::SpiralArm,
                        GalacticVantage::SpiralArm,
                        GalacticVantage::InterArmDisk,
                        GalacticVantage::GalacticBulge,
                        GalacticVantage::GalacticHalo,
                    ];
                    v_choices[(self.rng.next_u64() as usize) % v_choices.len()]
                }
                GalaxyType::Lenticular => {
                    let v_choices = [
                        GalacticVantage::InterArmDisk,
                        GalacticVantage::InterArmDisk,
                        GalacticVantage::GalacticBulge,
                        GalacticVantage::GalacticHalo,
                    ];
                    v_choices[(self.rng.next_u64() as usize) % v_choices.len()]
                }
                GalaxyType::Elliptical | GalaxyType::GlobularCluster => {
                    if self.rng.next_f64() < 0.72 {
                        GalacticVantage::SpheroidCore
                    } else {
                        GalacticVantage::GalacticHalo
                    }
                }
                GalaxyType::Irregular => GalacticVantage::SpiralArm,
            }
        };

        self.choose_sun_in_current_galaxy(star_filter);
    }

    /// Choose a new Home Star ("Sun") within the current galaxy and rebuild its surrounding 6,000-star sky.
    pub fn choose_sun_in_current_galaxy(&mut self, star_filter: u32) {
        self.is_real_hipparcos = false;
        self.star_class_filter = star_filter;

        // 1. Sample 3D position `(x, y, z)` in kpc of the Home Star within the galaxy:
        let gal_pos_kpc = self.sample_home_star_galactic_position();

        // 2. Choose the Home Star's spectral class, color (B-V), mass, radius, and habitable zone:
        self.home_star = self.sample_home_star_properties(gal_pos_kpc, star_filter);
        self.optics = PlanetaryOptics::compute(&self.home_star, self.atmosphere);

        // 3. Build 3D galaxy visualization particle cloud:
        self.build_galaxy_particles();

        // 4. Generate 6,000 surrounding naked-eye stars in the planet's sky:
        self.generate_surrounding_starmap(6000);
        self.build_constellation_edges();

        // 5. Generate the planetary system around the Home Star and substitute the simulated home world:
        self.planets = generate_solar_system_with_rng(
            &mut self.rng,
            &self.home_star,
            self.ecliptic_pole,
            false,
        );
        self.home_planet_index = substitute_simulated_planet(
            &mut self.planets,
            &self.home_star,
            6400.0,
            29,
            1,
            self.ecliptic_pole,
        );
    }

    /// Load the real ESA Hipparcos naked-eye star catalogue (`5,179` stars around Sol, `starmap.py r`).
    pub fn load_real_hipparcos_sol(&mut self) {
        self.is_real_hipparcos = true;
        self.galaxy_type = GalaxyType::Spiral;
        self.vantage = GalacticVantage::SpiralArm;
        self.home_star = HomeStar::sol();
        self.atmosphere = AtmosphereType::EarthLikeN2O2;
        self.optics = PlanetaryOptics::compute(&self.home_star, self.atmosphere);
        self.galactic_pole = normal([-0.867, -0.198, 0.456]);
        self.galactic_center_dir = normal([-0.055, -0.873, -0.484]);
        self.ecliptic_pole = normal([0.0, -0.398, 0.917]);
        self.sun_dir = [1.0, 0.0, 0.0];

        self.stars.clear();
        for line in HIPPARCOS_TSV.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 7 {
                continue;
            }
            let id: u32 = parts[0].parse().unwrap_or(0);
            let alpha_deg: f64 = parts[1].parse().unwrap_or(0.0);
            let delta_deg: f64 = parts[2].parse().unwrap_or(0.0);
            let pi_arcsec: f64 = parts[3].parse().unwrap_or(0.001);
            let vmag: f32 = parts[4].parse().unwrap_or(5.0);
            let bv: f32 = parts[5].parse().unwrap_or(0.5);
            let offset: f32 = parts[6].parse().unwrap_or(0.5);

            // Standard J2000 equatorial unit vector (`+Z` = North Celestial Pole `+90°`, `-Z` = South Celestial Pole `-90°`),
            // matching `parsed_hipparcos_equatorial_stars` and `self.galactic_pole` / `self.ecliptic_pole`:
            let r_ly = (3.26 / pi_arcsec.max(1e-5)) as f32;
            let ra_rad = alpha_deg * PI / 180.0;
            let dec_rad = delta_deg * PI / 180.0;
            let cos_d = dec_rad.cos();
            let dir = normal([cos_d * ra_rad.cos(), cos_d * ra_rad.sin(), dec_rad.sin()]);
            let abs_mag = vmag + 5.0 * ((pi_arcsec.max(1e-5).log10() as f32) + 1.0);
            let base_rgb = bv_to_rgb(bv);
            let scale = (1.0 - (vmag + 40.0) / 45.0).clamp(0.08, 1.0);
            let mag_boost = ((6.5 - vmag.clamp(-1.5, 6.2)) / 7.5).clamp(0.18, 1.0);
            let final_scale = (scale * (0.35 + 0.65 * mag_boost)).clamp(0.12, 1.0);

            self.stars.push(CelestialStar {
                id,
                dir,
                distance_ly: r_ly,
                bv_color: bv,
                abs_mag,
                app_mag: vmag,
                galactic_offset_rad: offset,
                rgb: [
                    base_rgb[0] * final_scale,
                    base_rgb[1] * final_scale,
                    base_rgb[2] * final_scale,
                ],
                base_rgb,
            });
        }
        self.build_galaxy_particles();
        self.build_constellation_edges();
        self.planets = generate_solar_system_with_rng(
            &mut self.rng,
            &self.home_star,
            self.ecliptic_pole,
            true,
        );
        self.home_planet_index = substitute_simulated_planet(
            &mut self.planets,
            &self.home_star,
            6400.0,
            29,
            1,
            self.ecliptic_pole,
        );
    }

    fn sample_home_star_galactic_position(&mut self) -> [f32; 3] {
        match (self.galaxy_type, self.vantage) {
            (GalaxyType::Spiral | GalaxyType::BarredSpiral, GalacticVantage::SpiralArm) => {
                let r_kpc = self.rng.uniform(5.5, 10.5);
                let arm_idx = (self.rng.next_u64() % 2) as f64;
                let pitch = 0.26; // ~15 deg pitch angle
                let base_angle = arm_idx * PI + (r_kpc / 2.2).ln() / pitch;
                let phi = base_angle + self.rng.gauss(0.0, 0.14);
                let z = self.rng.gauss(0.0, 0.06);
                [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
            }
            (_, GalacticVantage::InterArmDisk) => {
                let r_kpc = self.rng.uniform(4.5, 11.5);
                let phi = self.rng.uniform(0.0, 2.0 * PI);
                let z = self.rng.gauss(0.0, 0.22);
                [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
            }
            (_, GalacticVantage::GalacticBulge) => {
                let r_kpc = self.rng.uniform(0.5, 2.1);
                let phi = self.rng.uniform(0.0, 2.0 * PI);
                let z = self.rng.gauss(0.0, 0.45);
                [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
            }
            (_, GalacticVantage::GalacticHalo) => {
                let r_xy = self.rng.uniform(4.0, 12.0);
                let phi = self.rng.uniform(0.0, 2.0 * PI);
                let z_sign = if self.rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
                let z = z_sign * self.rng.uniform(3.2, 7.5);
                [(r_xy * phi.cos()) as f32, (r_xy * phi.sin()) as f32, z as f32]
            }
            (GalaxyType::Irregular, _) => {
                let r_kpc = self.rng.uniform(1.0, 5.5);
                let phi = self.rng.uniform(0.0, 2.0 * PI);
                let z = self.rng.gauss(0.0, 0.65);
                [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
            }
            _ => {
                // Elliptical spheroid or Globular Cluster interior:
                let r_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
                    self.rng.uniform(0.002, 0.012)
                } else {
                    self.rng.uniform(2.0, 8.5)
                };
                let u = self.rng.uniform(-1.0, 1.0);
                let phi = self.rng.uniform(0.0, 2.0 * PI);
                let s = (1.0 - u * u).sqrt();
                [
                    (r_kpc * s * phi.cos()) as f32,
                    (r_kpc * 0.78 * s * phi.sin()) as f32,
                    (r_kpc * 0.62 * u) as f32,
                ]
            }
        }
    }

    fn sample_home_star_properties(&mut self, gal_pos_kpc: [f32; 3], star_filter: u32) -> HomeStar {
        let is_old_pop = matches!(
            self.galaxy_type,
            GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
        ) || self.vantage == GalacticVantage::GalacticBulge;
        sample_home_star_with_rng(&mut self.rng, gal_pos_kpc, star_filter, is_old_pop)
    }

    /// Generate `count` naked-eye visible stars (`m_app <= 6.0`) around `self.home_star`,
    /// generalizing `RandomStars` (`randomstars.py`) to arbitrary galaxy morphologies and vantages.
    fn generate_surrounding_starmap(&mut self, count: usize) {
        let hist_pgm = parse_hist_pgm();
        let (w, h, hist) = (hist_pgm.width, hist_pgm.height, hist_pgm.pixels);
        self.stars.clear();
        self.stars.reserve(count);

        // Random 3D rotation of the galactic frame relative to the planet's celestial pole (`randomstars.py:16-21`):
        let z_rot = self.rng.uniform(-1.0, 1.0);
        let theta_rot = self.rng.uniform(0.0, 2.0 * PI);
        let r_rot = (1.0 - z_rot * z_rot).sqrt();
        let u_axis = [r_rot * theta_rot.cos(), r_rot * theta_rot.sin(), z_rot];
        let rot_angle = self.rng.uniform(0.0, 2.0 * PI);

        self.galactic_pole = normal(rotate([0.0, 0.0, 1.0], u_axis, rot_angle));
        self.galactic_center_dir = normal(rotate([1.0, 0.0, 0.0], u_axis, rot_angle));

        // Ecliptic pole tilted by ~23 deg from celestial pole:
        let ecl_phi = self.rng.uniform(0.0, 2.0 * PI);
        let ecl_tilt = 23.4 * PI / 180.0;
        self.ecliptic_pole = normal([
            ecl_tilt.sin() * ecl_phi.cos(),
            ecl_tilt.sin() * ecl_phi.sin(),
            ecl_tilt.cos(),
        ]);
        let sun_lon = self.rng.uniform(0.0, 2.0 * PI);
        let e1 = normal(cross(self.ecliptic_pole, [0.0, 0.0, 1.0]));
        let e1 = if length(e1) < 1e-4 { [1.0, 0.0, 0.0] } else { e1 };
        let e2 = normal(cross(self.ecliptic_pole, e1));
        self.sun_dir = normal([
            e1[0] * sun_lon.cos() + e2[0] * sun_lon.sin(),
            e1[1] * sun_lon.cos() + e2[1] * sun_lon.sin(),
            e1[2] * sun_lon.cos() + e2[2] * sun_lon.sin(),
        ]);

        // For Irregular galaxies, generate 6 star-forming OB association knots in galactic coordinates:
        let mut irr_knots: Vec<Vec3> = Vec::new();
        if self.galaxy_type == GalaxyType::Irregular {
            for _ in 0..6 {
                let kz = self.rng.uniform(-0.65, 0.65);
                let kphi = self.rng.uniform(0.0, 2.0 * PI);
                let kr = (1.0 - kz * kz).sqrt();
                irr_knots.push([kr * kphi.cos(), kr * kphi.sin(), kz]);
            }
        }

        let is_old_stellar_pop = matches!(
            self.galaxy_type,
            GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
        ) || self.vantage == GalacticVantage::GalacticBulge;
        let is_young_starburst = self.galaxy_type == GalaxyType::Irregular
            || (matches!(self.galaxy_type, GalaxyType::Spiral | GalaxyType::BarredSpiral)
                && self.vantage == GalacticVantage::SpiralArm);

        for id in 1..=(count as u32) {
            // 1. Sample (B-V, M_V) from smoothed Hipparcos HR distribution (`randomstars.py:29-35`),
            //    with the blog post's `254.9` off-white fix and galaxy population age weighting:
            let (c, m, x_idx, y_idx) = loop {
                let c = self.rng.uniform(-0.274, 3.271);
                let m = self.rng.uniform(-15.329, 7.49);

                let x = (((c + 0.274) * (w as f64) / (3.271 + 0.274)) as usize).min(w - 1);
                let y = (((m + 15.329) * (h as f64) / (7.49 + 15.329)) as usize).min(h - 1);
                let gray = hist[y * w + x];
                if gray == 255 {
                    continue;
                }
                // Blog post fix: "that lowest-probability shade in my smootherized histogram is too dark:
                // it should probably be like gray 254.9 or something."
                let mut base_prob = if gray == 254 {
                    0.06 / 255.0
                } else if gray == 253 {
                    0.14 / 255.0
                } else {
                    (255.0 - (gray as f64)) / 255.0
                };

                // Modulate HR diagram by the galaxy's stellar population age:
                if is_old_stellar_pop {
                    // Older populations have evolved off the upper blue main sequence (c < 0.05, m < -1.5)
                    // and have a richer red/orange giant branch (c > 0.85, m < 2.0):
                    if c < 0.05 && m < -1.5 {
                        base_prob *= 0.12;
                    } else if c > 0.80 && m < 2.2 {
                        base_prob *= 1.55;
                    }
                } else if is_young_starburst && c < 0.22 && m < 1.5 {
                    base_prob *= 1.35;
                }

                if self.rng.next_f64() < base_prob {
                    break (c as f32, m as f32, x, y);
                }
            };

            // 2. Sample distance within naked-eye visibility limit (`randomstars.py:37`),
            //    using thin-disk/volume weighting (`u^(1/2.4)`) so apparent magnitudes match Hipparcos:
            let max_r_ly = (3.26156 / 10.0_f64.powf(((m as f64) - 6.0) / 5.0 - 1.0)).max(1.2);
            let dist_scale = if self.galaxy_type == GalaxyType::GlobularCluster
                || self.vantage == GalacticVantage::GalacticBulge
            {
                // Dense bulge / globular cluster environments pack stars closer together, brightening the night sky:
                0.76
            } else {
                1.0
            };
            let u_vol = self.rng.uniform(0.0005, 1.0).powf(1.0 / 2.4);
            let r_ly = ((1.0 + (max_r_ly - 1.0) * u_vol) * dist_scale).max(1.0) as f32;

            // 3. Sample 3D celestial coordinates `(theta, phi)` in the galactic frame, generalized across
            //    galaxy morphologies and vantages (`randomstars.py:38-42` + non-spiral/halo generalizations):
            let bright_white_factor =
                1.0 - (((x_idx * y_idx) as f64) / ((w * h) as f64)).sqrt();

            let (v_gal, offset_rad) = match (self.galaxy_type, self.vantage) {
                // Case A: Inside a Disk Galaxy (Spiral Arm, Barred Spiral, Inter-Arm Disk, or Lenticular Disk)
                (
                    GalaxyType::Spiral | GalaxyType::BarredSpiral | GalaxyType::Lenticular,
                    GalacticVantage::SpiralArm | GalacticVantage::InterArmDisk,
                ) => {
                    let sigma_z = if self.vantage == GalacticVantage::SpiralArm {
                        0.115
                    } else if self.galaxy_type == GalaxyType::Lenticular {
                        0.19
                    } else {
                        0.15
                    };
                    let theta = if self.rng.next_f64() < bright_white_factor {
                        let cos_th = self.rng.gauss(0.0, sigma_z).clamp(-1.0, 1.0);
                        cos_th.acos()
                    } else {
                        self.rng.uniform(-1.0, 1.0).acos()
                    };
                    // Azimuthal concentration toward Galactic Center (phi = 0) and spiral arm tangents:
                    let phi = if self.rng.next_f64() < 0.28 * bright_white_factor {
                        self.rng.gauss(0.0, 0.85)
                    } else {
                        self.rng.uniform(0.0, 2.0 * PI)
                    };
                    let sin_th = theta.sin();
                    let v = [sin_th * phi.cos(), sin_th * phi.sin(), theta.cos()];
                    (v, ((PI / 2.0) - theta).abs() as f32)
                }

                // Case B: High Above the Galactic Plane in the Stellar Halo
                //         (The galaxy appears in one hemisphere `z < 0`, while `z > 0` looks out into intergalactic space!)
                (_, GalacticVantage::GalacticHalo) => {
                    let (theta, phi) = if self.rng.next_f64() < 0.78 {
                        // Looking down/inward toward the host galaxy in the southern galactic hemisphere:
                        let cos_th = self.rng.uniform(-0.88, -0.04);
                        let phi = self.rng.gauss(0.0, 0.95);
                        (cos_th.acos(), phi)
                    } else {
                        // Sparse foreground halo stars across the whole sky:
                        (
                            self.rng.uniform(-1.0, 1.0).acos(),
                            self.rng.uniform(0.0, 2.0 * PI),
                        )
                    };
                    let sin_th = theta.sin();
                    let v = [sin_th * phi.cos(), sin_th * phi.sin(), theta.cos()];
                    (v, ((PI / 2.0) - theta).abs() as f32)
                }

                // Case C: Irregular Starburst Galaxy (Clumpy OB associations & Magellanic clouds)
                (GalaxyType::Irregular, _) => {
                    let v = if !irr_knots.is_empty()
                        && self.rng.next_f64() < 0.65 * bright_white_factor.max(0.45)
                    {
                        let knot = irr_knots[(self.rng.next_u64() as usize) % irr_knots.len()];
                        normal([
                            knot[0] + self.rng.gauss(0.0, 0.22),
                            knot[1] + self.rng.gauss(0.0, 0.22),
                            knot[2] + self.rng.gauss(0.0, 0.18),
                        ])
                    } else {
                        let z = self.rng.uniform(-1.0, 1.0);
                        let phi = self.rng.uniform(0.0, 2.0 * PI);
                        let r = (1.0 - z * z).sqrt();
                        [r * phi.cos(), r * phi.sin(), z]
                    };
                    let off = (PI / 2.0 - v[2].clamp(-1.0, 1.0).acos()).abs() as f32;
                    (v, off)
                }

                // Case D: Elliptical Galaxy, Galactic Bulge, or Globular Cluster
                //         (Triaxial spheroidal sky with smooth hemispherical concentration toward the core `[1, 0, 0]`)
                _ => {
                    let core_bias = if self.galaxy_type == GalaxyType::GlobularCluster {
                        0.45
                    } else {
                        0.62
                    };
                    let v = if self.rng.next_f64() < core_bias {
                        // Concentrated toward the galactic/cluster center `[1, 0, 0]` with triaxial flattening:
                        normal([
                            0.42 + self.rng.gauss(0.0, 0.55),
                            self.rng.gauss(0.0, 0.52),
                            self.rng.gauss(0.0, 0.38),
                        ])
                    } else {
                        let z = self.rng.uniform(-1.0, 1.0);
                        let phi = self.rng.uniform(0.0, 2.0 * PI);
                        let r = (1.0 - z * z).sqrt();
                        [r * phi.cos(), r * phi.sin(), z]
                    };
                    let off = (PI / 2.0 - v[2].clamp(-1.0, 1.0).acos()).abs() as f32;
                    (v, off)
                }
            };

            let dir = normal(rotate(v_gal, u_axis, rot_angle));

            // Apparent magnitude and RGB scaling (`starmap.py:452-456`):
            // Distance modulus: m_app = M_abs + 5 * (log10(d_pc) - 1.0), where d_pc = r_ly / 3.26156:
            let app_mag = (m + 5.0 * (((r_ly as f64) / 3.26156).log10() as f32 - 1.0)).clamp(-1.6, 6.2);
            let base_rgb = bv_to_rgb(c);
            let scale = (1.0 - (app_mag + 40.0) / 45.0).clamp(0.08, 1.0);
            let mag_boost = ((6.5 - app_mag.clamp(-1.5, 6.2)) / 7.5).clamp(0.18, 1.0);
            let final_scale = (scale * (0.35 + 0.65 * mag_boost)).clamp(0.12, 1.0);

            self.stars.push(CelestialStar {
                id,
                dir,
                distance_ly: r_ly,
                bv_color: c,
                abs_mag: m,
                app_mag,
                galactic_offset_rad: offset_rad,
                rgb: [
                    base_rgb[0] * final_scale,
                    base_rgb[1] * final_scale,
                    base_rgb[2] * final_scale,
                ],
                base_rgb,
            });
        }
    }

    /// Build automatic constellation / asterism lines connecting neighboring bright stars (`app_mag <= 3.55`)
    /// and extract up to `24` connected `Constellation` groups across both Northern and Southern hemispheres.
    fn build_constellation_edges(&mut self) {
        let (edges, constellations) = build_constellations_from_stars_with_limit(&self.stars, 24);
        self.constellation_edges = edges;
        self.constellations = constellations;
    }

    /// Project a 3D galactocentric / ecliptic coordinate `(gx, gy, gz)` with a `38°` inclination tilt
    /// so that the 3D galaxy structure and planetary orbits are clearly visible from the front (`+Z`) camera.
    fn tilt_3d_plane(gx: f32, gy: f32, gz: f32) -> [f32; 3] {
        // 38 deg inclination around X axis:
        let sin_t = 0.6156615f32;
        let cos_t = 0.7880108f32;
        [gx, gz * cos_t + gy * sin_t, gy * cos_t - gz * sin_t]
    }

    /// Build a 3D galaxy point cloud (`1,800` stars) in `[-0.90, 0.90]^3` for the `3D Galaxy & Host Star` view.
    fn build_galaxy_particles(&mut self) {
        self.galaxy_particles.clear();
        let count = 1800;
        self.galaxy_particles.reserve(count);
        let max_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
            0.020
        } else {
            15.0
        };

        for i in 0..count {
            let (pos_kpc, bv) = match self.galaxy_type {
                GalaxyType::Spiral | GalaxyType::BarredSpiral => {
                    if i < 420 {
                        // Central bulge or stellar bar (warm K/M/G stars):
                        let r = self.rng.uniform(0.1, 2.4);
                        let phi = self.rng.uniform(0.0, 2.0 * PI);
                        let z = self.rng.gauss(0.0, 0.55);
                        let (x, y) = if self.galaxy_type == GalaxyType::BarredSpiral && i < 280 {
                            (
                                self.rng.gauss(0.0, 1.85).clamp(-3.6, 3.6),
                                self.rng.gauss(0.0, 0.55),
                            )
                        } else {
                            (r * phi.cos(), r * phi.sin())
                        };
                        ([x, y, z], self.rng.uniform(0.75, 1.45) as f32)
                    } else if i < 1450 {
                        // Logarithmic spiral arms (rich in blue-white O/B/A and F/G stars):
                        let r = self.rng.uniform(2.0, 13.5);
                        let arm = (i % 2) as f64;
                        let pitch = 0.26;
                        let angle = arm * PI + (r / 2.2).ln() / pitch + self.rng.gauss(0.0, 0.22);
                        let z = self.rng.gauss(0.0, 0.16);
                        let bv = if i % 3 == 0 {
                            self.rng.uniform(-0.22, 0.25) as f32
                        } else {
                            self.rng.uniform(0.30, 1.10) as f32
                        };
                        ([r * angle.cos(), r * angle.sin(), z], bv)
                    } else {
                        // Exponential disk + halo:
                        let r = self.rng.uniform(1.5, 14.0);
                        let phi = self.rng.uniform(0.0, 2.0 * PI);
                        let z = self.rng.gauss(0.0, 0.65);
                        ([r * phi.cos(), r * phi.sin(), z], self.rng.uniform(0.55, 1.35) as f32)
                    }
                }
                GalaxyType::Lenticular => {
                    let r = if i < 600 {
                        self.rng.uniform(0.1, 2.8)
                    } else {
                        self.rng.uniform(1.5, 13.5)
                    };
                    let phi = self.rng.uniform(0.0, 2.0 * PI);
                    let z = self.rng.gauss(0.0, if i < 600 { 0.75 } else { 0.28 });
                    ([r * phi.cos(), r * phi.sin(), z], self.rng.uniform(0.62, 1.45) as f32)
                }
                GalaxyType::Elliptical => {
                    let r = self.rng.uniform(0.2, 12.5) * self.rng.uniform(0.25, 1.0);
                    let u = self.rng.uniform(-1.0, 1.0);
                    let phi = self.rng.uniform(0.0, 2.0 * PI);
                    let s = (1.0 - u * u).sqrt();
                    (
                        [r * s * phi.cos(), r * 0.76 * s * phi.sin(), r * 0.58 * u],
                        self.rng.uniform(0.75, 1.55) as f32,
                    )
                }
                GalaxyType::Irregular => {
                    let knot_idx = i % 6;
                    let ka = (knot_idx as f64) * (2.0 * PI / 6.0) + 0.4;
                    let kr = 2.5 + (knot_idx as f64) * 1.1;
                    let x = kr * ka.cos() + self.rng.gauss(0.0, 1.6);
                    let y = kr * ka.sin() + self.rng.gauss(0.0, 1.6);
                    let z = self.rng.gauss(0.0, 1.1);
                    let bv = if i % 2 == 0 {
                        self.rng.uniform(-0.25, 0.20) as f32
                    } else {
                        self.rng.uniform(0.35, 1.25) as f32
                    };
                    ([x, y, z], bv)
                }
                GalaxyType::GlobularCluster => {
                    let r = self.rng.uniform(0.0005, 0.016) * self.rng.uniform(0.2, 1.0);
                    let u = self.rng.uniform(-1.0, 1.0);
                    let phi = self.rng.uniform(0.0, 2.0 * PI);
                    let s = (1.0 - u * u).sqrt();
                    let bv = if i % 7 == 0 {
                        self.rng.uniform(-0.10, 0.22) as f32 // Blue horizontal branch
                    } else {
                        self.rng.uniform(0.70, 1.55) as f32 // Red giants & main sequence
                    };
                    ([r * s * phi.cos(), r * s * phi.sin(), r * u], bv)
                }
            };

            let rgb = bv_to_rgb(bv);
            let scale = (0.85 / max_kpc) as f32;
            let tilted = Self::tilt_3d_plane(
                (pos_kpc[0] as f32 * scale).clamp(-0.92, 0.92),
                (pos_kpc[1] as f32 * scale).clamp(-0.92, 0.92),
                (pos_kpc[2] as f32 * scale).clamp(-0.92, 0.92),
            );
            self.galaxy_particles.push([
                tilted[0],
                tilted[1],
                tilted[2],
                rgb[0],
                rgb[1],
                rgb[2],
            ]);
        }
    }

    /// Flat buffer `[x, y, z, r, g, b, size_px, ...]` for all 3D star points in the current aspect:
    /// - `"night_sky"`: All ~6,000 naked-eye stars on the unit celestial sphere (`r = 1.005`).
    /// - `"galaxy"`: 1,800 3D galaxy structure stars + bright marker at the Home Star's galactocentric position.
    /// - `"solar_system"`: Central Home Star + each planet on its 3D tilted heliocentric orbit (plus ring particles for ringed giants).
    pub fn star_points_data(&self, aspect: &str) -> Vec<f32> {
        match aspect {
            "galaxy" => {
                let mut out = Vec::with_capacity((self.galaxy_particles.len() + 2) * 7);
                for p in &self.galaxy_particles {
                    out.extend_from_slice(&[p[0], p[1], p[2], p[3], p[4], p[5], 2.7]);
                }
                // Home Star position marker in the galaxy (outer emerald halo + inner true star color):
                let max_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
                    0.020
                } else {
                    15.0
                };
                let scale = 0.85 / max_kpc;
                let gp = self.home_star.gal_pos_kpc;
                let h = Self::tilt_3d_plane(
                    (gp[0] * scale).clamp(-0.90, 0.90),
                    (gp[1] * scale).clamp(-0.90, 0.90),
                    (gp[2] * scale).clamp(-0.90, 0.90),
                );
                let sr = self.home_star.rgb;
                out.extend_from_slice(&[h[0], h[1], h[2], 0.15, 1.0, 0.55, 12.0]);
                out.extend_from_slice(&[h[0], h[1], h[2], sr[0], sr[1], sr[2], 7.5]);
                out
            }
            "solar_system" => {
                let mut out = Vec::with_capacity((self.planets.len() * 14 + 4) * 7);
                let sr = self.home_star.rgb;
                // Central Home Star corona + photosphere at (0, 0, 0):
                out.extend_from_slice(&[0.0, 0.0, 0.0, sr[0] * 0.85, sr[1] * 0.75, sr[2] * 0.65, 22.0]);
                out.extend_from_slice(&[0.0, 0.0, 0.0, sr[0], sr[1], sr[2], 14.5]);
                let np = self.planets.len().max(1);
                for (idx, p) in self.planets.iter().enumerate() {
                    let r_norm = 0.18 + 0.72 * ((idx as f32) / ((np - 1).max(1) as f32));
                    let pos = Self::tilt_3d_plane(
                        r_norm * p.orbit_angle_rad.cos(),
                        r_norm * p.orbit_angle_rad.sin(),
                        0.0,
                    );
                    let sz = (5.8 + 2.8 * p.radius_earth.max(0.25).ln()).clamp(4.8, 13.5);
                    if p.is_home_world {
                        // Highlight Home World with a bright emerald halo behind the planet disc:
                        out.extend_from_slice(&[pos[0], pos[1], pos[2], 0.20, 0.98, 0.58, sz + 5.5]);
                    }
                    out.extend_from_slice(&[pos[0], pos[1], pos[2], p.rgb[0], p.rgb[1], p.rgb[2], sz]);
                    if p.has_rings {
                        let ring_r = 0.046f32;
                        for k in 0..16 {
                            let a = (k as f32) * 2.0 * (PI as f32) / 16.0;
                            let rp = Self::tilt_3d_plane(
                                r_norm * p.orbit_angle_rad.cos() + ring_r * a.cos(),
                                r_norm * p.orbit_angle_rad.sin() + ring_r * a.sin(),
                                0.008 * a.sin(),
                            );
                            out.extend_from_slice(&[
                                rp[0],
                                rp[1],
                                rp[2],
                                (p.rgb[0] * 0.92 + 0.08).min(1.0),
                                (p.rgb[1] * 0.88 + 0.08).min(1.0),
                                (p.rgb[2] * 0.80 + 0.08).min(1.0),
                                2.4,
                            ]);
                        }
                    }
                }
                out
            }
            _ => {
                let rad = 1.005f32;
                let mut out = Vec::with_capacity(self.stars.len() * 7);
                for s in &self.stars {
                    let x = (s.dir[0] as f32) * rad;
                    let y = (s.dir[1] as f32) * rad;
                    let z = (s.dir[2] as f32) * rad;
                    // Size in pixels based on apparent magnitude (-1.5 -> 5.2 px, 6.0 -> 1.3 px):
                    let size_px = (3.6 - 0.38 * s.app_mag).clamp(1.25, 5.5);
                    out.extend_from_slice(&[x, y, z, s.rgb[0], s.rgb[1], s.rgb[2], size_px]);
                }
                out
            }
        }
    }

    /// 3D line segments `[x0, y0, z0, x1, y1, z1, ...]` for constellation asterisms, galactic plane,
    /// 3D galactic orbit reticle, 3D planetary system orbits, or 1.0 R_sun size reference circle.
    pub fn overlay_line_segments(
        &self,
        aspect: &str,
        show_galactic_plane: bool,
        show_constellations: bool,
    ) -> Vec<f32> {
        let mut lines = Vec::new();
        let rad = 1.004f32;

        if aspect == "solar_system" {
            let np = self.planets.len().max(1);
            let segs = 64;
            for (idx, p) in self.planets.iter().enumerate() {
                let r_norm = 0.18 + 0.72 * ((idx as f32) / ((np - 1).max(1) as f32));
                for k in 0..segs {
                    let a0 = (k as f32) * 2.0 * (PI as f32) / (segs as f32);
                    let a1 = ((k + 1) as f32) * 2.0 * (PI as f32) / (segs as f32);
                    let p0 = Self::tilt_3d_plane(r_norm * a0.cos(), r_norm * a0.sin(), 0.0);
                    let p1 = Self::tilt_3d_plane(r_norm * a1.cos(), r_norm * a1.sin(), 0.0);
                    lines.extend_from_slice(&[p0[0], p0[1], p0[2], p1[0], p1[1], p1[2]]);
                }
                let pos = Self::tilt_3d_plane(
                    r_norm * p.orbit_angle_rad.cos(),
                    r_norm * p.orbit_angle_rad.sin(),
                    0.0,
                );
                if p.has_rings {
                    let ring_r = 0.048f32;
                    let rsegs = 28;
                    for k in 0..rsegs {
                        let a0 = (k as f32) * 2.0 * (PI as f32) / (rsegs as f32);
                        let a1 = ((k + 1) as f32) * 2.0 * (PI as f32) / (rsegs as f32);
                        let rp0 = Self::tilt_3d_plane(
                            r_norm * p.orbit_angle_rad.cos() + ring_r * a0.cos(),
                            r_norm * p.orbit_angle_rad.sin() + ring_r * a0.sin(),
                            0.008 * a0.sin(),
                        );
                        let rp1 = Self::tilt_3d_plane(
                            r_norm * p.orbit_angle_rad.cos() + ring_r * a1.cos(),
                            r_norm * p.orbit_angle_rad.sin() + ring_r * a1.sin(),
                            0.008 * a1.sin(),
                        );
                        lines.extend_from_slice(&[rp0[0], rp0[1], rp0[2], rp1[0], rp1[1], rp1[2]]);
                    }
                }
                if p.is_home_world {
                    let d = 0.065f32;
                    lines.extend_from_slice(&[pos[0] - d, pos[1], pos[2], pos[0] + d, pos[1], pos[2]]);
                    lines.extend_from_slice(&[pos[0], pos[1] - d, pos[2], pos[0], pos[1] + d, pos[2]]);
                    lines.extend_from_slice(&[pos[0], pos[1], pos[2] - d, pos[0], pos[1], pos[2] + d]);
                }
            }
            return lines;
        }

        if aspect == "galaxy" {
            // Draw galactocentric crosshair & tilted 3D orbit circle through the Home Star's galactic position:
            let max_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
                0.020
            } else {
                15.0
            };
            let scale = 0.85 / max_kpc;
            let gp = self.home_star.gal_pos_kpc;
            let gx = (gp[0] * scale).clamp(-0.90, 0.90);
            let gy = (gp[1] * scale).clamp(-0.90, 0.90);
            let gz = (gp[2] * scale).clamp(-0.90, 0.90);
            let h = Self::tilt_3d_plane(gx, gy, gz);
            let d = 0.09f32;
            lines.extend_from_slice(&[h[0] - d, h[1], h[2], h[0] + d, h[1], h[2]]);
            lines.extend_from_slice(&[h[0], h[1] - d, h[2], h[0], h[1] + d, h[2]]);
            lines.extend_from_slice(&[h[0], h[1], h[2] - d, h[0], h[1], h[2] + d]);

            let r_orb = (gx * gx + gy * gy).sqrt();
            if r_orb > 0.04 {
                let segs = 64;
                for k in 0..segs {
                    let a0 = (k as f32) * 2.0 * (PI as f32) / (segs as f32);
                    let a1 = ((k + 1) as f32) * 2.0 * (PI as f32) / (segs as f32);
                    let p0 = Self::tilt_3d_plane(r_orb * a0.cos(), r_orb * a0.sin(), gz);
                    let p1 = Self::tilt_3d_plane(r_orb * a1.cos(), r_orb * a1.sin(), gz);
                    lines.extend_from_slice(&[p0[0], p0[1], p0[2], p1[0], p1[1], p1[2]]);
                }
            }
            return lines;
        }

        if aspect == "home_sun" {
            // Reference 1.0 R_sun comparison ring in the XY plane (`r = 0.68`) so the user can compare the star's size to Sol:
            let sol_r = 0.68f32;
            let segs = 64;
            for k in 0..segs {
                if k % 2 == 1 {
                    continue; // Dashed reference circle for 1.0 R_sun
                }
                let a0 = (k as f32) * 2.0 * (PI as f32) / (segs as f32);
                let a1 = ((k + 1) as f32) * 2.0 * (PI as f32) / (segs as f32);
                lines.extend_from_slice(&[
                    sol_r * a0.cos(),
                    sol_r * a0.sin(),
                    0.02,
                    sol_r * a1.cos(),
                    sol_r * a1.sin(),
                    0.02,
                ]);
            }
            return lines;
        }

        if show_constellations {
            for &(ia, ib) in &self.constellation_edges {
                let a = self.stars[ia].dir;
                let b = self.stars[ib].dir;
                lines.extend_from_slice(&[
                    (a[0] as f32) * rad,
                    (a[1] as f32) * rad,
                    (a[2] as f32) * rad,
                    (b[0] as f32) * rad,
                    (b[1] as f32) * rad,
                    (b[2] as f32) * rad,
                ]);
            }
        }

        if show_galactic_plane {
            let n = self.galactic_pole;
            let u = self.galactic_center_dir;
            let v = normal(cross(n, u));
            let segs = 72;
            for k in 0..segs {
                let a0 = (k as f64) * 2.0 * PI / (segs as f64);
                let a1 = ((k + 1) as f64) * 2.0 * PI / (segs as f64);
                let p0 = [
                    (u[0] * a0.cos() + v[0] * a0.sin()) as f32 * rad,
                    (u[1] * a0.cos() + v[1] * a0.sin()) as f32 * rad,
                    (u[2] * a0.cos() + v[2] * a0.sin()) as f32 * rad,
                ];
                let p1 = [
                    (u[0] * a1.cos() + v[0] * a1.sin()) as f32 * rad,
                    (u[1] * a1.cos() + v[1] * a1.sin()) as f32 * rad,
                    (u[2] * a1.cos() + v[2] * a1.sin()) as f32 * rad,
                ];
                lines.extend_from_slice(&[p0[0], p0[1], p0[2], p1[0], p1[1], p1[2]]);
            }
        }

        lines
    }

    /// Background sphere RGB colors (`7,292` tiles) for each visualization aspect:
    /// - `"night_sky"`: Deep-space celestial sphere with diffuse Milky Way / galactic core glow.
    /// - `"home_sun"`: The chosen Home Star rendered as a 3D photosphere with limb darkening, convective granulation, and its true `B-V` blackbody chromaticity.
    /// - `"sky_plants"`: 3-zone Horizon Landscape Globe showing the Daytime Sky dome (top), Sunset & Twilight horizon band (equator), and Photosynthetic Plant Foliage & Ocean (bottom).
    /// - `"galaxy"` & `"solar_system"`: 3D interior point/orbit scenes (sphere mesh hidden in WebGL).
    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_galactic_plane: bool,
        selected_tile: Option<usize>,
    ) -> Vec<f32> {
        let n = self.grid.faces.len();
        let mut out = Vec::with_capacity(n * 3);

        let selected_star_tile = if aspect == "night_sky" {
            selected_tile.and_then(|sel| {
                self.nearest_star_to_tile(sel)
                    .map(|idx| self.tree.nearest(self.stars[idx].dir))
            })
        } else {
            None
        };

        for v in 0..n {
            let p = self.grid.faces[v].0;
            let mut rgb = match aspect {
                "home_sun" => {
                    // Render the Home Star's 3D photosphere with convective cells, limb darkening, and B-V color.
                    // Note: in Three.js, `p[1]` (Y) is the vertical pole axis and `p[2]` (Z) faces the camera.
                    let base = self.home_star.rgb;
                    let gran1 = ((p[0] * 18.0).sin() * (p[1] * 18.0).cos() * (p[2] * 18.0).sin()) as f32;
                    let gran2 = ((p[0] * 37.0 + 1.3).cos() * (p[2] * 37.0 - 0.7).sin()) as f32;
                    let lat = p[1].abs() as f32;
                    // Active magnetic sunspot belt at mid-latitudes (`|y| in 0.12..0.48`):
                    let spot_val = ((p[0] * 9.5 + 0.4).sin() * (p[2] * 11.0 - 0.8).cos()) as f32;
                    let is_spot = (0.12..=0.48).contains(&lat) && spot_val > 0.88;
                    let r_cyl = ((p[0] * p[0] + p[1] * p[1]).sqrt() as f32).clamp(0.0, 1.0);
                    let mu_limb = (1.0 - 0.75 * r_cyl * r_cyl).clamp(0.45, 1.0);
                    let mod_f = if is_spot {
                        0.42 * mu_limb
                    } else {
                        ((0.94 + 0.07 * gran1 + 0.04 * gran2) * mu_limb).clamp(0.45, 1.08)
                    };
                    [
                        (base[0] * mod_f).clamp(0.0, 1.0),
                        (base[1] * mod_f).clamp(0.0, 1.0),
                        (base[2] * mod_f).clamp(0.0, 1.0),
                    ]
                }
                "galaxy" | "solar_system" => {
                    // Subtle deep-space background for the 3D interior point/orbit scenes:
                    [0.02, 0.025, 0.045]
                }
                "sky_plants" => {
                    // 3-Zone Atmospheric & Biosphere Panorama Globe (using `y = p[1]` as vertical screen axis):
                    // - Upper Dome (`y > +0.16`): Daytime Sky from overhead Zenith (`y = +1.0`) to Daytime Horizon (`y = +0.16`),
                    //   with the Home Sun disk in the upper sky (`(-0.28, 0.60, 0.75)`) in its true `B-V` color & angular scale.
                    // - Equatorial Band (`-0.16 <= y <= +0.16`): Sunset & Twilight Horizon (`sunset_rgb` & `twilight_rgb`).
                    // - Lower Hemisphere (`y < -0.16`): Surface Continent with Photosynthetic Plant Foliage (`plant_foliage_rgb` & `plant_highlight_rgb`)
                    //   alongside Coastal Ocean reflecting the sky and sunset.
                    let y = p[1] as f32;
                    let op = &self.optics;
                    let c1 = ((p[0] * 4.2 + 0.3).sin() * (p[2] * 4.0 - 0.4).cos()) as f32;
                    let c2 = ((p[0] * 11.5 - 0.8).cos() * (p[1] * 9.0 + 0.5).sin() * (p[2] * 11.2 + 0.6).cos()) as f32;
                    let c3 = ((p[0] * 21.0 + 1.1).sin() * (p[2] * 21.0 - 0.9).cos()) as f32;

                    if y > 0.16 {
                        // Upper Dome: Daytime Sky (Zenith at top `y = 1.0` -> Horizon at `y = 0.16`)
                        let t_zen = ((y - 0.16) / 0.84).clamp(0.0, 1.0).powf(0.75);
                        let mut sky = [
                            op.sky_horizon_rgb[0] * (1.0 - t_zen) + op.sky_zenith_rgb[0] * t_zen,
                            op.sky_horizon_rgb[1] * (1.0 - t_zen) + op.sky_zenith_rgb[1] * t_zen,
                            op.sky_horizon_rgb[2] * (1.0 - t_zen) + op.sky_zenith_rgb[2] * t_zen,
                        ];
                        // Home Sun disk in the daytime sky dome:
                        let sun_pos = normal([-0.26, 0.62, 0.74]);
                        let ang = dot(p, sun_pos).clamp(-1.0, 1.0).acos() as f32;
                        let sun_rad = (0.095 * self.home_star.radius_solar.sqrt()).clamp(0.055, 0.16);
                        if ang <= sun_rad {
                            sky = op.sun_rgb;
                        } else if ang <= sun_rad * 2.2 {
                            let glow = (1.0 - (ang - sun_rad) / (sun_rad * 1.2)).clamp(0.0, 1.0).powi(2) * 0.65;
                            sky[0] = (sky[0] * (1.0 - glow) + op.sun_rgb[0] * glow).clamp(0.0, 1.0);
                            sky[1] = (sky[1] * (1.0 - glow) + op.sun_rgb[1] * glow).clamp(0.0, 1.0);
                            sky[2] = (sky[2] * (1.0 - glow) + op.sun_rgb[2] * glow).clamp(0.0, 1.0);
                        } else {
                            // Subtle sunlit fair-weather clouds in lower daytime sky:
                            let cloud = ((c2 - 0.42) / 0.58).clamp(0.0, 1.0) * (1.0 - t_zen) * 0.40;
                            sky[0] = (sky[0] * (1.0 - cloud) + (0.65 * op.sun_rgb[0] + 0.35 * sky[0]) * cloud).clamp(0.0, 1.0);
                            sky[1] = (sky[1] * (1.0 - cloud) + (0.65 * op.sun_rgb[1] + 0.35 * sky[1]) * cloud).clamp(0.0, 1.0);
                            sky[2] = (sky[2] * (1.0 - cloud) + (0.65 * op.sun_rgb[2] + 0.35 * sky[2]) * cloud).clamp(0.0, 1.0);
                        }
                        sky
                    } else if y >= -0.16 {
                        // Equatorial Band: Sunset & Twilight Horizon (`y` in `-0.16 .. +0.16`)
                        // Smooth transition from Daytime Horizon (`y = +0.16`) -> Twilight -> Vivid Sunset Glow (`y = -0.04 .. +0.04`):
                        let u = (y + 0.16) / 0.32; // 0.0 at bottom of sunset band, 1.0 at top
                        let mut band = if u >= 0.55 {
                            let t = ((u - 0.55) / 0.45).clamp(0.0, 1.0);
                            [
                                op.twilight_rgb[0] * (1.0 - t) + op.sky_horizon_rgb[0] * t,
                                op.twilight_rgb[1] * (1.0 - t) + op.sky_horizon_rgb[1] * t,
                                op.twilight_rgb[2] * (1.0 - t) + op.sky_horizon_rgb[2] * t,
                            ]
                        } else {
                            let t = (u / 0.55).clamp(0.0, 1.0);
                            [
                                op.sunset_rgb[0] * (1.0 - 0.45 * t) + op.twilight_rgb[0] * (0.45 * t),
                                op.sunset_rgb[1] * (1.0 - 0.45 * t) + op.twilight_rgb[1] * (0.45 * t),
                                op.sunset_rgb[2] * (1.0 - 0.45 * t) + op.twilight_rgb[2] * (0.45 * t),
                            ]
                        };
                        // Setting sun aureole on the horizon (`(0.32, -0.02, 0.95)`):
                        let set_pos = normal([0.32, -0.02, 0.95]);
                        let ang = dot(p, set_pos).clamp(-1.0, 1.0).acos() as f32;
                        if ang <= 0.22 {
                            let aureole = (1.0 - ang / 0.22).powi(2) * 0.55;
                            band[0] = (band[0] * (1.0 - aureole) + op.sunset_rgb[0].max(0.85 * op.sun_rgb[0]) * aureole).clamp(0.0, 1.0);
                            band[1] = (band[1] * (1.0 - aureole) + op.sunset_rgb[1] * aureole).clamp(0.0, 1.0);
                            band[2] = (band[2] * (1.0 - aureole) + op.sunset_rgb[2] * aureole).clamp(0.0, 1.0);
                        }
                        band
                    } else {
                        // Lower Hemisphere (`y < -0.16`): Photosynthetic Plant Foliage Canopy (Left/Center) & Coastal Ocean (Right)
                        let coast_line = 0.34 + 0.16 * c1;
                        let is_ocean = (p[0] as f32) > coast_line;
                        if is_ocean {
                            // Ocean reflects the overhead daytime sky & warm sunset horizon:
                            let wave = 0.90 + 0.10 * c3;
                            let sunset_refl = ((y + 0.45) / 0.29).clamp(0.0, 1.0) * 0.38;
                            [
                                ((0.05 + 0.55 * op.sky_zenith_rgb[0]) * (1.0 - sunset_refl) + op.sunset_rgb[0] * sunset_refl) * wave,
                                ((0.10 + 0.62 * op.sky_zenith_rgb[1]) * (1.0 - sunset_refl) + op.sunset_rgb[1] * sunset_refl) * wave,
                                ((0.18 + 0.70 * op.sky_zenith_rgb[2]) * (1.0 - sunset_refl) + op.sunset_rgb[2] * sunset_refl) * wave,
                            ]
                        } else {
                            // Lush photosynthetic plant canopy showing main foliage & highlight pigments:
                            let canopy_t = (0.5 + 0.35 * c2 + 0.25 * c3).clamp(0.0, 1.0);
                            [
                                (op.plant_foliage_rgb[0] * (1.0 - canopy_t) + op.plant_highlight_rgb[0] * canopy_t).clamp(0.0, 1.0),
                                (op.plant_foliage_rgb[1] * (1.0 - canopy_t) + op.plant_highlight_rgb[1] * canopy_t).clamp(0.0, 1.0),
                                (op.plant_foliage_rgb[2] * (1.0 - canopy_t) + op.plant_highlight_rgb[2] * canopy_t).clamp(0.0, 1.0),
                            ]
                        }
                    }
                }
                _ => {
                    // "night_sky": Smooth deep-space celestial sphere with subtle diffuse galactic band / core glow
                    // (crisp stars are rendered as `THREE.Points` via `star_points_data("night_sky")`):
                    let sin_b = dot(p, self.galactic_pole);
                    let cos_core = dot(p, self.galactic_center_dir);
                    let gal_glow = match (self.galaxy_type, self.vantage) {
                        (
                            GalaxyType::Spiral | GalaxyType::BarredSpiral | GalaxyType::Lenticular,
                            GalacticVantage::SpiralArm | GalacticVantage::InterArmDisk,
                        ) => {
                            let band = (-((sin_b / 0.18).powi(2))).exp();
                            let bulge = (cos_core.max(0.0).powi(4)) * (-((sin_b / 0.32).powi(2))).exp();
                            (0.08 * band + 0.10 * bulge) as f32
                        }
                        (_, GalacticVantage::GalacticHalo) => {
                            let disk_hemisphere = ((-sin_b).max(0.0)).powf(1.5) * (0.4 + 0.6 * cos_core.max(0.0));
                            (0.12 * disk_hemisphere) as f32
                        }
                        _ => {
                            let core_hemisphere = ((cos_core + 0.35).max(0.0) / 1.35).powi(2);
                            (0.10 * core_hemisphere) as f32
                        }
                    };

                    let glow_tint = if matches!(
                        self.galaxy_type,
                        GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
                    ) {
                        [0.95, 0.78, 0.55] // Warm golden Population II glow
                    } else {
                        [0.62, 0.74, 0.98] // Subtle milky blue-white band
                    };

                    let mut bg = [
                        0.010 + gal_glow * glow_tint[0],
                        0.013 + gal_glow * glow_tint[1],
                        0.024 + gal_glow * glow_tint[2],
                    ];

                    if show_galactic_plane && sin_b.abs() < 0.020 {
                        bg[0] = (bg[0] + 0.06).min(1.0);
                        bg[1] = (bg[1] + 0.11).min(1.0);
                        bg[2] = (bg[2] + 0.20).min(1.0);
                    }
                    bg
                }
            };

            if aspect == "night_sky" {
                if selected_star_tile == Some(v) {
                    rgb = [0.18, 0.75, 0.45];
                }
            } else if selected_tile == Some(v) {
                rgb = [
                    (rgb[0] * 0.65 + 0.15).min(1.0),
                    (rgb[1] * 0.65 + 0.35).min(1.0),
                    (rgb[2] * 0.65 + 0.25).min(1.0),
                ];
            }
            out.extend_from_slice(&rgb);
        }

        out
    }

    /// Find the index of the visible star nearest to `tile_idx` on the celestial sphere.
    pub fn nearest_star_to_tile(&self, tile_idx: usize) -> Option<usize> {
        let p = self.grid.faces.get(tile_idx)?.0;
        self.stars
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                dot(p, a.dir)
                    .partial_cmp(&dot(p, b.dir))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// One-line summary of the generated galaxy, chosen Home Star, Planetary System, and Sky/Sunset/Plant optics.
    pub fn summary_line(&self) -> String {
        let hs = &self.home_star;
        let op = &self.optics;
        let r_gal = (hs.gal_pos_kpc[0] * hs.gal_pos_kpc[0]
            + hs.gal_pos_kpc[1] * hs.gal_pos_kpc[1]
            + hs.gal_pos_kpc[2] * hs.gal_pos_kpc[2])
            .sqrt();
        format!(
            "Sun: {} ({:.0} K, B-V {:+.2}, {:.2} R☉) | Planets: {} (Home #{}) | Sky: {} | Sunset: {} | Plants: {} | {} (R={:.1} kpc)",
            hs.spectral_type,
            hs.temp_k,
            hs.bv_color,
            hs.radius_solar,
            self.planets.len(),
            self.home_planet_index + 1,
            op.sky_color_name,
            op.sunset_color_name,
            op.plant_color_name,
            self.galaxy_type.name(),
            r_gal
        )
    }

    /// Context-aware inspection for the active view mode (`aspect`) when clicking `tile_idx`.
    pub fn inspect_aspect_tile(&self, aspect: &str, tile_idx: usize) -> String {
        let hs = &self.home_star;
        let op = &self.optics;
        let p = self
            .grid
            .faces
            .get(tile_idx)
            .map(|f| f.0)
            .unwrap_or([0.0, 0.0, 1.0]);

        match aspect {
            "sky_plants" => {
                let y = p[1] as f32;
                let zone_report = if y > 0.16 {
                    format!(
                        "Clicked Region: Upper Dome — Daytime Sky ({})\n\
                         Why it has this color: {} ({}) scattered by {} ({}).\n\
                         Overhead Zenith RGB: ({:.0}%, {:.0}%, {:.0}%) → Horizon RGB: ({:.0}%, {:.0}%, {:.0}%)",
                        op.sky_color_name,
                        op.sky_mechanism,
                        self.atmosphere.short_formula(),
                        hs.spectral_type,
                        hs.class_description,
                        op.sky_zenith_rgb[0] * 100.0,
                        op.sky_zenith_rgb[1] * 100.0,
                        op.sky_zenith_rgb[2] * 100.0,
                        op.sky_horizon_rgb[0] * 100.0,
                        op.sky_horizon_rgb[1] * 100.0,
                        op.sky_horizon_rgb[2] * 100.0
                    )
                } else if y >= -0.16 {
                    format!(
                        "Clicked Region: Equatorial Horizon Band — Sunset & Twilight ({})\n\
                         Why it has this color: {} through {}.",
                        op.sunset_color_name,
                        op.sunset_mechanism,
                        self.atmosphere.name()
                    )
                } else {
                    format!(
                        "Clicked Region: Lower Hemisphere — Photosynthetic Plant Canopy & Ocean ({})\n\
                         Why plants have this color: {} under a {} ({:.0} K) sun and {} atmosphere.",
                        op.plant_color_name,
                        op.plant_pigment_desc,
                        hs.spectral_type,
                        hs.temp_k,
                        self.atmosphere.short_formula()
                    )
                };
                format!(
                    "{zone_report}\n\
                     Home Sun: {} ({}) — {:.0} K, {:.2} R☉, {:.2} L☉ | Habitable Orbit: {:.2} AU ({:.0}d/yr)",
                    hs.spectral_type,
                    hs.class_description,
                    hs.temp_k,
                    hs.radius_solar,
                    hs.luminosity_solar,
                    hs.habitable_au,
                    hs.orbital_period_days
                )
            }
            "home_sun" => {
                let lat_deg = (p[1].clamp(-1.0, 1.0).asin() * 180.0 / PI).abs();
                let region = if (7.0..=29.0).contains(&lat_deg) {
                    "Active Mid-Latitude Magnetic Belt (Sunspot & Faculae Zone)"
                } else if lat_deg > 60.0 {
                    "Polar Photospheric Cap (Limb-Darkened Convection)"
                } else {
                    "Equatorial Photospheric Granulation Zone"
                };
                format!(
                    "Clicked Home Sun Photosphere (Lat {:.1}°): {}\n\
                     Spectral Type: {} ({}) | Effective Temp: {:.0} K (B-V = {:+.2})\n\
                     Radius: {:.2} R☉ (dashed blue ring = 1.00 R☉ Sol reference) | Mass: {:.2} M☉ | Luminosity: {:.2} L☉\n\
                     Habitable Zone Orbit: {:.2} AU ({:.0} Earth days/yr) | Angular Size in Sky: {:.2}° | Solar Tides: {:.2}x",
                    lat_deg,
                    region,
                    hs.spectral_type,
                    hs.class_description,
                    hs.temp_k,
                    hs.bv_color,
                    hs.radius_solar,
                    hs.mass_solar,
                    hs.luminosity_solar,
                    hs.habitable_au,
                    hs.orbital_period_days,
                    hs.angular_diameter_deg,
                    hs.solar_tide_factor
                )
            }
            "galaxy" => {
                let gp = hs.gal_pos_kpc;
                let r_kpc = (gp[0] * gp[0] + gp[1] * gp[1] + gp[2] * gp[2]).sqrt();
                format!(
                    "3D Host Galaxy View: {} — Vantage: {}\n\
                     Home Star Galactocentric Position: (x={:+.2}, y={:+.2}, z={:+.2}) kpc | Distance from Galactic Center: R = {:.2} kpc\n\
                     (Green 3D crosshair & orbit circle mark your Home Star's location and galactic orbit inside the galaxy)\n\
                     Home Sun: {} ({}) — {:.0} K, {:.2} M☉, {:.2} R☉, Age {:.1}/{:.1} Gyr | Visible Night-Sky Stars: {}",
                    self.galaxy_type.name(),
                    self.vantage.name(),
                    gp[0],
                    gp[1],
                    gp[2],
                    r_kpc,
                    hs.spectral_type,
                    hs.class_description,
                    hs.temp_k,
                    hs.mass_solar,
                    hs.radius_solar,
                    hs.age_gyr,
                    hs.lifespan_gyr,
                    self.stars.len()
                )
            }
            "solar_system" => {
                let np = self.planets.len().max(1);
                let best_idx = self
                    .planets
                    .iter()
                    .enumerate()
                    .min_by(|&(idx_a, pl_a), &(idx_b, pl_b)| {
                        let dist_sq = |idx: usize, pl: &SolarSystemPlanet| {
                            let r_norm = 0.18 + 0.72 * ((idx as f32) / ((np - 1).max(1) as f32));
                            let pos = Self::tilt_3d_plane(
                                r_norm * pl.orbit_angle_rad.cos(),
                                r_norm * pl.orbit_angle_rad.sin(),
                                0.0,
                            );
                            let dx = (p[0] as f32) - pos[0];
                            let dy = (p[1] as f32) - pos[1];
                            dx * dx + dy * dy
                        };
                        dist_sq(idx_a, pl_a)
                            .partial_cmp(&dist_sq(idx_b, pl_b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(idx, _)| idx)
                    .unwrap_or_else(|| self.home_planet_index.min(np - 1));
                let pl = &self.planets[best_idx];
                let vis_str = if pl.is_home_world {
                    "Simulated Home World (marked with 3D target crosshair)".to_string()
                } else if pl.is_naked_eye_visible() {
                    format!("Naked-eye visible in night sky (m_V = {:+.1})", pl.apparent_mag)
                } else {
                    format!("Telescopic in night sky (m_V = {:+.1})", pl.apparent_mag)
                };
                format!(
                    "Selected Solar System Planet: Planet #{} — {} ({})\n\
                     Orbit: {:.2} AU (Period: {:.0} Earth days) | Radius: {:.2} R⊕ | Rings: {} | Moons: {}\n\
                     Full System ({} Planets): {}",
                    pl.index + 1,
                    pl.kind.appearance_label(),
                    vis_str,
                    pl.orbit_au,
                    pl.orbital_period_days,
                    pl.radius_earth,
                    if pl.has_rings { "Yes" } else { "No" },
                    pl.moon_count,
                    self.planets.len(),
                    self.planets_summary_text()
                )
            }
            _ => self.inspect_tile(tile_idx),
        }
    }

    /// Flat star buffer `[ra_rad, dec_deg, app_mag, base_r, base_g, base_b, ...]` (`6` floats per star)
    /// for rendering the 2D Northern & Southern Celestial Hemisphere Star Maps.
    pub fn star_map_stars_data(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.stars.len() * 6);
        for s in &self.stars {
            let (ra_deg, dec_deg) = star_dir_to_ra_dec_deg(s.dir);
            out.extend_from_slice(&[
                ra_deg.to_radians(),
                dec_deg,
                s.app_mag,
                s.base_rgb[0],
                s.base_rgb[1],
                s.base_rgb[2],
            ]);
        }
        out
    }

    /// Flat constellation edge buffer `[star_idx_a, star_idx_b, constellation_idx, ...]` (`3` `u32`s per edge)
    /// for the 2D Northern & Southern Celestial Hemisphere Star Maps.
    pub fn star_map_constellation_edges(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.constellation_edges.len() * 3);
        for c in &self.constellations {
            for &(ia, ib) in &c.edges {
                out.extend_from_slice(&[ia as u32, ib as u32, c.index]);
            }
        }
        out
    }

    /// Metadata JSON for the 2D Northern & Southern Celestial Hemisphere Star Maps:
    /// galactic & ecliptic plane poles, diffuse galaxy band mode, current Sun & naked-eye planet
    /// equatorial positions, and constellation centroids/labels.
    pub fn star_map_metadata_json(&self) -> String {
        let galaxy_band_mode: u8 = match (self.galaxy_type, self.vantage) {
            (_, GalacticVantage::GalacticHalo) => 1,
            (GalaxyType::Elliptical | GalaxyType::GlobularCluster, _)
            | (_, GalacticVantage::GalacticBulge) => 2,
            (GalaxyType::Irregular, _) => 3,
            _ => 0,
        };
        let warm_glow = matches!(
            self.galaxy_type,
            GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
        );
        let (sun_ra_deg, sun_dec_deg) = star_dir_to_ra_dec_deg(self.sun_dir);
        let sr = self.home_star.rgb;

        let mut planet_entries = Vec::new();
        for p in &self.planets {
            if p.is_home_world || !p.is_naked_eye_visible() {
                continue;
            }
            let (p_ra, p_dec) = star_dir_to_ra_dec_deg(p.sky_dir);
            planet_entries.push(format!(
                "{{\"number\":{},\"raRad\":{:.5},\"decDeg\":{:.3},\"appMag\":{:.2},\"rgb\":[{:.3},{:.3},{:.3}]}}",
                p.index + 1,
                p_ra.to_radians(),
                p_dec,
                p.apparent_mag,
                p.rgb[0],
                p.rgb[1],
                p.rgb[2]
            ));
        }

        let mut const_entries = Vec::new();
        for c in &self.constellations {
            const_entries.push(format!(
                "{{\"index\":{},\"number\":{},\"raRad\":{:.5},\"decDeg\":{:.3},\"starCount\":{},\"brightestMag\":{:.2},\"band\":\"{}\"}}",
                c.index,
                c.index + 1,
                c.right_ascension_deg.to_radians(),
                c.declination_deg,
                c.star_count,
                c.brightest_mag,
                c.hemisphere_band_label()
            ));
        }

        let gp = self.galactic_pole;
        let gc = self.galactic_center_dir;
        let ep = self.ecliptic_pole;
        format!(
            "{{\"isRealHipparcos\":{},\"starCount\":{},\"constellationCount\":{},\"galaxyBandMode\":{},\"warmGlow\":{},\"galacticPole\":[{:.5},{:.5},{:.5}],\"galacticCenterDir\":[{:.5},{:.5},{:.5}],\"eclipticPole\":[{:.5},{:.5},{:.5}],\"sun\":{{\"raRad\":{:.5},\"decDeg\":{:.3},\"rgb\":[{:.3},{:.3},{:.3}]}},\"planets\":[{}],\"constellations\":[{}]}}",
            self.is_real_hipparcos,
            self.stars.len(),
            self.constellations.len(),
            galaxy_band_mode,
            warm_glow,
            gp[0],
            gp[1],
            gp[2],
            gc[0],
            gc[1],
            gc[2],
            ep[0],
            ep[1],
            ep[2],
            sun_ra_deg.to_radians(),
            sun_dec_deg,
            sr[0],
            sr[1],
            sr[2],
            planet_entries.join(","),
            const_entries.join(",")
        )
    }

    /// Return the 0-based `Constellation` index containing `star_idx`, if any.
    pub fn star_constellation_index(&self, star_idx: usize) -> Option<u32> {
        self.constellations
            .iter()
            .find(|c| c.star_indices.contains(&star_idx))
            .map(|c| c.index)
    }

    /// Find the star nearest to equatorial coordinates `(ra_rad, dec_deg)` on the celestial sphere,
    /// gently favoring brighter / constellation stars within `2.6°` of the clicked point.
    pub fn find_nearest_star_ra_dec(&self, ra_rad: f64, dec_deg: f64) -> Option<usize> {
        if self.stars.is_empty() {
            return None;
        }
        let dec_rad = dec_deg.clamp(-90.0, 90.0) * PI / 180.0;
        let cos_d = dec_rad.cos();
        let target = normal([cos_d * ra_rad.cos(), cos_d * ra_rad.sin(), dec_rad.sin()]);

        // First check if a bright / constellation star lies within ~2.6° (`cos >= 0.9989`) of the click:
        let mut best_bright: Option<(usize, f64)> = None;
        for (idx, st) in self.stars.iter().enumerate() {
            let cos_ang = dot(target, st.dir);
            if cos_ang >= 0.9989 && st.app_mag <= 4.2 {
                let score = cos_ang + 0.00022 * ((4.5 - st.app_mag as f64).max(0.0));
                if best_bright.is_none_or(|(_, bs)| score > bs) {
                    best_bright = Some((idx, score));
                }
            }
        }
        if let Some((idx, _)) = best_bright {
            return Some(idx);
        }

        self.stars
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                dot(target, a.dir)
                    .partial_cmp(&dot(target, b.dir))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Detailed multi-line inspection of `stars[star_idx]` including equatorial coordinates (`RA`, `Dec`),
    /// hemisphere, constellation membership, spectral class, luminosity, and Home Sun context.
    pub fn inspect_star_index(&self, star_idx: usize) -> String {
        let Some(st) = self.stars.get(star_idx) else {
            return String::new();
        };
        let hs = &self.home_star;
        let op = &self.optics;
        let cls = classify_star(st.bv_color, st.abs_mag);
        let st_temp = bv_to_temp_k(st.bv_color);
        let st_lum = 10.0_f32.powf((4.83 - st.abs_mag) / 2.5);
        let (ra_deg, dec_deg) = star_dir_to_ra_dec_deg(st.dir);
        let ra_total_min = ((ra_deg as f64) / 15.0 * 60.0).round() as u32;
        let ra_h = (ra_total_min / 60) % 24;
        let ra_m = ra_total_min % 60;
        let hemi = if dec_deg >= 0.0 {
            "Northern Celestial Hemisphere"
        } else {
            "Southern Celestial Hemisphere"
        };
        let const_line = if let Some(c_idx) = self.star_constellation_index(star_idx) {
            let c = &self.constellations[c_idx as usize];
            format!(
                "Constellation C{} ({} stars, {}, brightest m_V = {:+.2})",
                c.index + 1,
                c.star_count,
                c.hemisphere_band_label(),
                c.brightest_mag
            )
        } else {
            "Ungrouped Field Star".to_string()
        };
        let gal_lat_deg = (90.0 - (st.galactic_offset_rad as f64 * 180.0 / PI)).abs();
        let cat_prefix = if self.is_real_hipparcos { "HIP " } else { "Star #" };
        let sys_summary = self.planets_summary_text();

        format!(
            "Selected Star: {cat_prefix}{} — {}{}{} ({}) | {}\n\
             Coordinates: RA {:02}h {:02}m ({:.1}°), Dec {:+.2}° ({hemi}) | Galactic Lat: {:.1}°\n\
             Apparent Magnitude: m_V = {:+.2} (naked-eye ≤ +6.0) | Absolute Magnitude: M_V = {:+.2} ({:.1} L☉)\n\
             Distance: {:.1} light-years ({:.1} pc) | Color Index: B-V = {:+.2} (T_eff ≈ {:.0} K)\n\
             Home Sun: {} ({}) — {:.0} K, {:.2} M☉, {:.2} R☉, {:.2} L☉ | Planets ({}): {}\n\
             Atmosphere: {} — Sky: {} | Sunset: {} | Plants: {}",
            st.id,
            cls.spectral_letter,
            cls.subclass,
            cls.luminosity_class,
            cls.description,
            const_line,
            ra_h,
            ra_m,
            ra_deg,
            dec_deg,
            gal_lat_deg,
            st.app_mag,
            st.abs_mag,
            st_lum,
            st.distance_ly,
            st.distance_ly / 3.26156,
            st.bv_color,
            st_temp,
            hs.spectral_type,
            hs.class_description,
            hs.temp_k,
            hs.mass_solar,
            hs.radius_solar,
            hs.luminosity_solar,
            self.planets.len(),
            sys_summary,
            self.atmosphere.name(),
            op.sky_color_name,
            op.sunset_color_name,
            op.plant_color_name
        )
    }

    /// Detailed multi-line inspection of the selected star on the celestial sphere, the Home Sun,
    /// the generated Planetary System, and the planet's Sky, Sunset, and Photosynthetic Plant Foliage colors.
    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        let Some(star_idx) = self.nearest_star_to_tile(tile_idx) else {
            return String::new();
        };
        self.inspect_star_index(star_idx)
    }
}

/// Complete stellar, atmospheric, lunar, solar-system, and constellation environment reconstructed
/// from a `tec11_proto::PlanetaryEnvironment` (or defaulted to Earth / Sol when `None`).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sun_sandbox_galaxy_types_and_hipparcos() {
        let mut sb = SunSandbox::new(42);
        assert_eq!(sb.star_count(), 6000);
        assert!(sb.home_star().radius_solar > 0.15 && sb.home_star().radius_solar < 4.0);
        assert!(sb.home_star().temp_k > 2500.0 && sb.home_star().temp_k < 12000.0);

        // Test generating each galaxy morphology (Spiral, BarredSpiral, Lenticular, Elliptical, Irregular, GlobularCluster):
        for g_id in 0..=5 {
            sb.generate_galaxy_and_sun(g_id, 99, 0);
            assert_eq!(sb.galaxy_type().id(), g_id);
            assert_eq!(sb.star_count(), 6000);
            let pts = sb.star_points_data("night_sky");
            assert_eq!(pts.len(), 6000 * 7);
        }

        // Test loading real Hipparcos catalogue around Sol:
        sb.load_real_hipparcos_sol();
        assert!(sb.is_real_hipparcos());
        assert_eq!(sb.star_count(), 5179);
        assert_eq!(sb.home_star().spectral_type, "G2V");
        let report = sb.inspect_tile(0);
        assert!(report.contains("HIP"));
    }

    #[test]
    fn test_planetary_optics_sky_sunset_and_plant_colors() {
        let mut sb = SunSandbox::new(77);

        // 1. Sol-like G2V star + Earth-like N2-O2 atmosphere:
        sb.load_real_hipparcos_sol();
        sb.set_atmosphere(0); // EarthLikeN2O2
        let op_g = sb.optics().clone();
        // Sky zenith should be strongly blue (B > R), sunset strongly red-orange (R > B), and plants green (G > R, G > B):
        assert!(op_g.sky_zenith_rgb[2] > op_g.sky_zenith_rgb[0]);
        assert!(op_g.sunset_rgb[0] > op_g.sunset_rgb[2]);
        assert!(op_g.plant_foliage_rgb[1] > op_g.plant_foliage_rgb[0]);
        assert!(op_g.plant_foliage_rgb[1] > op_g.plant_foliage_rgb[2]);

        // 2. M-class Red Dwarf (filter = 4): dark purple-black pan-visible absorbing flora:
        sb.choose_sun_in_current_galaxy(4);
        let op_m = sb.optics().clone();
        assert!(op_m.plant_foliage_rgb[0] < 0.35 && op_m.plant_foliage_rgb[1] < 0.20);
        assert!(op_m.plant_color_name.contains("Black") || op_m.plant_color_name.contains("Indigo"));

        // 3. K-class Orange Dwarf (filter = 3) + N2-O2: autumnal orange-gold flora (R > B):
        sb.choose_sun_in_current_galaxy(3);
        sb.set_atmosphere(0);
        let op_k = sb.optics().clone();
        assert!(op_k.plant_foliage_rgb[0] > op_k.plant_foliage_rgb[2]);

        // 4. Hot A-class Blue-White star (filter = 5) + N2-O2: blue-teal UV-shielding flora:
        sb.choose_sun_in_current_galaxy(5);
        sb.set_atmosphere(0);
        let op_a = sb.optics().clone();
        assert!(op_a.plant_foliage_rgb[2] > op_a.plant_foliage_rgb[0]);

        // 5. Methane-Rich (N2-CH4) atmosphere (id = 1): cyan sky and emerald-teal sunset (G > R at sunset):
        sb.load_real_hipparcos_sol();
        sb.set_atmosphere(1);
        let op_ch4 = sb.optics().clone();
        assert!(op_ch4.sunset_rgb[1] > op_ch4.sunset_rgb[0]);

        // 6. Thin Dusty CO2 (Mars-like, id = 4): butterscotch daytime sky (R > B) with cool blue sunset halo (B > R):
        sb.set_atmosphere(4);
        let op_mars = sb.optics().clone();
        assert!(op_mars.sky_zenith_rgb[0] > op_mars.sky_zenith_rgb[2]);
        assert!(op_mars.sunset_rgb[2] > op_mars.sunset_rgb[0]);

        // Verify 3D "sky_plants" and "solar_system" aspects render all 7,292 tiles:
        let sky_plant_colors = sb.tile_colors_rgb("sky_plants", false, Some(0));
        assert_eq!(sky_plant_colors.len(), sb.tile_count() * 3);
        let solar_sys_colors = sb.tile_colors_rgb("solar_system", false, Some(0));
        assert_eq!(solar_sys_colors.len(), sb.tile_count() * 3);
        assert!(sb.planet_count() >= 4);
        assert!(sb.planets()[sb.home_planet_index()].is_home_world);
    }

    #[test]
    fn test_galactic_sky_constellations_quadrants_and_vantage_dependence() {
        fn assert_full_sky_2d_dispersion(consts: &[Constellation], label: &str) {
            assert!(
                consts.len() >= 10,
                "{}: expected >= 10 constellations, got {}",
                label,
                consts.len()
            );
            let mut quadrants = [0usize; 4];
            let mut has_north_polar = false;
            let mut has_south_polar = false;
            for c in consts {
                let q = ((c.right_ascension_deg.rem_euclid(360.0)) / 90.0).floor() as usize;
                quadrants[q.min(3)] += 1;
                if c.declination_deg >= 55.0 {
                    has_north_polar = true;
                }
                if c.declination_deg <= -55.0 {
                    has_south_polar = true;
                }
                assert!(
                    c.resolved_stars.len() >= 3,
                    "{}: constellation must have >= 3 resolved stars",
                    label
                );
                assert!(
                    !c.local_edges.is_empty(),
                    "{}: constellation must have connected edges",
                    label
                );
            }
            for (qi, &cnt) in quadrants.iter().enumerate() {
                assert!(
                    cnt >= 1,
                    "{}: expected >= 1 constellation in RA quadrant {}, got {:?}",
                    label,
                    qi,
                    quadrants
                );
            }
            assert!(has_north_polar, "{}: expected northern polar constellation", label);
            assert!(has_south_polar, "{}: expected southern polar constellation", label);

            // Verify constellations are NOT arranged in a 1D diagonal line (|r(RA, Dec)| << 1):
            let n = consts.len() as f32;
            let mean_ra = consts.iter().map(|c| c.right_ascension_deg).sum::<f32>() / n;
            let mean_dec = consts.iter().map(|c| c.declination_deg).sum::<f32>() / n;
            let cov: f32 = consts
                .iter()
                .map(|c| (c.right_ascension_deg - mean_ra) * (c.declination_deg - mean_dec))
                .sum();
            let var_ra: f32 = consts
                .iter()
                .map(|c| (c.right_ascension_deg - mean_ra).powi(2))
                .sum();
            let var_dec: f32 = consts
                .iter()
                .map(|c| (c.declination_deg - mean_dec).powi(2))
                .sum();
            let corr = (cov / (var_ra * var_dec).sqrt().max(1e-6)).abs();
            assert!(
                corr < 0.65,
                "{}: constellations appear collinear in (RA, Dec) with |r| = {:.3}",
                label,
                corr
            );

            let backdrop = consts[0]
                .backdrop
                .as_ref()
                .expect("expected GalacticSkyBackdrop on synthesized constellations");
            assert!(
                backdrop.field_stars.len() >= 80,
                "{}: expected >= 80 ungrouped field stars, got {}",
                label,
                backdrop.field_stars.len()
            );
        }

        // 1. Sol / Earth (real Hipparcos 3D catalog around [8.1, 0.0, 0.02] kpc):
        let sol_consts = synthesize_galactic_sky_constellations(
            777,
            true,
            GalaxyType::BarredSpiral,
            GalacticVantage::SpiralArm,
            [8.1, 0.0, 0.02],
        );
        assert_full_sky_2d_dispersion(&sol_consts, "Sol Hipparcos");

        // 2. Procedural mid-plane Spiral Arm solar system ([8.0, 0.0, 0.03] kpc):
        let disk_consts = synthesize_galactic_sky_constellations(
            4242,
            false,
            GalaxyType::BarredSpiral,
            GalacticVantage::SpiralArm,
            [8.0, 0.0, 0.03],
        );
        assert_full_sky_2d_dispersion(&disk_consts, "Procedural SpiralArm");

        // 3. Galactic Halo system ([3.2, 0.0, 1.8] kpc) sits high above the inner galactic disk,
        //    so distant luminous stars skew strongly toward `galactic_center_dir` / `-galactic_pole`:
        let halo_consts = synthesize_galactic_sky_constellations(
            4242,
            false,
            GalaxyType::Spiral,
            GalacticVantage::GalacticHalo,
            [3.2, 0.0, 1.8],
        );
        let halo_bd = halo_consts[0]
            .backdrop
            .as_ref()
            .expect("expected halo backdrop");
        assert_eq!(halo_bd.galaxy_band_mode, 1);

        let ra_dec_to_eq = |ra_deg: f32, dec_deg: f32| -> [f32; 3] {
            let ra = ra_deg.to_radians();
            let dec = dec_deg.to_radians();
            let cd = dec.cos();
            [cd * ra.cos(), cd * ra.sin(), dec.sin()]
        };
        let gc = halo_bd.galactic_center_dir;
        let toward_galaxy_stars = halo_bd
            .field_stars
            .iter()
            .filter(|fs| {
                let d = ra_dec_to_eq(fs[0], fs[1]);
                d[0] * gc[0] + d[1] * gc[1] + d[2] * gc[2] > 0.0
            })
            .count();
        let halo_frac = toward_galaxy_stars as f32 / halo_bd.field_stars.len().max(1) as f32;
        assert!(
            halo_frac > 0.58,
            "Expected Galactic Halo sky to skew toward host galaxy hemisphere: halo_frac={:.3}",
            halo_frac
        );

        // Also check disk concentration along galactic plane for SpiralArm (`|sin(b)| < 0.35`):
        let disk_bd = disk_consts[0].backdrop.as_ref().unwrap();
        let gp = disk_bd.galactic_pole;
        let near_plane_frac = disk_bd
            .field_stars
            .iter()
            .filter(|fs| {
                let d = ra_dec_to_eq(fs[0], fs[1]);
                (d[0] * gp[0] + d[1] * gp[1] + d[2] * gp[2]).abs() < 0.35
            })
            .count() as f32
            / disk_bd.field_stars.len().max(1) as f32;
        // Isotropic uniform sphere has `P(|sin(b)| < 0.35) == 0.35`; disk concentration boosts this significantly:
        assert!(
            near_plane_frac > 0.44,
            "Expected SpiralArm bright field stars to concentrate toward galactic plane (uniform=0.350, got {:.3})",
            near_plane_frac
        );
    }

    #[test]
    fn test_sun_sandbox_dual_hemisphere_star_map_data() {
        let mut sb = SunSandbox::new(42);
        let stars_buf = sb.star_map_stars_data();
        assert_eq!(stars_buf.len(), sb.star_count() * 6);
        let edges_buf = sb.star_map_constellation_edges();
        assert!(edges_buf.len() >= 30 && edges_buf.len() % 3 == 0);
        let meta = sb.star_map_metadata_json();
        assert!(meta.contains("\"constellations\":["));
        assert!(meta.contains("\"galacticPole\":["));

        // Verify real Hipparcos Sol places Polaris (HIP 11767) in the Northern Hemisphere (> +88°)
        // and Sirius (HIP 32349) in the Southern Hemisphere (< -15°), with constellations in both hemispheres:
        sb.load_real_hipparcos_sol();
        let hip_buf = sb.star_map_stars_data();
        assert_eq!(hip_buf.len(), 5179 * 6);
        let polaris_idx = sb
            .stars()
            .iter()
            .position(|s| s.id == 11767)
            .expect("Expected Polaris (HIP 11767) in Hipparcos catalog");
        let polaris_dec = hip_buf[polaris_idx * 6 + 1];
        assert!(
            polaris_dec > 88.0,
            "Expected Polaris in deep Northern Hemisphere, got dec = {polaris_dec}"
        );

        let sirius_idx = sb
            .stars()
            .iter()
            .position(|s| s.id == 32349)
            .expect("Expected Sirius (HIP 32349) in Hipparcos catalog");
        let sirius_dec = hip_buf[sirius_idx * 6 + 1];
        assert!(
            sirius_dec < -15.0 && sirius_dec > -18.0,
            "Expected Sirius in Southern Hemisphere (~-16.7°), got dec = {sirius_dec}"
        );

        let north_consts = sb
            .constellations()
            .iter()
            .filter(|c| c.declination_deg >= 0.0)
            .count();
        let south_consts = sb
            .constellations()
            .iter()
            .filter(|c| c.declination_deg < 0.0)
            .count();
        assert!(north_consts >= 6 && south_consts >= 6);

        let hit = sb
            .find_nearest_star_ra_dec(hip_buf[polaris_idx * 6] as f64, polaris_dec as f64)
            .expect("Expected star hit near Polaris");
        assert_eq!(hit, polaris_idx);
        let report = sb.inspect_star_index(hit);
        assert!(report.contains("HIP 11767"));
        assert!(report.contains("Northern Celestial Hemisphere"));
    }
}
