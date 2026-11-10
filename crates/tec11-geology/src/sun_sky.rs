use std::f64::consts::PI;

use tec11_math_grid::{cross, dot, length, normal, rotate, Pcg64Mcg, PointTree, SphericalGrid, Vec3};

const HIST_PGM_BYTES: &[u8] = include_bytes!("../data/hist.pgm");
const HIPPARCOS_TSV: &str = include_str!("../data/hipparcos_naked.tsv");

/// Morphological classification of the generated host galaxy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalaxyType {
    /// Grand-design or multi-arm spiral galaxy (`Sb / Sc`, Milky Way-like).
    Spiral = 0,
    /// Barred spiral galaxy (`SBb / SBc`) with a prominent central stellar bar.
    BarredSpiral = 1,
    /// Smooth lenticular disk galaxy (`S0`) with prominent bulge and no spiral arms.
    Lenticular = 2,
    /// Triaxial spheroidal elliptical galaxy (`E0-E5`) dominated by older K/M stars and giants.
    Elliptical = 3,
    /// Clumpy Magellanic / starburst irregular galaxy (`Irr`) rich in OB associations.
    Irregular = 4,
    /// Dense globular cluster orbiting in a galactic halo (`GC`).
    GlobularCluster = 5,
}

impl GalaxyType {
    pub fn from_id(id: u32) -> Self {
        match id {
            0 => Self::Spiral,
            1 => Self::BarredSpiral,
            2 => Self::Lenticular,
            3 => Self::Elliptical,
            4 => Self::Irregular,
            _ => Self::GlobularCluster,
        }
    }

    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Spiral => "Spiral Galaxy (Sb/Sc)",
            Self::BarredSpiral => "Barred Spiral Galaxy (SBb)",
            Self::Lenticular => "Lenticular Disk Galaxy (S0)",
            Self::Elliptical => "Elliptical Galaxy (E2)",
            Self::Irregular => "Irregular Starburst Galaxy (Irr)",
            Self::GlobularCluster => "Globular Cluster (Halo GC)",
        }
    }
}

/// Vantage region of the Home Star within the host galaxy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalacticVantage {
    /// Inside an active star-forming spiral arm or OB association.
    SpiralArm = 0,
    /// In the smooth stellar disk between spiral arms.
    InterArmDisk = 1,
    /// Deep inside the dense galactic bulge or central bar.
    GalacticBulge = 2,
    /// High above the galactic plane in the stellar halo (galaxy visible in one hemisphere).
    GalacticHalo = 3,
    /// Inside the triaxial spheroid of an elliptical galaxy or cluster core.
    SpheroidCore = 4,
}

impl GalacticVantage {
    pub fn from_id(id: u32) -> Self {
        match id {
            0 => Self::SpiralArm,
            1 => Self::InterArmDisk,
            2 => Self::GalacticBulge,
            3 => Self::GalacticHalo,
            _ => Self::SpheroidCore,
        }
    }

    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::SpiralArm => "Spiral Arm (Star-Forming Disk)",
            Self::InterArmDisk => "Inter-Arm Thin Disk",
            Self::GalacticBulge => "Galactic Bulge / Central Bar",
            Self::GalacticHalo => "Galactic Halo (High Above Disk)",
            Self::SpheroidCore => "Triaxial Spheroid / Cluster Interior",
        }
    }
}

/// Properties of the planet's chosen Home Star ("Sun").
#[derive(Clone, Debug)]
pub struct HomeStar {
    pub spectral_type: String,
    pub class_description: String,
    pub bv_color: f32,
    pub rgb: [f32; 3],
    pub temp_k: f32,
    pub mass_solar: f32,
    pub radius_solar: f32,
    pub luminosity_solar: f32,
    pub habitable_au: f32,
    pub orbital_period_days: f32,
    pub angular_diameter_deg: f32,
    pub solar_tide_factor: f32,
    pub age_gyr: f32,
    pub lifespan_gyr: f32,
    pub gal_pos_kpc: [f32; 3],
}

impl Default for HomeStar {
    fn default() -> Self {
        Self::sol()
    }
}

impl HomeStar {
    /// Standard Earth Sun (`Sol`, `G2V`, `B-V = 0.65`, `T = 5778 K`, `1.0 R_sun`, `R_gal = 8.1 kpc`).
    pub fn sol() -> Self {
        let bv = 0.65f32;
        Self {
            spectral_type: "G2V".to_string(),
            class_description: "Yellow Main-Sequence Dwarf".to_string(),
            bv_color: bv,
            rgb: bv_to_rgb(bv),
            temp_k: 5778.0,
            mass_solar: 1.0,
            radius_solar: 1.0,
            luminosity_solar: 1.0,
            habitable_au: 1.0,
            orbital_period_days: 365.25,
            angular_diameter_deg: 0.533,
            solar_tide_factor: 1.0,
            age_gyr: 4.6,
            lifespan_gyr: 10.0,
            gal_pos_kpc: [8.1, 0.0, 0.02],
        }
    }
}

/// A single naked-eye visible star on the celestial sphere (`star.py` in `Celestiality`).
#[derive(Clone, Debug)]
pub struct CelestialStar {
    /// Catalog ID (`HIP` number for Hipparcos stars, or `1..=N` for generated stars).
    pub id: u32,
    /// Unit direction vector `(x, y, z)` on the rotated celestial sphere.
    pub dir: Vec3,
    /// Distance from the Home Star in light-years (`r` in `star.py`).
    pub distance_ly: f32,
    /// `B-V` color index (`color` in `star.py`).
    pub bv_color: f32,
    /// Absolute visual magnitude `M_V` (`magnitude` in `star.py`).
    pub abs_mag: f32,
    /// Apparent visual magnitude `m_V` as seen from the planet (`<= 6.2`).
    pub app_mag: f32,
    /// Angular distance in radians from the galactic plane (`offset` in `star.py`).
    pub galactic_offset_rad: f32,
    /// True RGB color modulated by apparent brightness (`starmap.py:443-456`).
    pub rgb: [f32; 3],
    /// Pure chromatic RGB color (unscaled by magnitude).
    pub base_rgb: [f32; 3],
}

/// Convert `B-V` color index to `[r, g, b]` in `[0.0, 1.0]` (`starmap.py:443-450` in `Celestiality`).
pub fn bv_to_rgb(bv: f32) -> [f32; 3] {
    if bv < 0.25 {
        let s = ((bv + 0.5) / 0.75).clamp(0.35, 1.0);
        [s, s, 1.0]
    } else if bv < 1.0 {
        let b = ((1.0 - bv) / 0.75).clamp(0.0, 1.0);
        [1.0, 1.0, b]
    } else {
        let g = (2.0 - bv.min(2.0)).clamp(0.15, 1.0);
        [1.0, g, 0.0]
    }
}

/// Effective stellar temperature in Kelvin from `B-V` color index (Ballesteros' formula).
pub fn bv_to_temp_k(bv: f32) -> f32 {
    let b = bv.clamp(-0.35, 2.5);
    4600.0 * (1.0 / (0.92 * b + 1.7) + 1.0 / (0.92 * b + 0.62))
}

/// Format spectral classification (`O/B/A/F/G/K/M` + subclass `0..9` + luminosity class `I/III/V`)
/// from `(B-V, M_V)`.
pub fn classify_star(bv: f32, abs_mag: f32) -> (&'static str, u32, &'static str, &'static str) {
    let (letter, sub, base_desc) = if bv < -0.22 {
        ("O", ((bv + 0.35) / 0.13 * 9.0).clamp(3.0, 9.0) as u32, "Blue Star")
    } else if bv < 0.0 {
        ("B", ((bv + 0.22) / 0.22 * 9.0).clamp(0.0, 9.0) as u32, "Blue-White Star")
    } else if bv < 0.30 {
        ("A", (bv / 0.30 * 9.0).clamp(0.0, 9.0) as u32, "White Star")
    } else if bv < 0.58 {
        ("F", ((bv - 0.30) / 0.28 * 9.0).clamp(0.0, 9.0) as u32, "Yellow-White Star")
    } else if bv < 0.81 {
        ("G", ((bv - 0.58) / 0.23 * 9.0).clamp(0.0, 9.0) as u32, "Yellow Star")
    } else if bv < 1.40 {
        ("K", ((bv - 0.81) / 0.59 * 9.0).clamp(0.0, 9.0) as u32, "Orange Star")
    } else {
        ("M", ((bv - 1.40) / 0.80 * 9.0).clamp(0.0, 9.0) as u32, "Red Star")
    };

    // Distinguish Supergiants (I), Giants (III), Subgiants (IV), and Main Sequence Dwarfs (V):
    let ms_expected_mv = -1.5 + 6.2 * bv;
    let (lum_class, lum_desc) = if abs_mag < -3.8 {
        ("I", "Supergiant")
    } else if bv > 0.55 && abs_mag < ms_expected_mv - 2.2 {
        ("III", "Giant")
    } else if bv > 0.45 && abs_mag < ms_expected_mv - 1.0 {
        ("IV", "Subgiant")
    } else {
        ("V", "Main-Sequence Dwarf")
    };

    (letter, sub, lum_class, if lum_class == "V" { base_desc } else { lum_desc })
}

/// Parse the 100x100 `hist.pgm` smoothed 2D `(B-V, M_V)` histogram from `tps12/Celestiality`.
fn parse_hist_pgm() -> (usize, usize, &'static [u8]) {
    // Skip 4 newline-terminated header lines (`P5`, `# CREATOR...`, `100 100`, `255`):
    let mut newlines = 0;
    let mut start = 0;
    for (i, &b) in HIST_PGM_BYTES.iter().enumerate() {
        if b == b'\n' {
            newlines += 1;
            if newlines == 4 {
                start = i + 1;
                break;
            }
        }
    }
    let pixels = &HIST_PGM_BYTES[start..start + 10_000];
    (100, 100, pixels)
}

/// Interactive Sun, Plausible Galaxy & Celestial Sphere Star Map model (`SunSandbox`).
pub struct SunSandbox {
    grid: SphericalGrid,
    tree: PointTree,
    galaxy_type: GalaxyType,
    vantage: GalacticVantage,
    star_class_filter: u32,
    is_real_hipparcos: bool,
    home_star: HomeStar,
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
        let mut sb = Self {
            grid,
            tree,
            galaxy_type: GalaxyType::Spiral,
            vantage: GalacticVantage::SpiralArm,
            star_class_filter: 0,
            is_real_hipparcos: false,
            home_star: HomeStar::sol(),
            stars: Vec::with_capacity(6000),
            galactic_pole: [0.0, 0.0, 1.0],
            galactic_center_dir: [1.0, 0.0, 0.0],
            ecliptic_pole: [0.0, 0.39, 0.92],
            sun_dir: [1.0, 0.0, 0.0],
            constellation_edges: Vec::new(),
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

        // 3. Build 3D galaxy visualization particle cloud:
        self.build_galaxy_particles();

        // 4. Generate 6,000 surrounding naked-eye stars in the planet's sky:
        self.generate_surrounding_starmap(6000);
        self.build_constellation_edges();
    }

    /// Load the real ESA Hipparcos naked-eye star catalogue (`5,179` stars around Sol, `starmap.py r`).
    pub fn load_real_hipparcos_sol(&mut self) {
        self.is_real_hipparcos = true;
        self.galaxy_type = GalaxyType::Spiral;
        self.vantage = GalacticVantage::SpiralArm;
        self.home_star = HomeStar::sol();
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

            // Exact spherical coordinate conversion from `hipparcos.py:624-630`:
            let r_ly = (3.26 / pi_arcsec.max(1e-5)) as f32;
            let theta = (delta_deg + 90.0) * PI / 180.0;
            let phi = -alpha_deg * PI / 180.0;
            let sin_th = theta.sin();
            let dir = normal([sin_th * phi.cos(), sin_th * phi.sin(), theta.cos()]);
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
        // Choose spectral class:
        // 0 = Auto (Habitable Main Sequence F/G/K/M weighted by galactic population)
        // 1 = F-Class Yellow-White (B-V 0.32..0.56)
        // 2 = G-Class Yellow Sol-like (B-V 0.58..0.80)
        // 3 = K-Class Orange Dwarf (B-V 0.82..1.35)
        // 4 = M-Class Red Dwarf (B-V 1.38..1.72)
        // 5 = A-Class Blue-White Dwarf (B-V 0.02..0.28)
        // 6 = Evolved Subgiant / Giant (B-V 0.75..1.25, M_V +0.5..+2.8)
        let chosen_class = if (1..=6).contains(&star_filter) {
            star_filter
        } else {
            // Older stellar populations (Elliptical, Lenticular, Bulge, GlobularCluster)
            // favor K/G/M stars over shorter-lived F/A stars:
            let is_old_pop = matches!(
                self.galaxy_type,
                GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
            ) || self.vantage == GalacticVantage::GalacticBulge;
            let p = self.rng.next_f64();
            if is_old_pop {
                if p < 0.06 {
                    1 // F
                } else if p < 0.38 {
                    2 // G
                } else if p < 0.78 {
                    3 // K
                } else if p < 0.94 {
                    4 // M
                } else {
                    6 // Subgiant
                }
            } else if p < 0.04 {
                5 // A
            } else if p < 0.20 {
                1 // F
            } else if p < 0.56 {
                2 // G
            } else if p < 0.86 {
                3 // K
            } else {
                4 // M
            }
        };

        let (bv, mass_solar, radius_solar, is_subgiant) = match chosen_class {
            1 => {
                // F-Class Yellow-White Main-Sequence Star
                let t = self.rng.uniform(0.0, 1.0) as f32;
                let bv = 0.32 + 0.24 * t;
                let mass = 1.50 - 0.42 * t;
                let radius = 1.45 - 0.35 * t;
                (bv, mass, radius, false)
            }
            2 => {
                // G-Class Yellow Sol-like Main-Sequence Star
                let t = self.rng.uniform(0.0, 1.0) as f32;
                let bv = 0.58 + 0.22 * t;
                let mass = 1.08 - 0.22 * t;
                let radius = 1.10 - 0.22 * t;
                (bv, mass, radius, false)
            }
            3 => {
                // K-Class Orange Main-Sequence Dwarf
                let t = self.rng.uniform(0.0, 1.0) as f32;
                let bv = 0.82 + 0.52 * t;
                let mass = 0.84 - 0.28 * t;
                let radius = 0.86 - 0.24 * t;
                (bv, mass, radius, false)
            }
            4 => {
                // M-Class Red Main-Sequence Dwarf
                let t = self.rng.uniform(0.0, 1.0) as f32;
                let bv = 1.38 + 0.32 * t;
                let mass = 0.52 - 0.30 * t;
                let radius = 0.56 - 0.30 * t;
                (bv, mass, radius, false)
            }
            5 => {
                // A-Class White Main-Sequence Star
                let t = self.rng.uniform(0.0, 1.0) as f32;
                let bv = 0.02 + 0.26 * t;
                let mass = 2.10 - 0.55 * t;
                let radius = 1.85 - 0.38 * t;
                (bv, mass, radius, false)
            }
            _ => {
                // Evolved Subgiant / Early Giant (IV)
                let t = self.rng.uniform(0.0, 1.0) as f32;
                let bv = 0.72 + 0.45 * t;
                let mass = 1.15 + 0.35 * (1.0 - t);
                let radius = 2.10 + 1.10 * t;
                (bv, mass, radius, true)
            }
        };

        let temp_k = bv_to_temp_k(bv);
        let rgb = bv_to_rgb(bv);
        let temp_ratio = temp_k / 5778.0;
        let luminosity_solar = (radius_solar * radius_solar) * temp_ratio.powi(4);
        let habitable_au = luminosity_solar.sqrt().max(0.04);
        let orbital_period_days =
            365.25 * (habitable_au.powi(3) / mass_solar.max(0.08)).sqrt();
        let angular_diameter_deg = 0.533 * (radius_solar / habitable_au);
        let solar_tide_factor = (mass_solar / habitable_au.powi(3)).clamp(0.05, 50.0);

        let lifespan_gyr = (10.0 * (mass_solar as f64).powf(-2.5)).clamp(0.6, 80.0) as f32;
        let age_gyr = (lifespan_gyr * (self.rng.uniform(0.25, 0.75) as f32)).clamp(0.4, 12.5);

        let abs_mag_est = 4.83 - 2.5 * luminosity_solar.max(1e-4).log10();
        let (letter, sub, mut lum_cls, _) = classify_star(bv, abs_mag_est);
        if is_subgiant {
            lum_cls = "IV";
        }
        let spectral_type = format!("{letter}{sub}{lum_cls}");
        let class_description = match (letter, is_subgiant) {
            (_, true) => "Orange-Gold Evolved Subgiant",
            ("A", _) => "Blue-White Main-Sequence Star",
            ("F", _) => "Yellow-White Main-Sequence Dwarf",
            ("G", _) => "Yellow Main-Sequence Dwarf",
            ("K", _) => "Orange Main-Sequence Dwarf",
            ("M", _) => "Red Main-Sequence Dwarf",
            _ => "Main-Sequence Star",
        }
        .to_string();

        HomeStar {
            spectral_type,
            class_description,
            bv_color: bv,
            rgb,
            temp_k,
            mass_solar,
            radius_solar,
            luminosity_solar,
            habitable_au,
            orbital_period_days,
            angular_diameter_deg,
            solar_tide_factor,
            age_gyr,
            lifespan_gyr,
            gal_pos_kpc,
        }
    }

    /// Generate `count` naked-eye visible stars (`m_app <= 6.0`) around `self.home_star`,
    /// generalizing `RandomStars` (`randomstars.py`) to arbitrary galaxy morphologies and vantages.
    fn generate_surrounding_starmap(&mut self, count: usize) {
        let (w, h, hist) = parse_hist_pgm();
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

            // 2. Sample distance within naked-eye visibility limit (`randomstars.py:37`):
            let max_r_ly = (3.26 / 10.0_f64.powf(((m as f64) - 6.0) / 5.0 - 1.0)).max(1.2);
            let dist_scale = if self.galaxy_type == GalaxyType::GlobularCluster
                || self.vantage == GalacticVantage::GalacticBulge
            {
                // Dense bulge / globular cluster environments pack stars closer together, brightening the night sky:
                0.68
            } else {
                1.0
            };
            let r_ly = (self.rng.uniform(1.0, max_r_ly) * dist_scale).max(1.0) as f32;

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
            let app_mag = m - 5.0 * (((r_ly as f64) / 3.26).log10() as f32 + 1.0);
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

    /// Build automatic constellation / asterism lines connecting neighboring bright stars (`app_mag <= 2.75`).
    fn build_constellation_edges(&mut self) {
        self.constellation_edges.clear();
        let mut bright_indices: Vec<usize> = self
            .stars
            .iter()
            .enumerate()
            .filter(|(_, s)| s.app_mag <= 2.85)
            .map(|(i, _)| i)
            .collect();
        bright_indices.sort_by(|&a, &b| {
            self.stars[a]
                .app_mag
                .partial_cmp(&self.stars[b].app_mag)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        bright_indices.truncate(220);

        let mut degree = vec![0u8; bright_indices.len()];
        for i in 0..bright_indices.len() {
            if degree[i] >= 3 {
                continue;
            }
            let ia = bright_indices[i];
            let pa = self.stars[ia].dir;
            let mut best: Option<(usize, f64)> = None;
            for j in (i + 1)..bright_indices.len() {
                if degree[j] >= 3 {
                    continue;
                }
                let ib = bright_indices[j];
                let pb = self.stars[ib].dir;
                let cos_d = dot(pa, pb);
                // Connect stars separated by ~3.5 deg to ~18 deg on the celestial sphere:
                if (0.950..=0.998).contains(&cos_d) {
                    if best.map_or(true, |(_, b_cos)| cos_d > b_cos) {
                        best = Some((j, cos_d));
                    }
                }
            }
            if let Some((j, _)) = best {
                degree[i] += 1;
                degree[j] += 1;
                self.constellation_edges
                    .push((bright_indices[i], bright_indices[j]));
            }
        }
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
            self.galaxy_particles.push([
                (pos_kpc[0] as f32 * scale).clamp(-0.92, 0.92),
                (pos_kpc[2] as f32 * scale).clamp(-0.92, 0.92),
                (pos_kpc[1] as f32 * scale).clamp(-0.92, 0.92),
                rgb[0],
                rgb[1],
                rgb[2],
            ]);
        }
    }

    /// Flat buffer `[x, y, z, r, g, b, size_px, ...]` for all 3D star points in the current aspect:
    /// - `"night_sky"`: All ~6,000 naked-eye stars on the unit celestial sphere (`r = 1.006`), plus a bright marker for the Home Sun if `show_ecliptic` is enabled.
    /// - `"home_sun"`: Background starfield on a slightly larger radius (`r = 1.04`).
    /// - `"galaxy"`: 1,800 3D galaxy structure stars + bright reticle points at the Home Star's galactocentric position.
    pub fn star_points_data(&self, aspect: &str) -> Vec<f32> {
        match aspect {
            "galaxy" => {
                let mut out = Vec::with_capacity((self.galaxy_particles.len() + 1) * 7);
                for p in &self.galaxy_particles {
                    out.extend_from_slice(&[p[0], p[1], p[2], p[3], p[4], p[5], 2.4]);
                }
                // Home Star position marker in the galaxy:
                let max_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
                    0.020
                } else {
                    15.0
                };
                let scale = 0.85 / max_kpc;
                let gp = self.home_star.gal_pos_kpc;
                let hx = (gp[0] * scale).clamp(-0.90, 0.90);
                let hy = (gp[2] * scale).clamp(-0.90, 0.90);
                let hz = (gp[1] * scale).clamp(-0.90, 0.90);
                out.extend_from_slice(&[hx, hy, hz, 0.15, 1.0, 0.45, 7.5]);
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

    /// 3D line segments `[x0, y0, z0, x1, y1, z1, ...]` for constellation asterisms and/or galactic plane & ecliptic circles.
    pub fn overlay_line_segments(
        &self,
        aspect: &str,
        show_galactic_plane: bool,
        show_constellations: bool,
    ) -> Vec<f32> {
        let mut lines = Vec::new();
        let rad = 1.004f32;

        if aspect == "galaxy" {
            // Draw galactocentric crosshair & orbit circle through the Home Star's galactic position:
            let max_kpc = if self.galaxy_type == GalaxyType::GlobularCluster {
                0.020
            } else {
                15.0
            };
            let scale = 0.85 / max_kpc;
            let gp = self.home_star.gal_pos_kpc;
            let hx = (gp[0] * scale).clamp(-0.90, 0.90);
            let hy = (gp[2] * scale).clamp(-0.90, 0.90);
            let hz = (gp[1] * scale).clamp(-0.90, 0.90);
            let d = 0.08f32;
            lines.extend_from_slice(&[hx - d, hy, hz, hx + d, hy, hz]);
            lines.extend_from_slice(&[hx, hy - d, hz, hx, hy + d, hz]);
            lines.extend_from_slice(&[hx, hy, hz - d, hx, hy, hz + d]);

            let r_orb = (hx * hx + hz * hz).sqrt();
            if r_orb > 0.04 {
                let segs = 48;
                for k in 0..segs {
                    let a0 = (k as f32) * 2.0 * (PI as f32) / (segs as f32);
                    let a1 = ((k + 1) as f32) * 2.0 * (PI as f32) / (segs as f32);
                    lines.extend_from_slice(&[
                        r_orb * a0.cos(),
                        hy,
                        r_orb * a0.sin(),
                        r_orb * a1.cos(),
                        hy,
                        r_orb * a1.sin(),
                    ]);
                }
            }
            return lines;
        }

        if aspect == "home_sun" {
            // Reference 1.0 R_sun ring around the sphere equator when comparing star size:
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

    /// Background sphere RGB colors (`7,292` tiles) for the 3 visualization aspects:
    /// - `"night_sky"`: Deep-space celestial sphere with diffuse Milky Way / galactic core glow and bright star projection.
    /// - `"home_sun"`: The chosen Home Star rendered as a 3D photosphere with limb darkening, convective granulation, and its true `B-V` blackbody chromaticity.
    /// - `"galaxy"`: Dark translucent celestial boundary enclosing the 3D galaxy particle cloud.
    pub fn tile_colors_rgb(
        &self,
        aspect: &str,
        show_galactic_plane: bool,
        selected_tile: Option<usize>,
    ) -> Vec<f32> {
        let n = self.grid.faces.len();
        let mut out = Vec::with_capacity(n * 3);

        // Pre-bin the brightest star on each coarse tile so even thesphere surface shows starlight:
        let mut tile_star_rgb = vec![[0.0f32; 3]; n];
        if aspect == "night_sky" {
            for s in &self.stars {
                let tid = self.tree.nearest(s.dir);
                let cur = tile_star_rgb[tid];
                let s_lum = s.rgb[0] + s.rgb[1] + s.rgb[2];
                let c_lum = cur[0] + cur[1] + cur[2];
                if s_lum > c_lum {
                    tile_star_rgb[tid] = s.rgb;
                }
            }
        }

        let selected_star_tile = selected_tile.and_then(|sel| {
            self.nearest_star_to_tile(sel)
                .map(|idx| self.tree.nearest(self.stars[idx].dir))
        });

        for v in 0..n {
            let p = self.grid.faces[v].0;
            let mut rgb = match aspect {
                "home_sun" => {
                    // Render the Home Star's 3D photosphere with convective cells, limb glow, and B-V color:
                    let base = self.home_star.rgb;
                    let gran1 = ((p[0] * 18.0).sin() * (p[1] * 18.0).cos() * (p[2] * 18.0).sin()) as f32;
                    let gran2 = ((p[0] * 37.0 + 1.3).cos() * (p[2] * 37.0 - 0.7).sin()) as f32;
                    let lat = p[2].abs() as f32;
                    // Active magnetic sunspot belt at mid-latitudes (`|z| in 0.15..0.45`):
                    let spot_val = ((p[0] * 9.5 + 0.4).sin() * (p[1] * 11.0 - 0.8).cos()) as f32;
                    let is_spot = (0.12..=0.48).contains(&lat) && spot_val > 0.88;
                    let mod_f = if is_spot {
                        0.42
                    } else {
                        (0.92 + 0.08 * gran1 + 0.05 * gran2).clamp(0.78, 1.12)
                    };
                    [
                        (base[0] * mod_f).clamp(0.0, 1.0),
                        (base[1] * mod_f).clamp(0.0, 1.0),
                        (base[2] * mod_f).clamp(0.0, 1.0),
                    ]
                }
                "galaxy" => {
                    // Subtle deep-space background for the 3D galaxy cloud:
                    [0.02, 0.025, 0.045]
                }
                _ => {
                    // "night_sky": Celestial sphere with subtle diffuse galactic band / core glow:
                    let sin_b = dot(p, self.galactic_pole);
                    let cos_core = dot(p, self.galactic_center_dir);
                    let gal_glow = match (self.galaxy_type, self.vantage) {
                        (
                            GalaxyType::Spiral | GalaxyType::BarredSpiral | GalaxyType::Lenticular,
                            GalacticVantage::SpiralArm | GalacticVantage::InterArmDisk,
                        ) => {
                            let band = (-((sin_b / 0.18).powi(2))).exp();
                            let bulge = (cos_core.max(0.0).powi(4)) * (-((sin_b / 0.32).powi(2))).exp();
                            (0.09 * band + 0.11 * bulge) as f32
                        }
                        (_, GalacticVantage::GalacticHalo) => {
                            let disk_hemisphere = ((-sin_b).max(0.0)).powf(1.5) * (0.4 + 0.6 * cos_core.max(0.0));
                            (0.14 * disk_hemisphere) as f32
                        }
                        _ => {
                            let core_hemisphere = ((cos_core + 0.35).max(0.0) / 1.35).powi(2);
                            (0.12 * core_hemisphere) as f32
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
                        0.012 + gal_glow * glow_tint[0],
                        0.015 + gal_glow * glow_tint[1],
                        0.028 + gal_glow * glow_tint[2],
                    ];

                    if show_galactic_plane && sin_b.abs() < 0.022 {
                        bg[0] = (bg[0] + 0.08).min(1.0);
                        bg[1] = (bg[1] + 0.14).min(1.0);
                        bg[2] = (bg[2] + 0.25).min(1.0);
                    }

                    // Blend any star landing on this tile:
                    let sr = tile_star_rgb[v];
                    [
                        bg[0].max(sr[0] * 0.55),
                        bg[1].max(sr[1] * 0.55),
                        bg[2].max(sr[2] * 0.55),
                    ]
                }
            };

            if selected_star_tile == Some(v) || selected_tile == Some(v) {
                rgb = [0.20, 0.95, 0.55];
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

    /// One-line summary of the generated galaxy and chosen Home Star.
    pub fn summary_line(&self) -> String {
        let hs = &self.home_star;
        let r_gal = (hs.gal_pos_kpc[0] * hs.gal_pos_kpc[0]
            + hs.gal_pos_kpc[1] * hs.gal_pos_kpc[1]
            + hs.gal_pos_kpc[2] * hs.gal_pos_kpc[2])
            .sqrt();
        format!(
            "Sun: {} ({:.0} K, B-V {:+.2}, {:.2} R☉, {:.2} L☉) | HZ: {:.2} AU ({:.0}d yr, {:.2}° sky disc) | {} — {} (R={:.1} kpc)",
            hs.spectral_type,
            hs.temp_k,
            hs.bv_color,
            hs.radius_solar,
            hs.luminosity_solar,
            hs.habitable_au,
            hs.orbital_period_days,
            hs.angular_diameter_deg,
            self.galaxy_type.name(),
            self.vantage.name(),
            r_gal
        )
    }

    /// Detailed multi-line inspection of the selected star on the celestial sphere AND the Home Sun.
    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        let hs = &self.home_star;
        let Some(star_idx) = self.nearest_star_to_tile(tile_idx) else {
            return String::new();
        };
        let st = &self.stars[star_idx];
        let (letter, sub, lum_cls, lum_desc) = classify_star(st.bv_color, st.abs_mag);
        let st_temp = bv_to_temp_k(st.bv_color);
        let st_lum = 10.0_f32.powf((4.83 - st.abs_mag) / 2.5);
        let gal_lat_deg = (90.0 - (st.galactic_offset_rad as f64 * 180.0 / PI)).abs();
        let cat_prefix = if self.is_real_hipparcos { "HIP" } else { "Star #" };

        format!(
            "Selected Night-Sky Star: {cat_prefix}{} — {letter}{sub}{lum_cls} ({lum_desc})\n\
             Apparent Magnitude: m_V = {:+.2} | Absolute Magnitude: M_V = {:+.2} ({:.1} L☉)\n\
             Distance: {:.1} light-years ({:.1} pc) | Color Index: B-V = {:+.2} (T_eff ≈ {:.0} K) | Offset from Galactic Plane: {:.1}°\n\
             Home Sun: {} ({}) — {:.0} K, {:.2} M☉, {:.2} R☉, {:.2} L☉ | Habitable Orbit: {:.2} AU ({:.0} Earth days/yr, Solar Tides {:.2}x)",
            st.id,
            st.app_mag,
            st.abs_mag,
            st_lum,
            st.distance_ly,
            st.distance_ly / 3.26156,
            st.bv_color,
            st_temp,
            gal_lat_deg,
            hs.spectral_type,
            hs.class_description,
            hs.temp_k,
            hs.mass_solar,
            hs.radius_solar,
            hs.luminosity_solar,
            hs.habitable_au,
            hs.orbital_period_days,
            hs.solar_tide_factor
        )
    }
}

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
}
