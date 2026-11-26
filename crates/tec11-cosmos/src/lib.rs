use std::f64::consts::PI;

use tec11_rng::Pcg64Mcg;
use tec11_vec3::{cross, dot, normal, rotate, Vec3};

pub const HIST_PGM_BYTES: &[u8] = include_bytes!("../data/hist.pgm");
pub const HIPPARCOS_TSV: &str = include_str!("../data/hipparcos_naked.tsv");

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

    /// Map this star's spectral type back to the discrete star filter ID (`1` = F, `2` = G, `3` = K, `4` = M, `5` = A, `6` = Subgiant).
    pub fn star_filter_id(&self) -> u32 {
        if self.spectral_type.ends_with("IV") {
            6
        } else {
            match self.spectral_type.as_bytes().first().copied() {
                Some(b'F') => 1,
                Some(b'G') => 2,
                Some(b'K') => 3,
                Some(b'M') => 4,
                Some(b'A') => 5,
                _ => 2,
            }
        }
    }

    pub fn to_proto(&self) -> tec11_proto::HomeStarMetadata {
        tec11_proto::HomeStarMetadata {
            spectral_type: self.spectral_type.clone(),
            class_description: self.class_description.clone(),
            bv_color: self.bv_color,
            rgb: Some(tec11_proto::Vec3 {
                x: self.rgb[0],
                y: self.rgb[1],
                z: self.rgb[2],
            }),
            temp_k: self.temp_k,
            mass_solar: self.mass_solar,
            radius_solar: self.radius_solar,
            luminosity_solar: self.luminosity_solar,
            habitable_au: self.habitable_au,
            orbital_period_days: self.orbital_period_days,
            angular_diameter_deg: self.angular_diameter_deg,
            solar_tide_factor: self.solar_tide_factor,
            age_gyr: self.age_gyr,
            lifespan_gyr: self.lifespan_gyr,
            gal_pos_kpc: Some(tec11_proto::Vec3 {
                x: self.gal_pos_kpc[0],
                y: self.gal_pos_kpc[1],
                z: self.gal_pos_kpc[2],
            }),
        }
    }

    pub fn from_proto(p: &tec11_proto::HomeStarMetadata) -> Self {
        let sol = Self::sol();
        let rgb = p
            .rgb
            .map(|v| [v.x.clamp(0.0, 1.0), v.y.clamp(0.0, 1.0), v.z.clamp(0.0, 1.0)])
            .unwrap_or_else(|| bv_to_rgb(p.bv_color));
        let gal = p
            .gal_pos_kpc
            .map(|v| [v.x, v.y, v.z])
            .unwrap_or(sol.gal_pos_kpc);
        Self {
            spectral_type: if p.spectral_type.is_empty() {
                sol.spectral_type
            } else {
                p.spectral_type.clone()
            },
            class_description: if p.class_description.is_empty() {
                sol.class_description
            } else {
                p.class_description.clone()
            },
            bv_color: p.bv_color,
            rgb,
            temp_k: if p.temp_k > 100.0 { p.temp_k } else { sol.temp_k },
            mass_solar: if p.mass_solar > 0.01 { p.mass_solar } else { sol.mass_solar },
            radius_solar: if p.radius_solar > 0.01 { p.radius_solar } else { sol.radius_solar },
            luminosity_solar: if p.luminosity_solar > 1e-5 {
                p.luminosity_solar
            } else {
                sol.luminosity_solar
            },
            habitable_au: if p.habitable_au > 0.005 {
                p.habitable_au
            } else {
                sol.habitable_au
            },
            orbital_period_days: if p.orbital_period_days > 1.0 {
                p.orbital_period_days
            } else {
                sol.orbital_period_days
            },
            angular_diameter_deg: if p.angular_diameter_deg > 0.01 {
                p.angular_diameter_deg
            } else {
                sol.angular_diameter_deg
            },
            solar_tide_factor: if p.solar_tide_factor > 0.0 {
                p.solar_tide_factor
            } else {
                sol.solar_tide_factor
            },
            age_gyr: if p.age_gyr > 0.0 { p.age_gyr } else { sol.age_gyr },
            lifespan_gyr: if p.lifespan_gyr > 0.0 {
                p.lifespan_gyr
            } else {
                sol.lifespan_gyr
            },
            gal_pos_kpc: gal,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StarClassification {
    pub spectral_letter: &'static str,
    pub subclass: u32,
    pub luminosity_class: &'static str,
    pub description: &'static str,
}

/// Format spectral classification (`O/B/A/F/G/K/M` + subclass `0..9` + luminosity class `I/III/V`)
/// from `(B-V, M_V)`.
pub fn classify_star(bv: f32, abs_mag: f32) -> StarClassification {
    let (spectral_letter, subclass, base_desc) = if bv < -0.22 {
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
    let (luminosity_class, lum_desc) = if abs_mag < -3.8 {
        ("I", "Supergiant")
    } else if bv > 0.55 && abs_mag < ms_expected_mv - 2.2 {
        ("III", "Giant")
    } else if bv > 0.45 && abs_mag < ms_expected_mv - 1.0 {
        ("IV", "Subgiant")
    } else {
        ("V", "Main-Sequence Dwarf")
    };

    StarClassification {
        spectral_letter,
        subclass,
        luminosity_class,
        description: if luminosity_class == "V" {
            base_desc
        } else {
            lum_desc
        },
    }
}

pub struct HistPgm {
    pub width: usize,
    pub height: usize,
    pub pixels: &'static [u8],
}

/// Parse the 100x100 `hist.pgm` smoothed 2D `(B-V, M_V)` histogram from `tps12/Celestiality`.
pub fn parse_hist_pgm() -> HistPgm {
    // Skip 4 newline-terminated header lines (`P5`, `# CREATOR...`, `100 100`, `255`):
    let start = HIST_PGM_BYTES
        .iter()
        .enumerate()
        .filter(|&(_, &b)| b == b'\n')
        .nth(3)
        .map(|(i, _)| i + 1)
        .unwrap_or(0);
    HistPgm {
        width: 100,
        height: 100,
        pixels: &HIST_PGM_BYTES[start..start + 10_000],
    }
}

/// Atmospheric gas & aerosol composition governing Rayleigh/Mie scattering, molecular absorption bands,
/// daytime sky color, sunset color, and the surface light spectrum available for photosynthesis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtmosphereType {
    /// Earth-like nitrogen-oxygen atmosphere with a stratospheric ozone ($\text{O}_3$) layer ($1.0\text{ bar}$).
    /// Clean $\lambda^{-4}$ Rayleigh scattering + Chappuis band yellow-orange absorption.
    EarthLikeN2O2 = 0,
    /// Methane-rich nitrogen atmosphere ($90\%\text{ N}_2, 8\%\text{ CH}_4, 2\%\text{ CO}_2/\text{H}_2$, $1.2\text{ bar}$).
    /// Methane strongly absorbs red & orange photons ($600\text{–}890\text{ nm}$), shifting skies to cyan-aquamarine and sunsets to emerald-teal.
    MethaneRich = 1,
    /// High-pressure carbon dioxide & nitrogen envelope ($92\%\text{ CO}_2, 8\%\text{ N}_2$, $4.5\text{ bar}$).
    /// Heavy Rayleigh optical depth extinguishes short blue wavelengths even at midday, yielding golden-cream skies and copper-crimson sunsets.
    DenseCO2 = 2,
    /// Photochemical hydrocarbon haze ($\text{N}_2\text{–CH}_4\text{–CO}_2$ + organic tholin aerosols, $1.5\text{ bar}$, Archean/Titan-like).
    /// Tholin aerosols absorb UV/blue light ($<520\text{ nm}$) and Mie-scatter warm amber/butterscotch hues.
    TholinHaze = 3,
    /// Thin carbon dioxide atmosphere with suspended iron-oxide ($\text{Fe}_2\text{O}_3$) silicate dust ($0.08\text{ bar}$, Mars-like).
    /// Dust absorbs blue light to create a butterscotch-salmon daytime sky while forward-scattering a cool blue-periwinkle halo at sunset.
    ThinDustyCO2 = 4,
    /// Hydrogen-helium-nitrogen envelope with water-vapor clouds ($\text{H}_2\text{–He–N}_2$, $2.5\text{ bar}$, Hycean / Super-Earth).
    /// Clean molecular $\text{H}_2$ Rayleigh scattering + mild $\text{H}_2\text{O}$ red absorption yields luminous periwinkle skies and rose-magenta sunsets.
    HydrogenRich = 5,
}

impl Default for AtmosphereType {
    fn default() -> Self {
        Self::EarthLikeN2O2
    }
}

impl AtmosphereType {
    pub fn from_id(id: u32) -> Self {
        match id {
            0 => Self::EarthLikeN2O2,
            1 => Self::MethaneRich,
            2 => Self::DenseCO2,
            3 => Self::TholinHaze,
            4 => Self::ThinDustyCO2,
            _ => Self::HydrogenRich,
        }
    }

    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::EarthLikeN2O2 => "N2-O2 + O3 Ozone (Earth-like, 1.0 bar)",
            Self::MethaneRich => "N2-CH4 Methane-Rich (Red-Absorbing, 1.2 bar)",
            Self::DenseCO2 => "Dense CO2-N2 (High-Pressure, 4.5 bar)",
            Self::TholinHaze => "N2-CH4 Tholin Haze (Organic Aerosols, 1.5 bar)",
            Self::ThinDustyCO2 => "Thin CO2 + Iron-Oxide Dust (Mars-like, 0.08 bar)",
            Self::HydrogenRich => "H2-He-N2 + H2O Vapor (Hycean, 2.5 bar)",
        }
    }

    pub fn short_formula(self) -> &'static str {
        match self {
            Self::EarthLikeN2O2 => "78% N2, 21% O2, O3 (1.0 bar)",
            Self::MethaneRich => "90% N2, 8% CH4, 2% CO2 (1.2 bar)",
            Self::DenseCO2 => "92% CO2, 8% N2 (4.5 bar)",
            Self::TholinHaze => "N2-CH4-CO2 + Tholin Haze (1.5 bar)",
            Self::ThinDustyCO2 => "95% CO2 + Fe2O3 Dust (0.08 bar)",
            Self::HydrogenRich => "H2-He-N2 + H2O Clouds (2.5 bar)",
        }
    }
}

/// Coupled radiative-transfer and astrobiological pigment model determining daytime sky color,
/// sunset/twilight colors, and main photosynthetic plant foliage colors from the Host Star (`HomeStar`)
/// and Atmospheric Gases (`AtmosphereType`).
#[derive(Clone, Debug)]
pub struct PlanetaryOptics {
    pub atmosphere: AtmosphereType,
    pub sun_rgb: [f32; 3],
    pub sky_zenith_rgb: [f32; 3],
    pub sky_horizon_rgb: [f32; 3],
    pub sky_color_name: String,
    pub sky_mechanism: String,
    pub sunset_rgb: [f32; 3],
    pub twilight_rgb: [f32; 3],
    pub sunset_color_name: String,
    pub sunset_mechanism: String,
    pub plant_foliage_rgb: [f32; 3],
    pub plant_highlight_rgb: [f32; 3],
    pub plant_color_name: String,
    pub plant_pigment_desc: String,
}

impl PlanetaryOptics {
    /// Compute sky, sunset, and main plant colors from `home_star` and `atmosphere`.
    pub fn compute(home_star: &HomeStar, atmosphere: AtmosphereType) -> Self {
        let temp_k = home_star.temp_k.clamp(2500.0, 12000.0);
        let sun_rgb = home_star.rgb;

        // 1. Incident stellar photon flux in Red (650 nm), Green (535 nm), and Blue (450 nm) bands:
        //    Blend normalized Planck blackbody irradiance with `home_star.rgb`:
        let planck_band = |lambda_nm: f32| -> f32 {
            let l_um = lambda_nm * 1e-3;
            let x = (14387.77 / (l_um * temp_k)).clamp(0.2, 50.0);
            1.0 / (l_um.powi(5) * (x.exp() - 1.0))
        };
        let pr = planck_band(650.0);
        let pg = planck_band(535.0);
        let pb = planck_band(450.0);
        let pmax = pr.max(pg).max(pb).max(1e-6);
        let i_star = [
            (0.55 * (pr / pmax) + 0.45 * sun_rgb[0]).clamp(0.08, 1.0),
            (0.55 * (pg / pmax) + 0.45 * sun_rgb[1]).clamp(0.08, 1.0),
            (0.55 * (pb / pmax) + 0.45 * sun_rgb[2]).clamp(0.04, 1.0),
        ];

        // 2. Atmospheric Rayleigh (lambda^-4), Mie aerosol, and specific gas absorption optical depths:
        //    Rayleigh cross-section ratio at [650nm, 535nm, 450nm] is [0.23, 0.50, 1.00].
        let (tau_rayleigh_b, tau_mie, tau_gas_abs, sky_mech, sunset_mech) = match atmosphere {
            AtmosphereType::EarthLikeN2O2 => (
                0.38f32,
                [0.04f32, 0.04, 0.04],
                // Stratospheric O3 Chappuis absorption in yellow-orange (550-650 nm):
                [0.055f32, 0.045, 0.005],
                "N2/O2 Rayleigh scattering (λ⁻⁴) + O3 Chappuis band",
                "High-airmass Rayleigh blue extinction + O3 twilight absorption",
            ),
            AtmosphereType::MethaneRich => (
                0.46f32,
                [0.05f32, 0.06, 0.06],
                // Strong CH4 vibrational absorption in red & orange (600-890 nm):
                [0.64f32, 0.08, 0.01],
                "N2 Rayleigh scattering + CH4 red/orange band absorption",
                "Long-path CH4 red stripping leaves emerald-teal/cyan dusk",
            ),
            AtmosphereType::DenseCO2 => (
                1.65f32,
                [0.24f32, 0.22, 0.18],
                // Thick 4.5-bar extinction strips short blue photons even at zenith:
                [0.02f32, 0.11, 0.48],
                "High-pressure 4.5-bar CO2 multiple scattering & blue extinction",
                "Extreme slant optical depth leaves only deep copper-red photons",
            ),
            AtmosphereType::TholinHaze => (
                0.34f32,
                [0.36f32, 0.28, 0.14],
                // Photochemical organic tholin haze strongly absorbs UV/blue (<520 nm):
                [0.04f32, 0.30, 0.96],
                "Organic tholin Mie haze absorbs blue/UV & scatters warm amber",
                "Hydrocarbon aerosol extinction yields smoldering carmine-sienna dusk",
            ),
            AtmosphereType::ThinDustyCO2 => (
                0.035f32,
                [0.42f32, 0.26, 0.12],
                // Suspended Fe2O3 iron-oxide dust absorbs blue & scatters butterscotch:
                [0.05f32, 0.28, 0.68],
                "Suspended Fe2O3 dust Mie scattering dominates thin 0.08-bar CO2",
                "Micron dust forward-scatters a cool blue halo around the setting sun",
            ),
            AtmosphereType::HydrogenRich => (
                0.56f32,
                [0.07f32, 0.07, 0.08],
                // Mild H2-H2 CIA and H2O vapor red absorption:
                [0.16f32, 0.03, 0.005],
                "Clean molecular H2/He Rayleigh scattering + H2O vapor clouds",
                "Deep H2 Rayleigh scattering with rose-magenta water-vapor glow",
            ),
        };

        let tau_r = [
            tau_rayleigh_b * 0.23,
            tau_rayleigh_b * 0.50,
            tau_rayleigh_b * 1.00,
        ];

        // 3. Daytime Sky Color (`sky_zenith_rgb` & `sky_horizon_rgb`):
        let mut sky_raw = [0.0f32; 3];
        let mut hor_raw = [0.0f32; 3];
        for c in 0..3 {
            let scat = 1.0 - (-(tau_r[c] + tau_mie[c])).exp();
            // Multiple-scattering self-extinction in thick atmospheres (`-0.42 * tau_r[c]`):
            let trans = (-tau_gas_abs[c] - 0.42 * tau_r[c]).exp();
            sky_raw[c] = i_star[c] * scat * trans;

            // Horizon (airmass ~ 2.6) has stronger multiple scattering & warm transmission:
            let scat_h = 1.0 - (-2.6 * (tau_r[c] + tau_mie[c])).exp();
            let trans_h = (-1.6 * tau_gas_abs[c] - 0.72 * tau_r[c]).exp();
            hor_raw[c] = i_star[c] * scat_h * trans_h;
        }
        let sky_max = sky_raw[0].max(sky_raw[1]).max(sky_raw[2]).max(1e-4);
        // Under very cool M-dwarfs, overall visual sky brightness is somewhat softer/dusky:
        let star_vis_gain = ((temp_k - 2200.0) / 2800.0).clamp(0.68, 0.98);
        let sky_zenith_rgb = [
            (sky_raw[0] / sky_max * star_vis_gain).clamp(0.06, 0.98),
            (sky_raw[1] / sky_max * star_vis_gain).clamp(0.06, 0.98),
            (sky_raw[2] / sky_max * star_vis_gain).clamp(0.08, 1.00),
        ];
        let hor_max = hor_raw[0].max(hor_raw[1]).max(hor_raw[2]).max(1e-4);
        let sky_horizon_rgb = [
            (hor_raw[0] / hor_max * (star_vis_gain + 0.05).min(1.0)).clamp(0.10, 0.99),
            (hor_raw[1] / hor_max * (star_vis_gain + 0.05).min(1.0)).clamp(0.10, 0.99),
            (hor_raw[2] / hor_max * (star_vis_gain + 0.05).min(1.0)).clamp(0.10, 1.00),
        ];

        // 4. Sunset & Twilight Colors (`sunset_rgb` & `twilight_rgb`):
        let (sunset_rgb, twilight_rgb) = if atmosphere == AtmosphereType::ThinDustyCO2 {
            // Mars-like reverse sunset: micron iron-oxide dust forward-scatters blue-violet in the solar aureole!
            let blue_halo = [
                (0.36 * i_star[0] + 0.14).clamp(0.22, 0.78),
                (0.58 * i_star[1] + 0.16).clamp(0.36, 0.85),
                (0.86 * i_star[2].max(0.45) + 0.18).clamp(0.55, 0.98),
            ];
            let dusty_twilight = [
                (0.48 * i_star[0]).clamp(0.20, 0.60),
                (0.28 * i_star[1]).clamp(0.12, 0.42),
                (0.32 * i_star[2].max(0.35)).clamp(0.14, 0.48),
            ];
            (blue_halo, dusty_twilight)
        } else {
            // Slant airmass extinction of direct + forward-scattered starlight at the horizon:
            let airmass = match atmosphere {
                AtmosphereType::DenseCO2 => 2.2f32,
                AtmosphereType::TholinHaze => 2.8f32,
                AtmosphereType::MethaneRich => 5.2f32,
                _ => 6.8f32,
            };
            let mut sun_raw = [0.0f32; 3];
            for c in 0..3 {
                sun_raw[c] = i_star[c] * (-airmass * (tau_r[c] + 0.95 * tau_gas_abs[c])).exp();
            }
            // Add a touch of multiple-scattered horizon glow so hot blue stars / methane skies keep their rich secondary hue:
            sun_raw[0] += 0.12 * sky_horizon_rgb[0];
            sun_raw[1] += 0.10 * sky_horizon_rgb[1];
            sun_raw[2] += 0.14 * sky_horizon_rgb[2] * (if temp_k > 6400.0 { 1.6 } else { 0.7 });

            let smax = sun_raw[0].max(sun_raw[1]).max(sun_raw[2]).max(1e-4);
            let sunset_gain = if temp_k < 3600.0 { 0.82 } else { 0.98 };
            let s_rgb = [
                (sun_raw[0] / smax * sunset_gain).clamp(0.08, 0.99),
                (sun_raw[1] / smax * sunset_gain).clamp(0.06, 0.96),
                (sun_raw[2] / smax * sunset_gain).clamp(0.05, 0.96),
            ];
            let tw_rgb = [
                (0.35 * s_rgb[0] + 0.25 * sky_zenith_rgb[0]).clamp(0.05, 0.65),
                (0.25 * s_rgb[1] + 0.30 * sky_zenith_rgb[1]).clamp(0.04, 0.65),
                (0.30 * s_rgb[2] + 0.45 * sky_zenith_rgb[2]).clamp(0.08, 0.78),
            ];
            (s_rgb, tw_rgb)
        };

        // 5. Main Plant Foliage Colors (`plant_foliage_rgb` & `plant_highlight_rgb`):
        //    Compute surface-transmitted photon spectrum `f_surf` and evolve photosynthetic pigments
        //    following NASA Astrobiology (Kiang et al.) & the Google AI Mode reference:
        let f_surf = [
            i_star[0] * (-0.65 * (tau_r[0] + tau_gas_abs[0])).exp(),
            i_star[1] * (-0.65 * (tau_r[1] + tau_gas_abs[1])).exp(),
            i_star[2] * (-0.65 * (tau_r[2] + tau_gas_abs[2])).exp(),
        ];
        let blue_blocked = matches!(
            atmosphere,
            AtmosphereType::DenseCO2 | AtmosphereType::TholinHaze | AtmosphereType::ThinDustyCO2
        );
        let red_blocked = atmosphere == AtmosphereType::MethaneRich;

        let (plant_foliage_rgb, plant_highlight_rgb, plant_color_name, plant_pigment_desc) =
            if temp_k < 4015.0 {
                // M-Type Red Dwarf: scarce visible light, peak in red/near-IR -> pan-visible absorber (jet-black / deep purple-burgundy)
                let cool_t = ((4015.0 - temp_k) / 1300.0).clamp(0.0, 1.0);
                let fol = [
                    (0.28 - 0.10 * cool_t).clamp(0.14, 0.32),
                    (0.11 - 0.04 * cool_t).clamp(0.06, 0.14),
                    (0.30 - 0.08 * cool_t).clamp(0.18, 0.34),
                ];
                let hi = [
                    (0.50 - 0.10 * cool_t).clamp(0.34, 0.54),
                    (0.16 - 0.04 * cool_t).clamp(0.10, 0.20),
                    (0.38 - 0.06 * cool_t).clamp(0.28, 0.42),
                ];
                (
                    fol,
                    hi,
                    "Deep Indigo-Black / Burgundy".to_string(),
                    "Pan-visible bacteriochlorophyll absorbs nearly all visible photons under red/IR M-dwarf light".to_string(),
                )
            } else if temp_k < 5300.0 {
                // K-Type Orange Dwarf (or cool orange subgiant): peak photon flux in orange-red (580-720 nm)
                let k_t = ((temp_k - 4015.0) / 1285.0).clamp(0.0, 1.0);
                if red_blocked {
                    // Methane strips red out of the K-star's light, leaving green-yellow at the surface:
                    (
                        [0.48 + 0.10 * k_t, 0.62 + 0.08 * k_t, 0.22],
                        [0.68, 0.78, 0.32],
                        "Bronze-Chartreuse / Olive".to_string(),
                        "CH4 filters orange-red photons; plants harvest green-yellow and reflect bronze-olive".to_string(),
                    )
                } else {
                    // Classic K-star autumnal foliage (yellow-orange, russet-gold, crimson-amber):
                    (
                        [0.84 - 0.14 * k_t, 0.44 + 0.16 * k_t, 0.13 + 0.04 * k_t],
                        [0.95, 0.68 + 0.10 * k_t, 0.18],
                        "Autumnal Amber-Orange / Russet".to_string(),
                        "Xanthophyll & phycoerythrin pigments harvest orange-red & blue-green K-star light".to_string(),
                    )
                }
            } else if temp_k < 6350.0 {
                // G-Type & Late F-Type Stars (Sol-like balanced spectrum):
                let g_t = ((temp_k - 5300.0) / 1050.0).clamp(0.0, 1.0);
                if red_blocked {
                    // Methane absorbs red (600-700 nm); plants absorb blue + yellow-green and reflect jade-turquoise:
                    (
                        [0.11, 0.68 + 0.08 * g_t, 0.50 + 0.10 * g_t],
                        [0.24, 0.86, 0.68],
                        "Jade-Turquoise / Cyan-Green".to_string(),
                        "Atmospheric CH4 absorbs red light; plants shift absorption to yellow-green & reflect cyan-jade".to_string(),
                    )
                } else if blue_blocked {
                    // Thick CO2 / Tholin haze / Iron dust filters out blue light before it reaches the surface:
                    let r_surf_ratio = (f_surf[0] / (f_surf[2] + 0.15)).clamp(0.8, 2.5);
                    let warm_shift = ((r_surf_ratio - 0.8) / 1.7).clamp(0.0, 1.0);
                    (
                        [0.46 + 0.18 * warm_shift, 0.68 - 0.04 * warm_shift, 0.16],
                        [0.74, 0.82, 0.24],
                        "Golden-Olive / Ochre-Green".to_string(),
                        "Atmospheric haze/CO2 depletes blue photons; plants harvest yellow-red & reflect olive-gold".to_string(),
                    )
                } else {
                    // Classic chlorophyll green (shifting slightly toward lime-teal for hotter F8/F9 stars):
                    (
                        [0.15 + 0.06 * g_t, 0.68 + 0.10 * g_t, 0.20 + 0.12 * g_t],
                        [0.36 + 0.08 * g_t, 0.86, 0.28 + 0.14 * g_t],
                        "Chlorophyll Emerald Green".to_string(),
                        "Chlorophyll a/b absorbs abundant blue & red photons while reflecting the green spectral gap".to_string(),
                    )
                }
            } else {
                // Early F-Type & A-Type Hot Blue-White Stars (T >= 6350 K): intense blue & UV radiation
                let f_t = ((temp_k - 6350.0) / 2800.0).clamp(0.0, 1.0);
                if blue_blocked {
                    // Thick atmospheric haze shields the surface from UV/blue, allowing bright emerald-chartreuse flora:
                    (
                        [0.34, 0.76, 0.28 + 0.12 * f_t],
                        [0.58, 0.88, 0.42],
                        "Luminous Chartreuse-Emerald".to_string(),
                        "Atmospheric haze screens harsh UV/blue, leaving abundant green-yellow photons for photosynthesis".to_string(),
                    )
                } else {
                    // Flooded with intense blue/UV photons: plants reflect excess blue/cyan and use golden-red carotenoid shields!
                    (
                        [0.16 + 0.12 * f_t, 0.62 - 0.06 * f_t, 0.80 + 0.12 * f_t],
                        [0.92, 0.46 + 0.10 * f_t, 0.28],
                        "Cerulean-Teal & Coral UV-Shield".to_string(),
                        "Reflects excess high-energy blue photons with anthocyanin/carotenoid UV-screening canopy pigments".to_string(),
                    )
                }
            };

        let sky_color_name = Self::describe_sky_color(sky_zenith_rgb, atmosphere, temp_k);
        let sunset_color_name = Self::describe_sunset_color(sunset_rgb, atmosphere, temp_k);

        Self {
            atmosphere,
            sun_rgb,
            sky_zenith_rgb,
            sky_horizon_rgb,
            sky_color_name,
            sky_mechanism: sky_mech.to_string(),
            sunset_rgb,
            twilight_rgb,
            sunset_color_name,
            sunset_mechanism: sunset_mech.to_string(),
            plant_foliage_rgb,
            plant_highlight_rgb,
            plant_color_name,
            plant_pigment_desc,
        }
    }

    fn describe_sky_color(rgb: [f32; 3], atm: AtmosphereType, temp_k: f32) -> String {
        match atm {
            AtmosphereType::MethaneRich => {
                if temp_k < 4200.0 {
                    "Deep Sea-Green / Jade".to_string()
                } else {
                    "Vivid Cyan-Aquamarine".to_string()
                }
            }
            AtmosphereType::DenseCO2 => {
                if temp_k < 4400.0 {
                    "Hazy Amber-Copper".to_string()
                } else {
                    "Milky Golden-Cream".to_string()
                }
            }
            AtmosphereType::TholinHaze => "Hazy Butterscotch-Amber".to_string(),
            AtmosphereType::ThinDustyCO2 => "Rusty Butterscotch-Salmon".to_string(),
            AtmosphereType::EarthLikeN2O2 | AtmosphereType::HydrogenRich => {
                if temp_k >= 6500.0 {
                    "Brilliant Sapphire-Violet".to_string()
                } else if temp_k >= 5200.0 {
                    if atm == AtmosphereType::HydrogenRich {
                        "Luminous Periwinkle-Azure".to_string()
                    } else {
                        "Cerulean Rayleigh Blue".to_string()
                    }
                } else if temp_k >= 3900.0 {
                    "Warm Turquoise-Peach".to_string()
                } else {
                    let _ = rgb;
                    "Dusky Amber-Mauve".to_string()
                }
            }
        }
    }

    fn describe_sunset_color(rgb: [f32; 3], atm: AtmosphereType, temp_k: f32) -> String {
        let _ = rgb;
        match atm {
            AtmosphereType::MethaneRich => "Emerald-Teal & Indigo".to_string(),
            AtmosphereType::ThinDustyCO2 => "Cool Blue-Periwinkle Halo".to_string(),
            AtmosphereType::DenseCO2 => "Smoldering Copper-Maroon".to_string(),
            AtmosphereType::TholinHaze => "Burnt Sienna & Carmine".to_string(),
            AtmosphereType::HydrogenRich => {
                if temp_k >= 6200.0 {
                    "Electric Violet-Magenta".to_string()
                } else {
                    "Rose-Coral & Crimson".to_string()
                }
            }
            AtmosphereType::EarthLikeN2O2 => {
                if temp_k >= 6400.0 {
                    "Saffron-Gold & Magenta".to_string()
                } else if temp_k >= 5100.0 {
                    "Crimson-Orange & Gold".to_string()
                } else if temp_k >= 3850.0 {
                    "Deep Blood-Orange & Ruby".to_string()
                } else {
                    "Dark Infrared-Ember & Purple".to_string()
                }
            }
        }
    }
}

/// Sample a plausible `HomeStar` from `rng` given `star_filter` (`0` = Auto Habitable, `1` = F, `2` = G, `3` = K, `4` = M, `5` = A, `6` = Subgiant)
/// and `is_old_pop` (true for Elliptical/Lenticular/Globular/Bulge populations).
pub fn sample_home_star_with_rng(
    rng: &mut Pcg64Mcg,
    gal_pos_kpc: [f32; 3],
    star_filter: u32,
    is_old_pop: bool,
) -> HomeStar {
    let chosen_class = if (1..=6).contains(&star_filter) {
        star_filter
    } else {
        let p = rng.next_f64();
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
            let t = rng.uniform(0.0, 1.0) as f32;
            (0.32 + 0.24 * t, 1.50 - 0.42 * t, 1.45 - 0.35 * t, false)
        }
        2 => {
            let t = rng.uniform(0.0, 1.0) as f32;
            (0.58 + 0.22 * t, 1.08 - 0.22 * t, 1.10 - 0.22 * t, false)
        }
        3 => {
            let t = rng.uniform(0.0, 1.0) as f32;
            (0.82 + 0.52 * t, 0.84 - 0.28 * t, 0.86 - 0.24 * t, false)
        }
        4 => {
            let t = rng.uniform(0.0, 1.0) as f32;
            (1.38 + 0.32 * t, 0.52 - 0.30 * t, 0.56 - 0.30 * t, false)
        }
        5 => {
            let t = rng.uniform(0.0, 1.0) as f32;
            (0.02 + 0.26 * t, 2.10 - 0.55 * t, 1.85 - 0.38 * t, false)
        }
        _ => {
            let t = rng.uniform(0.0, 1.0) as f32;
            (
                0.72 + 0.45 * t,
                1.15 + 0.35 * (1.0 - t),
                2.10 + 1.10 * t,
                true,
            )
        }
    };

    let temp_k = bv_to_temp_k(bv);
    let rgb = bv_to_rgb(bv);
    let temp_ratio = temp_k / 5778.0;
    let luminosity_solar = (radius_solar * radius_solar) * temp_ratio.powi(4);
    let habitable_au = luminosity_solar.sqrt().max(0.04);
    let orbital_period_days = 365.25 * (habitable_au.powi(3) / mass_solar.max(0.08)).sqrt();
    let angular_diameter_deg = 0.533 * (radius_solar / habitable_au);
    let solar_tide_factor = (mass_solar / habitable_au.powi(3)).clamp(0.05, 50.0);

    let lifespan_gyr = (10.0 * (mass_solar as f64).powf(-2.5)).clamp(0.6, 80.0) as f32;
    let age_gyr = (lifespan_gyr * (rng.uniform(0.25, 0.75) as f32)).clamp(0.4, 12.5);

    let abs_mag_est = 4.83 - 2.5 * luminosity_solar.max(1e-4).log10();
    let cls = classify_star(bv, abs_mag_est);
    let lum_cls = if is_subgiant {
        "IV"
    } else {
        cls.luminosity_class
    };
    let spectral_type = format!("{}{}{lum_cls}", cls.spectral_letter, cls.subclass);
    let class_description = match (cls.spectral_letter, is_subgiant) {
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

/// Sample or resolve an `AtmosphereType` (`0..=5` explicit, or `>= 99` for weighted random habitable atmosphere).
pub fn sample_atmosphere_with_rng(rng: &mut Pcg64Mcg, atmosphere_id: u32) -> AtmosphereType {
    if atmosphere_id <= 5 {
        AtmosphereType::from_id(atmosphere_id)
    } else {
        let p = rng.next_f64();
        if p < 0.42 {
            AtmosphereType::EarthLikeN2O2
        } else if p < 0.60 {
            AtmosphereType::MethaneRich
        } else if p < 0.74 {
            AtmosphereType::DenseCO2
        } else if p < 0.86 {
            AtmosphereType::TholinHaze
        } else if p < 0.93 {
            AtmosphereType::ThinDustyCO2
        } else {
            AtmosphereType::HydrogenRich
        }
    }
}

/// Visual classification of a planet in a generated solar system ("what it looks like").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetKind {
    RockyBarren,
    RockyCloudy,
    RockyTemperate,
    RockyDesert,
    RockySuperEarth,
    GasGiant,
    RingedGasGiant,
    IceGiant,
    RingedIceGiant,
    IcyDwarf,
}

impl PlanetKind {
    pub fn appearance_label(self) -> &'static str {
        match self {
            Self::RockyBarren => "Scorched Rocky",
            Self::RockyCloudy => "Cloudy Rocky",
            Self::RockyTemperate => "Temperate Rocky",
            Self::RockyDesert => "Red Rocky Desert",
            Self::RockySuperEarth => "Rocky Super-Earth",
            Self::GasGiant => "Gas Giant",
            Self::RingedGasGiant => "Gas Giant with Rings",
            Self::IceGiant => "Ice Giant",
            Self::RingedIceGiant => "Ice Giant with Rings",
            Self::IcyDwarf => "Icy Dwarf",
        }
    }

    pub fn has_rings(self) -> bool {
        matches!(self, Self::RingedGasGiant | Self::RingedIceGiant)
    }

    pub fn is_rocky(self) -> bool {
        matches!(
            self,
            Self::RockyBarren
                | Self::RockyCloudy
                | Self::RockyTemperate
                | Self::RockyDesert
                | Self::RockySuperEarth
                | Self::IcyDwarf
        )
    }

    pub fn default_rgb(self) -> [f32; 3] {
        match self {
            Self::RockyBarren => [0.68, 0.64, 0.60],
            Self::RockyCloudy => [0.94, 0.86, 0.64],
            Self::RockyTemperate => [0.28, 0.62, 0.92],
            Self::RockyDesert => [0.88, 0.44, 0.26],
            Self::RockySuperEarth => [0.42, 0.76, 0.62],
            Self::GasGiant => [0.90, 0.74, 0.52],
            Self::RingedGasGiant => [0.94, 0.84, 0.58],
            Self::IceGiant => [0.38, 0.72, 0.96],
            Self::RingedIceGiant => [0.52, 0.86, 0.94],
            Self::IcyDwarf => [0.76, 0.82, 0.90],
        }
    }

    pub fn geometric_albedo(self) -> f32 {
        match self {
            Self::RockyBarren => 0.14,
            Self::RockyCloudy => 0.68,
            Self::RockyTemperate => 0.36,
            Self::RockyDesert => 0.22,
            Self::RockySuperEarth => 0.38,
            Self::GasGiant => 0.52,
            Self::RingedGasGiant => 0.62, // Rings boost effective brightness
            Self::IceGiant => 0.41,
            Self::RingedIceGiant => 0.48,
            Self::IcyDwarf => 0.55,
        }
    }
}

/// A planet in the host star's planetary system.
#[derive(Clone, Debug)]
pub struct SolarSystemPlanet {
    /// 0-based orbital index from innermost (`0`) to outermost (`N - 1`).
    pub index: u32,
    /// Semi-major axis in Astronomical Units (`AU`).
    pub orbit_au: f32,
    /// Keplerian orbital period in Earth days (`365.25 * sqrt(a^3 / M_star)`).
    pub orbital_period_days: f32,
    /// Planetary radius in Earth radii (`R⊕`, where `1.0 R⊕ = 6,371 km`).
    pub radius_earth: f32,
    /// Visual classification ("what it looks like": rocky, gas giant, with rings, etc.).
    pub kind: PlanetKind,
    /// Whether the planet has a prominent ring system.
    pub has_rings: bool,
    /// Number of moons orbiting this planet.
    pub moon_count: u32,
    /// True if this slot is the simulated / home world.
    pub is_home_world: bool,
    /// Apparent visual magnitude `m_V` as seen from the home world (`0.0` for the home world itself).
    pub apparent_mag: f32,
    /// Heliocentric orbital longitude in radians `[0, 2π)` for 3D solar system & sky rendering.
    pub orbit_angle_rad: f32,
    /// Unit direction on the celestial sphere along the ecliptic as seen from the home world.
    pub sky_dir: Vec3,
    /// Characteristic RGB color `[r, g, b]` in `[0.0, 1.0]`.
    pub rgb: [f32; 3],
}

impl SolarSystemPlanet {
    pub fn is_naked_eye_visible(&self) -> bool {
        !self.is_home_world && self.apparent_mag <= 6.0
    }

    pub fn summary_entry(&self) -> String {
        if self.is_home_world {
            format!(
                "#{} [HOME]: {} ({:.2} R⊕ @ {:.2} AU, {:.0}d, {} moon{})",
                self.index + 1,
                self.kind.appearance_label(),
                self.radius_earth,
                self.orbit_au,
                self.orbital_period_days,
                self.moon_count,
                if self.moon_count == 1 { "" } else { "s" }
            )
        } else {
            format!(
                "#{}: {} ({:.2} R⊕ @ {:.2} AU, {:.0}d, m_V {:+.1})",
                self.index + 1,
                self.kind.appearance_label(),
                self.radius_earth,
                self.orbit_au,
                self.orbital_period_days,
                self.apparent_mag
            )
        }
    }

    /// Gloss used for `ConceptKey::Planet(self.index)` in `history.rs`.
    pub fn lexicon_gloss(&self) -> String {
        let look = self.kind.appearance_label().to_lowercase();
        if self.is_home_world {
            format!(
                "planet #{} (home world — {}, {:.2} R⊕ @ {:.2} AU)",
                self.index + 1,
                look,
                self.radius_earth,
                self.orbit_au
            )
        } else {
            format!(
                "planet #{} ({}, {:.2} R⊕ @ {:.2} AU)",
                self.index + 1,
                look,
                self.radius_earth,
                self.orbit_au
            )
        }
    }
}

/// Recompute apparent visual magnitudes (`apparent_mag`) and sky directions (`sky_dir`)
/// for all planets in `planets` as observed from the home world (`is_home_world == true`).
pub fn recompute_planet_visibility_from_home(
    planets: &mut [SolarSystemPlanet],
    home_star: &HomeStar,
    ecliptic_pole: Vec3,
) {
    let home_idx = planets
        .iter()
        .position(|p| p.is_home_world)
        .unwrap_or(planets.len() / 3);
    let home_au = planets
        .get(home_idx)
        .map(|p| p.orbit_au)
        .unwrap_or(home_star.habitable_au)
        .max(0.02);
    let home_angle = planets
        .get(home_idx)
        .map(|p| p.orbit_angle_rad)
        .unwrap_or(0.0);

    let n_pole = normal(ecliptic_pole);
    let ref_axis = if n_pole[0].abs() < 0.8 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let e_u = normal(cross(n_pole, ref_axis));
    let e_v = normal(cross(n_pole, e_u));
    let hx = home_au * home_angle.cos();
    let hy = home_au * home_angle.sin();
    let lum_mag_offset = -2.5 * home_star.luminosity_solar.max(1e-4).log10();

    for p in planets.iter_mut() {
        p.orbital_period_days =
            365.25 * (p.orbit_au.powi(3) / home_star.mass_solar.max(0.08)).sqrt();
        if p.is_home_world {
            p.apparent_mag = 0.0;
            p.sky_dir = e_u;
            continue;
        }
        // Characteristic favorable viewing distance (near opposition for outer planets, near greatest elongation for inner):
        let delta_au = (p.orbit_au - home_au).abs().max(0.12 * home_au);
        let r_km = (p.radius_earth * 6371.0).max(500.0);
        let r_au = r_km / 1.496e8;
        let phase_factor = if p.orbit_au < home_au { 0.45 } else { 0.85 };
        let flux_ratio =
            (p.kind.geometric_albedo() * phase_factor * (r_au * r_au))
                / (p.orbit_au * p.orbit_au * delta_au * delta_au).max(1e-12);
        p.apparent_mag =
            (-26.74 - 2.5 * flux_ratio.max(1e-18).log10() + lum_mag_offset).clamp(-4.8, 11.5);

        let px = p.orbit_au * p.orbit_angle_rad.cos();
        let py = p.orbit_au * p.orbit_angle_rad.sin();
        let dx = (px - hx) as f64;
        let dy = (py - hy) as f64;
        let norm_d = (dx * dx + dy * dy).sqrt().max(1e-6);
        let ux = dx / norm_d;
        let uy = dy / norm_d;
        p.sky_dir = normal([
            e_u[0] * ux + e_v[0] * uy,
            e_u[1] * ux + e_v[1] * uy,
            e_u[2] * ux + e_v[2] * uy,
        ]);
    }
}

/// Generate a plausible planetary system (`4..=9` planets, or the 8-planet Solar System when `is_real_sol` is true)
/// around `home_star`, determining each planet's orbital distance (`orbit_au`) and appearance (`PlanetKind`).
pub fn generate_solar_system_with_rng(
    rng: &mut Pcg64Mcg,
    home_star: &HomeStar,
    ecliptic_pole: Vec3,
    is_real_sol: bool,
) -> Vec<SolarSystemPlanet> {
    if is_real_sol {
        let specs: [(f32, f32, PlanetKind, u32, bool, f32); 8] = [
            (0.39, 0.38, PlanetKind::RockyBarren, 0, false, 0.8),
            (0.72, 0.95, PlanetKind::RockyCloudy, 0, false, 2.1),
            (1.00, 1.00, PlanetKind::RockyTemperate, 1, true, 0.0),
            (1.52, 0.53, PlanetKind::RockyDesert, 2, false, 3.7),
            (5.20, 11.21, PlanetKind::GasGiant, 4, false, 4.9),
            (9.58, 9.45, PlanetKind::RingedGasGiant, 7, false, 1.4),
            (19.20, 4.01, PlanetKind::RingedIceGiant, 5, false, 2.9),
            (30.05, 3.88, PlanetKind::IceGiant, 1, false, 5.4),
        ];
        let mut planets = Vec::with_capacity(8);
        for (idx, (au, rad, kind, moons, is_home, angle)) in specs.into_iter().enumerate() {
            planets.push(SolarSystemPlanet {
                index: idx as u32,
                orbit_au: au,
                orbital_period_days: 365.25 * (au.powi(3) / home_star.mass_solar.max(0.08)).sqrt(),
                radius_earth: rad,
                kind,
                has_rings: kind.has_rings(),
                moon_count: moons,
                is_home_world: is_home,
                apparent_mag: 0.0,
                orbit_angle_rad: angle,
                sky_dir: [1.0, 0.0, 0.0],
                rgb: kind.default_rgb(),
            });
        }
        recompute_planet_visibility_from_home(&mut planets, home_star, ecliptic_pole);
        return planets;
    }

    let n_planets = 4 + ((rng.next_u64() as usize) % 6); // 4..=9 planets
    let hz = home_star.habitable_au.max(0.04);
    let hz_slot = (1 + ((rng.next_u64() as usize) % (n_planets / 2).max(1))).min(n_planets - 2);
    let hz_orbit = hz * (rng.uniform(0.84, 1.22) as f32);

    let mut orbits = vec![0.0f32; n_planets];
    orbits[hz_slot] = hz_orbit;
    for i in (0..hz_slot).rev() {
        let step = rng.uniform(0.54, 0.68) as f32;
        orbits[i] = (orbits[i + 1] * step).max(0.015 * hz);
    }
    for i in (hz_slot + 1)..n_planets {
        let step = rng.uniform(1.52, 1.92) as f32;
        orbits[i] = orbits[i - 1] * step;
    }

    let mut planets = Vec::with_capacity(n_planets);
    for (idx, &orbit_au) in orbits.iter().enumerate() {
        let ratio = orbit_au / hz;
        let p_roll = rng.next_f64();
        let (kind, radius_earth, moon_count) = if idx == hz_slot {
            // Ensure at least one terrestrial candidate near the habitable zone:
            let k = if p_roll < 0.55 {
                PlanetKind::RockyTemperate
            } else if p_roll < 0.78 {
                PlanetKind::RockyDesert
            } else {
                PlanetKind::RockySuperEarth
            };
            let rad = if k == PlanetKind::RockySuperEarth {
                rng.uniform(1.30, 1.85) as f32
            } else {
                rng.uniform(0.75, 1.25) as f32
            };
            let moons = (rng.next_u64() % 3) as u32;
            (k, rad, moons)
        } else if ratio < 0.65 {
            // Inner hot zone (Mercury / Venus analogues):
            if p_roll < 0.58 {
                (
                    PlanetKind::RockyBarren,
                    rng.uniform(0.32, 0.78) as f32,
                    0,
                )
            } else {
                (
                    PlanetKind::RockyCloudy,
                    rng.uniform(0.72, 1.15) as f32,
                    (rng.next_u64() % 2) as u32,
                )
            }
        } else if ratio <= 1.75 {
            // Outer habitable / warm zone (Mars / Super-Earth / warm giant analogues):
            if p_roll < 0.42 {
                (
                    PlanetKind::RockyDesert,
                    rng.uniform(0.45, 0.95) as f32,
                    (rng.next_u64() % 3) as u32,
                )
            } else if p_roll < 0.72 {
                (
                    PlanetKind::RockyTemperate,
                    rng.uniform(0.78, 1.28) as f32,
                    (rng.next_u64() % 3) as u32,
                )
            } else if p_roll < 0.90 {
                (
                    PlanetKind::RockySuperEarth,
                    rng.uniform(1.30, 1.95) as f32,
                    (1 + rng.next_u64() % 3) as u32,
                )
            } else {
                (
                    PlanetKind::RingedGasGiant,
                    rng.uniform(6.0, 10.5) as f32,
                    (3 + rng.next_u64() % 5) as u32,
                )
            }
        } else if ratio <= 8.5 {
            // Frost-line gas giant zone (Jupiter / Saturn / Super-Earth analogues):
            if p_roll < 0.44 {
                (
                    PlanetKind::GasGiant,
                    rng.uniform(7.2, 12.8) as f32,
                    (3 + rng.next_u64() % 6) as u32,
                )
            } else if p_roll < 0.82 {
                (
                    PlanetKind::RingedGasGiant,
                    rng.uniform(6.2, 10.8) as f32,
                    (4 + rng.next_u64() % 6) as u32,
                )
            } else {
                (
                    PlanetKind::RockySuperEarth,
                    rng.uniform(1.40, 2.15) as f32,
                    (1 + rng.next_u64() % 3) as u32,
                )
            }
        } else {
            // Outer ice giant & Kuiper dwarf zone (Uranus / Neptune / Pluto analogues):
            if p_roll < 0.40 {
                (
                    PlanetKind::IceGiant,
                    rng.uniform(3.1, 4.8) as f32,
                    (1 + rng.next_u64() % 5) as u32,
                )
            } else if p_roll < 0.76 {
                (
                    PlanetKind::RingedIceGiant,
                    rng.uniform(3.3, 5.1) as f32,
                    (2 + rng.next_u64() % 5) as u32,
                )
            } else if p_roll < 0.88 {
                (
                    PlanetKind::RingedGasGiant,
                    rng.uniform(5.5, 8.8) as f32,
                    (3 + rng.next_u64() % 5) as u32,
                )
            } else {
                (
                    PlanetKind::IcyDwarf,
                    rng.uniform(0.24, 0.58) as f32,
                    (rng.next_u64() % 2) as u32,
                )
            }
        };

        let orbit_angle_rad = rng.uniform(0.3, 2.0 * PI - 0.3) as f32;
        planets.push(SolarSystemPlanet {
            index: idx as u32,
            orbit_au,
            orbital_period_days: 365.25
                * (orbit_au.powi(3) / home_star.mass_solar.max(0.08)).sqrt(),
            radius_earth,
            kind,
            has_rings: kind.has_rings(),
            moon_count,
            is_home_world: idx == hz_slot,
            apparent_mag: 0.0,
            orbit_angle_rad,
            sky_dir: [1.0, 0.0, 0.0],
            rgb: kind.default_rgb(),
        });
    }

    recompute_planet_visibility_from_home(&mut planets, home_star, ecliptic_pole);
    planets
}

/// Find the planet in `planets` that is the closest match to the simulated planet
/// (matching proximity to `home_star.habitable_au`, terrestrial rocky nature, and `sim_radius_km`),
/// substitute the simulated planet into that orbital slot, recompute sibling planet magnitudes,
/// and return the substituted planet's 0-based index.
pub fn substitute_simulated_planet(
    planets: &mut [SolarSystemPlanet],
    home_star: &HomeStar,
    sim_radius_km: f32,
    sim_land_pct: u32,
    sim_moon_count: u32,
    ecliptic_pole: Vec3,
) -> usize {
    if planets.is_empty() {
        return 0;
    }
    let sim_radius_earth = (sim_radius_km / 6371.0).clamp(0.25, 2.5);
    let target_au = home_star.habitable_au.max(0.04);

    let mut best_idx = 0usize;
    let mut best_score = f32::INFINITY;
    for (idx, p) in planets.iter().enumerate() {
        let orbit_diff = (p.orbit_au.max(0.01) / target_au).ln().abs();
        let rad_diff = (p.radius_earth.max(0.1) / sim_radius_earth).ln().abs();
        let type_penalty = if p.kind.is_rocky() { 0.0 } else { 3.5 };
        let score = 2.6 * orbit_diff + 1.1 * rad_diff + type_penalty;
        if score < best_score {
            best_score = score;
            best_idx = idx;
        }
    }

    for p in planets.iter_mut() {
        p.is_home_world = false;
    }

    let sim_kind = if sim_radius_earth >= 1.32 {
        PlanetKind::RockySuperEarth
    } else if sim_land_pct >= 65 {
        PlanetKind::RockyDesert
    } else if sim_land_pct == 0 {
        PlanetKind::RockyCloudy
    } else {
        PlanetKind::RockyTemperate
    };

    let home = &mut planets[best_idx];
    home.is_home_world = true;
    home.radius_earth = sim_radius_earth;
    home.moon_count = sim_moon_count;
    home.kind = sim_kind;
    home.has_rings = false;
    home.rgb = sim_kind.default_rgb();

    recompute_planet_visibility_from_home(planets, home_star, ecliptic_pole);
    best_idx
}

/// Format a concise multi-line summary of the planets in a solar system.
pub fn format_solar_system_summary(planets: &[SolarSystemPlanet]) -> String {
    planets
        .iter()
        .map(|p| p.summary_entry())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Shared galactic sky backdrop (3D galactic orientation, galaxy morphology/vantage, and naked-eye
/// ungrouped field stars) attached to a planet's `Constellation` catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct GalacticSkyBackdrop {
    pub galaxy_type: GalaxyType,
    pub vantage: GalacticVantage,
    pub gal_pos_kpc: [f32; 3],
    /// Unit normal of the galactic midplane in the planet's equatorial sky coordinates.
    pub galactic_pole: [f32; 3],
    /// Unit vector toward the host galaxy's center `[0, 0, 0] - gal_pos_kpc` in equatorial sky coordinates.
    pub galactic_center_dir: [f32; 3],
    /// `0` = disk Milky-Way-like band, `1` = halo one-sided galaxy view, `2` = bulge/spheroid core glow, `3` = irregular clouds.
    pub galaxy_band_mode: u8,
    /// Brightest ungrouped naked-eye field stars `[ra_deg, dec_deg, app_mag, bv_color]`.
    pub field_stars: Vec<[f32; 4]>,
}

impl GalacticSkyBackdrop {
    pub fn summary_label(&self) -> String {
        let [x, y, z] = self.gal_pos_kpc;
        let r_xy = (x * x + y * y).sqrt();
        format!(
            "{} · {} (R={r_xy:.1} kpc, z={z:+.2} kpc)",
            self.galaxy_type.name(),
            self.vantage.name()
        )
    }
}

/// A constellation (connected asterism of bright stars on the celestial sphere).
#[derive(Clone, Debug)]
pub struct Constellation {
    /// 0-based constellation index (`0..N-1`).
    pub index: u32,
    /// Indices into `stars` belonging to this constellation.
    pub star_indices: Vec<usize>,
    /// Pairs of star indices `(ia, ib)` forming the constellation's line segments.
    pub edges: Vec<(usize, usize)>,
    /// Resolved `[ra_deg, dec_deg, app_mag]` star positions in standard J2000-style equatorial coordinates.
    pub resolved_stars: Vec<[f32; 3]>,
    /// Local index pairs `[u, v]` into `resolved_stars` (`0..resolved_stars.len()`).
    pub local_edges: Vec<[usize; 2]>,
    /// Shared galactic orientation & ungrouped field-star backdrop for 2D sky/vantage rendering.
    pub backdrop: Option<std::sync::Arc<GalacticSkyBackdrop>>,
    /// Unit vector on the celestial sphere pointing to the centroid of the constellation.
    pub center_dir: Vec3,
    /// Celestial declination `δ ∈ [-90°, +90°]` (`asin(center_dir[2]) * 180 / π`).
    pub declination_deg: f32,
    /// Celestial right ascension `α ∈ [0°, 360°)` (`atan2(center_dir[1], center_dir[0])`).
    pub right_ascension_deg: f32,
    /// Number of stars in the constellation.
    pub star_count: u32,
    /// Apparent magnitude `m_V` of the brightest (alpha) star in the constellation.
    pub brightest_mag: f32,
}

impl PartialEq for Constellation {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
            && self.star_count == other.star_count
            && (self.declination_deg - other.declination_deg).abs() < 1e-4
            && (self.right_ascension_deg - other.right_ascension_deg).abs() < 1e-4
            && self.resolved_stars == other.resolved_stars
            && self.local_edges == other.local_edges
    }
}

impl Constellation {
    pub fn hemisphere_band_label(&self) -> &'static str {
        if self.declination_deg >= 50.0 {
            "north circumpolar"
        } else if self.declination_deg >= 18.0 {
            "northern sky"
        } else if self.declination_deg > -18.0 {
            "equatorial zodiac"
        } else if self.declination_deg > -50.0 {
            "southern sky"
        } else {
            "south circumpolar"
        }
    }

    /// Returns true if this constellation rises at least `5°` above the horizon at some point
    /// during the year for an observer at geographic latitude `lat_deg ∈ [-90°, +90°]`.
    /// Because the Sun completes a full `360°` circuit of right ascension over the course of the year,
    /// any right ascension is visible in the night sky during part of the year provided its
    /// culmination altitude `h_max = 90° - |lat_deg - declination_deg| >= 5°` (`|lat_deg - δ| <= 85°`).
    pub fn is_visible_at_latitude_deg(&self, lat_deg: f64) -> bool {
        (lat_deg - (self.declination_deg as f64)).abs() <= 85.0
    }

    /// Gloss used for `ConceptKey::Constellation(self.index)` in `history.rs`.
    pub fn lexicon_gloss(&self) -> String {
        format!(
            "constellation #{} ({} stars, dec {:+.0}° {})",
            self.index + 1,
            self.star_count,
            self.declination_deg,
            self.hemisphere_band_label()
        )
    }
}

/// Sample a plausible host `(GalaxyType, GalacticVantage, gal_pos_kpc)` for a solar system.
/// When `is_sol == true`, returns Milky Way Spiral Arm at Sol's position `[8.1, 0.0, 0.02] kpc`.
pub fn sample_galactic_home_with_rng(
    rng: &mut Pcg64Mcg,
    is_sol: bool,
) -> (GalaxyType, GalacticVantage, [f32; 3]) {
    if is_sol {
        return (
            GalaxyType::Spiral,
            GalacticVantage::SpiralArm,
            [8.1, 0.0, 0.02],
        );
    }
    let g_choices = [
        GalaxyType::Spiral,
        GalaxyType::Spiral,
        GalaxyType::Spiral,
        GalaxyType::Spiral,
        GalaxyType::BarredSpiral,
        GalaxyType::BarredSpiral,
        GalaxyType::Lenticular,
        GalaxyType::Elliptical,
        GalaxyType::Irregular,
        GalaxyType::GlobularCluster,
    ];
    let galaxy_type = g_choices[(rng.next_u64() as usize) % g_choices.len()];
    let vantage = match galaxy_type {
        GalaxyType::Spiral | GalaxyType::BarredSpiral => {
            let v_choices = [
                GalacticVantage::SpiralArm,
                GalacticVantage::SpiralArm,
                GalacticVantage::SpiralArm,
                GalacticVantage::InterArmDisk,
                GalacticVantage::InterArmDisk,
                GalacticVantage::GalacticBulge,
                GalacticVantage::GalacticHalo,
            ];
            v_choices[(rng.next_u64() as usize) % v_choices.len()]
        }
        GalaxyType::Lenticular => {
            let v_choices = [
                GalacticVantage::InterArmDisk,
                GalacticVantage::InterArmDisk,
                GalacticVantage::GalacticBulge,
                GalacticVantage::GalacticHalo,
            ];
            v_choices[(rng.next_u64() as usize) % v_choices.len()]
        }
        GalaxyType::Elliptical | GalaxyType::GlobularCluster => {
            if rng.next_f64() < 0.78 {
                GalacticVantage::SpheroidCore
            } else {
                GalacticVantage::GalacticHalo
            }
        }
        GalaxyType::Irregular => GalacticVantage::SpiralArm,
    };

    let gal_pos_kpc = match (galaxy_type, vantage) {
        (GalaxyType::Spiral | GalaxyType::BarredSpiral, GalacticVantage::SpiralArm) => {
            let r_kpc = rng.uniform(5.5, 10.5);
            let arm_idx = (rng.next_u64() % 2) as f64;
            let pitch = 0.26;
            let base_angle = arm_idx * PI + (r_kpc / 2.2).ln() / pitch;
            let phi = base_angle + rng.gauss(0.0, 0.14);
            let z = rng.gauss(0.0, 0.06);
            [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
        }
        (_, GalacticVantage::InterArmDisk) => {
            let r_kpc = rng.uniform(4.5, 11.5);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let z = rng.gauss(0.0, 0.20);
            [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
        }
        (_, GalacticVantage::GalacticBulge) => {
            let r_kpc = rng.uniform(0.5, 2.1);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let z = rng.gauss(0.0, 0.42);
            [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
        }
        (_, GalacticVantage::GalacticHalo) => {
            let r_xy = rng.uniform(4.0, 11.5);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let z_sign = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
            let z = z_sign * rng.uniform(2.8, 6.2);
            [(r_xy * phi.cos()) as f32, (r_xy * phi.sin()) as f32, z as f32]
        }
        (GalaxyType::Irregular, _) => {
            let r_kpc = rng.uniform(1.0, 5.2);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let z = rng.gauss(0.0, 0.55);
            [(r_kpc * phi.cos()) as f32, (r_kpc * phi.sin()) as f32, z as f32]
        }
        _ => {
            let r_kpc = if galaxy_type == GalaxyType::GlobularCluster {
                rng.uniform(0.002, 0.012)
            } else {
                rng.uniform(2.0, 8.5)
            };
            let u = rng.uniform(-1.0, 1.0);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let s = (1.0 - u * u).sqrt();
            [
                (r_kpc * s * phi.cos()) as f32,
                (r_kpc * 0.78 * s * phi.sin()) as f32,
                (r_kpc * 0.62 * u) as f32,
            ]
        }
    };
    (galaxy_type, vantage, gal_pos_kpc)
}

pub fn star_dir_to_ra_dec_deg(dir: Vec3) -> (f32, f32) {
    let d = normal(dir);
    let dec_deg = (d[2].clamp(-1.0, 1.0).asin() * 180.0 / PI) as f32;
    let ra_deg = ((d[1].atan2(d[0]) * 180.0 / PI).rem_euclid(360.0)) as f32;
    (ra_deg, dec_deg)
}

fn parsed_hipparcos_equatorial_stars() -> &'static Vec<CelestialStar> {
    static HIP_CACHE: std::sync::OnceLock<Vec<CelestialStar>> = std::sync::OnceLock::new();
    HIP_CACHE.get_or_init(|| {
        let mut out = Vec::with_capacity(5200);
        for line in HIPPARCOS_TSV.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let Some(id_s) = parts.next() else { continue };
            let Some(ra_s) = parts.next() else { continue };
            let Some(dec_s) = parts.next() else { continue };
            let Some(pi_s) = parts.next() else { continue };
            let Some(vmag_s) = parts.next() else { continue };
            let Some(bv_s) = parts.next() else { continue };
            let offset_s = parts.next().unwrap_or("0.5");

            let id: u32 = id_s.parse().unwrap_or(0);
            let alpha_deg: f64 = ra_s.parse().unwrap_or(0.0);
            let delta_deg: f64 = dec_s.parse().unwrap_or(0.0);
            let pi_arcsec: f64 = pi_s.parse().unwrap_or(0.001);
            let vmag: f32 = vmag_s.parse().unwrap_or(5.0);
            let bv: f32 = bv_s.parse().unwrap_or(0.5);
            let offset: f32 = offset_s.parse().unwrap_or(0.5);

            let r_ly = (3.26 / pi_arcsec.max(1e-5)) as f32;
            let ra_rad = alpha_deg * PI / 180.0;
            let dec_rad = delta_deg * PI / 180.0;
            let cos_d = dec_rad.cos();
            let dir = normal([cos_d * ra_rad.cos(), cos_d * ra_rad.sin(), dec_rad.sin()]);
            let abs_mag = vmag + 5.0 * ((pi_arcsec.max(1e-5).log10() as f32) + 1.0);
            let base_rgb = bv_to_rgb(bv);
            out.push(CelestialStar {
                id,
                dir,
                distance_ly: r_ly,
                bv_color: bv,
                abs_mag,
                app_mag: vmag,
                galactic_offset_rad: offset,
                rgb: base_rgb,
                base_rgb,
            });
        }
        out
    })
}

/// Evaluate relative 3D stellar density `ρ(x_star)` at galactocentric position `pos = [x, y, z]` kpc.
fn galactic_3d_stellar_density(
    galaxy_type: GalaxyType,
    pos: [f64; 3],
    is_young: bool,
    irr_knots: &[[f64; 3]],
) -> f64 {
    let (x, y, z) = (pos[0], pos[1], pos[2]);
    let r_xy = (x * x + y * y).sqrt();
    match galaxy_type {
        GalaxyType::Spiral | GalaxyType::BarredSpiral | GalaxyType::Lenticular => {
            let hz = if galaxy_type == GalaxyType::Lenticular {
                0.42
            } else if is_young {
                0.068 // Young OB/A thin disk (~68 pc scale height, matching Hipparcos)
            } else {
                0.28
            };
            let sech = 1.0 / (z / hz).clamp(-20.0, 20.0).cosh();
            let thin_vert = sech * sech;
            let thick_vert = (if is_young { 0.03 } else { 0.14 }) * (-z.abs() / 0.95).exp();
            let arm_boost = if galaxy_type == GalaxyType::Lenticular || r_xy < 0.8 {
                1.0
            } else {
                let phi = y.atan2(x);
                let spiral_phase = 2.0 * phi - (2.0 / 0.26) * (r_xy / 2.2).ln();
                let amp = if is_young { 0.95 } else { 0.42 };
                1.0 + amp * spiral_phase.cos().max(0.0)
            };
            let disk = (-r_xy / 3.05).exp() * (0.90 * thin_vert * arm_boost + thick_vert);
            let bulge_amp = if is_young { 0.45 } else { 2.4 };
            let bulge = if galaxy_type == GalaxyType::BarredSpiral {
                bulge_amp
                    * (-(x * x / (2.6 * 2.6) + y * y / (1.1 * 1.1) + z * z / (0.80 * 0.80))).exp()
            } else {
                bulge_amp * (-((r_xy * r_xy + (z / 0.60).powi(2)) / (1.65 * 1.65))).exp()
            };
            let halo =
                (if is_young { 0.002 } else { 0.015 }) * (1.0 + (r_xy * r_xy + z * z) / 36.0).powf(-1.5);
            disk + bulge + halo
        }
        GalaxyType::Irregular => {
            let mut d = 0.28 * (-((r_xy * r_xy + (z / 0.75).powi(2)) / 18.0)).exp();
            for k in irr_knots {
                let dx = x - k[0];
                let dy = y - k[1];
                let dz = z - k[2];
                let d2 = dx * dx + dy * dy + dz * dz;
                d += 1.45 * (-d2 / (0.75 * 0.75)).exp();
            }
            d + 0.01
        }
        GalaxyType::GlobularCluster => {
            let r2 = x * x + y * y + z * z;
            (1.0 + r2 / (0.0055 * 0.0055)).powf(-2.2)
        }
        GalaxyType::Elliptical => {
            let q2 = x * x + (y / 0.78).powi(2) + (z / 0.62).powi(2);
            (1.0 + q2 / (3.2 * 3.2)).powf(-1.85)
        }
    }
}

/// Sample a 3D luminous stellar beacon `(pos_kpc, bv_color, abs_mag)` directly from the host galaxy's
/// macroscopic structure (spiral arms, central bar/bulge, starburst knots, or spheroid).
fn sample_host_galaxy_luminous_beacon(
    rng: &mut Pcg64Mcg,
    galaxy_type: GalaxyType,
    irr_knots: &[[f64; 3]],
) -> ([f64; 3], f32, f32) {
    match galaxy_type {
        GalaxyType::Spiral | GalaxyType::BarredSpiral | GalaxyType::Lenticular => {
            let bulge_frac = match galaxy_type {
                GalaxyType::Lenticular => 0.42,
                GalaxyType::BarredSpiral => 0.34,
                _ => 0.28,
            };
            if rng.next_f64() < bulge_frac {
                let pos = if galaxy_type == GalaxyType::BarredSpiral {
                    [
                        rng.gauss(0.0, 2.05),
                        rng.gauss(0.0, 0.68),
                        rng.gauss(0.0, 0.46),
                    ]
                } else {
                    let r = rng.gauss(0.0, 1.35).abs();
                    let phi = rng.uniform(0.0, 2.0 * PI);
                    [r * phi.cos(), r * phi.sin(), rng.gauss(0.0, 0.48)]
                };
                let bv = rng.uniform(0.82, 1.62) as f32;
                let abs_mag = rng.uniform(-8.6, -3.8) as f32;
                (pos, bv, abs_mag)
            } else {
                let uprod = (rng.next_f64() * rng.next_f64()).max(1e-6);
                let r = (-2.65 * uprod.ln()).clamp(0.45, 15.5);
                let phi = if galaxy_type != GalaxyType::Lenticular && rng.next_f64() < 0.68 {
                    let arm = if rng.next_f64() < 0.5 { 0.0 } else { PI };
                    arm + (1.0 / 0.26) * (r / 2.2).max(0.2).ln() + rng.gauss(0.0, 0.32)
                } else {
                    rng.uniform(0.0, 2.0 * PI)
                };
                let z_sig = if galaxy_type == GalaxyType::Lenticular {
                    0.22
                } else {
                    0.085
                };
                let pos = [r * phi.cos(), r * phi.sin(), rng.gauss(0.0, z_sig)];
                let (bv, abs_mag) = if galaxy_type != GalaxyType::Lenticular && rng.next_f64() < 0.64
                {
                    (
                        rng.uniform(-0.26, 0.20) as f32,
                        rng.uniform(-9.8, -4.6) as f32,
                    )
                } else {
                    (
                        rng.uniform(0.78, 1.60) as f32,
                        rng.uniform(-9.0, -4.2) as f32,
                    )
                };
                (pos, bv, abs_mag)
            }
        }
        GalaxyType::Irregular => {
            let pos = if !irr_knots.is_empty() && rng.next_f64() < 0.76 {
                let k = irr_knots[(rng.next_u64() as usize) % irr_knots.len()];
                [
                    k[0] + rng.gauss(0.0, 0.48),
                    k[1] + rng.gauss(0.0, 0.48),
                    k[2] + rng.gauss(0.0, 0.36),
                ]
            } else {
                let r = rng.uniform(0.3, 4.5);
                let phi = rng.uniform(0.0, 2.0 * PI);
                [r * phi.cos(), r * phi.sin(), rng.gauss(0.0, 0.65)]
            };
            let bv = if rng.next_f64() < 0.72 {
                rng.uniform(-0.28, 0.16) as f32
            } else {
                rng.uniform(0.75, 1.55) as f32
            };
            (pos, bv, rng.uniform(-9.9, -4.5) as f32)
        }
        GalaxyType::GlobularCluster => {
            let u = rng.uniform(-1.0, 1.0);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let s = (1.0 - u * u).sqrt();
            let r = rng.gauss(0.0, 0.0055).abs().min(0.020);
            (
                [r * s * phi.cos(), r * s * phi.sin(), r * u],
                rng.uniform(0.65, 1.55) as f32,
                rng.uniform(-3.6, 0.8) as f32,
            )
        }
        GalaxyType::Elliptical => {
            let u = rng.uniform(-1.0, 1.0);
            let phi = rng.uniform(0.0, 2.0 * PI);
            let s = (1.0 - u * u).sqrt();
            let r = rng.gauss(0.0, 2.6).abs().min(11.0);
            (
                [r * s * phi.cos(), 0.78 * r * s * phi.sin(), 0.62 * r * u],
                rng.uniform(0.80, 1.62) as f32,
                rng.uniform(-8.8, -3.9) as f32,
            )
        }
    }
}

/// Synthesize a physically grounded night-sky `Constellation` + `GalacticSkyBackdrop` catalog rooted
/// in the solar system's 3D galactic location `(galaxy_type, vantage, gal_pos_kpc)`:
/// - Nearby stars (`r_ly << 1,000 ly` disk thickness) populate all sky directions isotropically,
/// - Distant luminous giants & OB supergiants (`r_ly >= 800 ly`) trace the 3D density of the galactic
///   plane, spiral arms, bulge, halo hemisphere, or irregular star-forming clouds.
pub fn synthesize_galactic_sky_constellations(
    seed: u64,
    is_sol: bool,
    galaxy_type: GalaxyType,
    vantage: GalacticVantage,
    gal_pos_kpc: [f32; 3],
) -> Vec<Constellation> {
    synthesize_galactic_sky_stars_and_constellations(
        seed,
        is_sol,
        galaxy_type,
        vantage,
        gal_pos_kpc,
    )
    .1
}

/// Synthesize both the full naked-eye `CelestialStar` catalog and the `Constellation` catalog
/// for `(seed, is_sol, galaxy_type, vantage, gal_pos_kpc)`.
pub fn synthesize_galactic_sky_stars_and_constellations(
    seed: u64,
    is_sol: bool,
    galaxy_type: GalaxyType,
    vantage: GalacticVantage,
    gal_pos_kpc: [f32; 3],
) -> (Vec<CelestialStar>, Vec<Constellation>) {
    if is_sol {
        static SOL_SKY: std::sync::OnceLock<Vec<Constellation>> = std::sync::OnceLock::new();
        let hip_stars = parsed_hipparcos_equatorial_stars();
        let consts = SOL_SKY
            .get_or_init(|| {
                let (_, mut consts) = build_constellations_from_stars(hip_stars);
                let used: std::collections::BTreeSet<usize> = consts
                    .iter()
                    .flat_map(|c| c.star_indices.iter().copied())
                    .collect();
                let mut field_candidates: Vec<usize> = (0..hip_stars.len())
                    .filter(|i| !used.contains(i) && hip_stars[*i].app_mag <= 4.55)
                    .collect();
                field_candidates.sort_by(|&a, &b| {
                    hip_stars[a]
                        .app_mag
                        .partial_cmp(&hip_stars[b].app_mag)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                field_candidates.truncate(180);
                let field_stars: Vec<[f32; 4]> = field_candidates
                    .into_iter()
                    .map(|i| {
                        let s = &hip_stars[i];
                        let (ra, dec) = star_dir_to_ra_dec_deg(s.dir);
                        [ra, dec, s.app_mag, s.bv_color]
                    })
                    .collect();
                let backdrop = std::sync::Arc::new(GalacticSkyBackdrop {
                    galaxy_type: GalaxyType::Spiral,
                    vantage: GalacticVantage::SpiralArm,
                    gal_pos_kpc: [8.1, 0.0, 0.02],
                    galactic_pole: [-0.86767, -0.19808, 0.45598],
                    galactic_center_dir: [-0.05488, -0.87344, -0.48384],
                    galaxy_band_mode: 0,
                    field_stars,
                });
                for c in &mut consts {
                    c.backdrop = Some(std::sync::Arc::clone(&backdrop));
                }
                consts
            })
            .clone();
        return (hip_stars.clone(), consts);
    }

    let mut rng = Pcg64Mcg::from_seed_u64(seed ^ 0x6A1A_C71C_541E_5EED);
    let hist_pgm = parse_hist_pgm();
    let (w, h, hist) = (hist_pgm.width, hist_pgm.height, hist_pgm.pixels);

    // Random 3D rotation of the galactic coordinate frame into the planet's equatorial sky frame:
    let z_rot = rng.uniform(-1.0, 1.0);
    let theta_rot = rng.uniform(0.0, 2.0 * PI);
    let r_rot = (1.0 - z_rot * z_rot).sqrt();
    let u_axis = [r_rot * theta_rot.cos(), r_rot * theta_rot.sin(), z_rot];
    let rot_angle = rng.uniform(0.0, 2.0 * PI);

    let x0 = [
        gal_pos_kpc[0] as f64,
        gal_pos_kpc[1] as f64,
        gal_pos_kpc[2] as f64,
    ];
    let to_center_gal = {
        let len = (x0[0] * x0[0] + x0[1] * x0[1] + x0[2] * x0[2]).sqrt();
        if len > 1e-5 {
            [-x0[0] / len, -x0[1] / len, -x0[2] / len]
        } else {
            [1.0, 0.0, 0.0]
        }
    };
    let gal_pole_eq = normal(rotate([0.0, 0.0, 1.0], u_axis, rot_angle));
    let gal_center_eq = normal(rotate(to_center_gal, u_axis, rot_angle));

    let mut irr_knots = Vec::new();
    if galaxy_type == GalaxyType::Irregular {
        for _ in 0..6 {
            let kr = rng.uniform(0.8, 4.2);
            let kz = rng.uniform(-1.2, 1.2);
            let kphi = rng.uniform(0.0, 2.0 * PI);
            irr_knots.push([
                x0[0] * 0.4 + kr * kphi.cos(),
                x0[1] * 0.4 + kr * kphi.sin(),
                kz,
            ]);
        }
    }

    let is_old_stellar_pop = matches!(
        galaxy_type,
        GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
    ) || vantage == GalacticVantage::GalacticBulge;
    let is_young_starburst = galaxy_type == GalaxyType::Irregular
        || (matches!(galaxy_type, GalaxyType::Spiral | GalaxyType::BarredSpiral)
            && vantage == GalacticVantage::SpiralArm);

    let mut stars = Vec::with_capacity(2400);
    let mut next_id = 1u32;

    // Part A: Local 3D Stellar Neighborhood (`r <= 2,200 ly` around `x0`), thinned by true 3D
    // galactic stellar density `ρ(x0 + r_kpc * u)` so foreground stars surround disk worlds in all
    // sky quadrants while thinner OB stars concentrate near the galactic plane and halo worlds have
    // appropriately sparse local skies:
    for _ in 0..5200 {
        let (c, m) = loop {
            let c = rng.uniform(-0.274, 3.271);
            let m = rng.uniform(-15.329, 7.49);
            let x = (((c + 0.274) * (w as f64) / (3.271 + 0.274)) as usize).min(w - 1);
            let y = (((m + 15.329) * (h as f64) / (7.49 + 15.329)) as usize).min(h - 1);
            let gray = hist[y * w + x];
            if gray == 255 {
                continue;
            }
            let mut base_prob = if gray == 254 {
                0.08 / 255.0
            } else if gray == 253 {
                0.18 / 255.0
            } else {
                (255.0 - (gray as f64)) / 255.0
            };
            if is_old_stellar_pop {
                if c < 0.05 && m < -1.5 {
                    base_prob *= 0.12;
                } else if c > 0.80 && m < 2.2 {
                    base_prob *= 1.55;
                }
            } else if is_young_starburst && c < 0.22 && m < 1.5 {
                base_prob *= 1.35;
            }
            if rng.next_f64() < base_prob {
                break (c as f32, m as f32);
            }
        };

        let is_young = c < 0.32 || m < 0.2;
        let max_r_ly = (3.26156 / 10.0_f64.powf(((m as f64) - 5.8) / 5.0 - 1.0)).clamp(1.5, 2200.0);
        let dist_scale = if galaxy_type == GalaxyType::GlobularCluster
            || vantage == GalacticVantage::GalacticBulge
        {
            0.76
        } else {
            1.0
        };
        let u_vol = rng.uniform(0.0005, 1.0).powf(1.0 / 2.8);
        let r_ly = ((1.0 + (max_r_ly - 1.0) * u_vol) * dist_scale).max(1.0);
        let r_kpc = if galaxy_type == GalaxyType::GlobularCluster {
            (r_ly / 3261.56).min(0.018)
        } else {
            r_ly / 3261.56
        };

        let uz = rng.uniform(-1.0, 1.0);
        let uphi = rng.uniform(0.0, 2.0 * PI);
        let ur = (1.0 - uz * uz).sqrt();
        let v_gal = [ur * uphi.cos(), ur * uphi.sin(), uz];
        let star_pos = [
            x0[0] + r_kpc * v_gal[0],
            x0[1] + r_kpc * v_gal[1],
            x0[2] + r_kpc * v_gal[2],
        ];
        let rho = galactic_3d_stellar_density(galaxy_type, star_pos, is_young, &irr_knots);
        let p_acc = (rho / 0.32).clamp(0.014, 1.0);
        if rng.next_f64() >= p_acc {
            continue;
        }
        let app_mag = m + 5.0 * (((r_ly / 3.26156).log10() as f32) - 1.0);
        if app_mag > 5.8 || stars.len() >= 4200 {
            continue;
        }
        let dir = normal(rotate(v_gal, u_axis, rot_angle));
        let base_rgb = bv_to_rgb(c);
        let off = (PI / 2.0 - v_gal[2].clamp(-1.0, 1.0).acos()).abs() as f32;
        stars.push(CelestialStar {
            id: next_id,
            dir,
            distance_ly: r_ly as f32,
            bv_color: c,
            abs_mag: m,
            app_mag: app_mag.max(-1.6),
            galactic_offset_rad: off,
            rgb: base_rgb,
            base_rgb,
        });
        next_id += 1;
    }

    // Part B: Direct 3D Host-Galaxy Luminous Beacons (`x_star` sampled from spiral arms, bar/bulge,
    // starburst knots, or spheroid and projected onto the sky via `Δx = x_star - x0`), naturally
    // tracing the Milky Way band from inside the disk or illuminating one sky hemisphere from the Halo:
    let min_beacon_kpc = if galaxy_type == GalaxyType::GlobularCluster {
        0.0012
    } else {
        0.25
    };
    let has_dust_disk = matches!(
        galaxy_type,
        GalaxyType::Spiral | GalaxyType::BarredSpiral | GalaxyType::Irregular
    );
    for _ in 0..2800 {
        let (spos, c, m) = sample_host_galaxy_luminous_beacon(&mut rng, galaxy_type, &irr_knots);
        let dx = [spos[0] - x0[0], spos[1] - x0[1], spos[2] - x0[2]];
        let r_kpc = (dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2]).sqrt();
        if r_kpc < min_beacon_kpc {
            continue;
        }
        let v_gal = [dx[0] / r_kpc, dx[1] / r_kpc, dx[2] / r_kpc];
        let r_ly = r_kpc * 3261.56;
        let ext_mag = if has_dust_disk {
            let dz = (spos[2] - x0[2]).abs();
            let dust_path_kpc = if x0[2].abs() < 0.14 && spos[2].abs() < 0.14 {
                r_kpc
            } else {
                r_kpc.min(r_kpc * (0.14 / dz.max(0.14)))
            };
            (0.55 * dust_path_kpc).min(2.6) as f32
        } else {
            0.0
        };
        let app_mag = m + 5.0 * (((r_ly / 3.26156).log10() as f32) - 1.0) + ext_mag;
        if app_mag > 5.65 || stars.len() >= 4800 {
            continue;
        }
        let dir = normal(rotate(v_gal, u_axis, rot_angle));
        let base_rgb = bv_to_rgb(c);
        let off = (PI / 2.0 - v_gal[2].clamp(-1.0, 1.0).acos()).abs() as f32;
        stars.push(CelestialStar {
            id: next_id,
            dir,
            distance_ly: r_ly as f32,
            bv_color: c,
            abs_mag: m,
            app_mag: app_mag.max(-1.4),
            galactic_offset_rad: off,
            rgb: base_rgb,
            base_rgb,
        });
        next_id += 1;
    }

    let (_, mut consts) = build_constellations_from_stars(&stars);
    let used: std::collections::BTreeSet<usize> = consts
        .iter()
        .flat_map(|c| c.star_indices.iter().copied())
        .collect();
    let mut field_candidates: Vec<usize> = (0..stars.len())
        .filter(|i| !used.contains(i) && stars[*i].app_mag <= 4.65)
        .collect();
    field_candidates.sort_by(|&a, &b| {
        stars[a]
            .app_mag
            .partial_cmp(&stars[b].app_mag)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    field_candidates.truncate(180);
    let field_stars: Vec<[f32; 4]> = field_candidates
        .into_iter()
        .map(|i| {
            let s = &stars[i];
            let (ra, dec) = star_dir_to_ra_dec_deg(s.dir);
            [ra, dec, s.app_mag, s.bv_color]
        })
        .collect();

    let galaxy_band_mode = match (galaxy_type, vantage) {
        (_, GalacticVantage::GalacticHalo) => 1,
        (GalaxyType::Elliptical | GalaxyType::GlobularCluster, _)
        | (_, GalacticVantage::GalacticBulge) => 2,
        (GalaxyType::Irregular, _) => 3,
        _ => 0,
    };
    let backdrop = std::sync::Arc::new(GalacticSkyBackdrop {
        galaxy_type,
        vantage,
        gal_pos_kpc,
        galactic_pole: [
            gal_pole_eq[0] as f32,
            gal_pole_eq[1] as f32,
            gal_pole_eq[2] as f32,
        ],
        galactic_center_dir: [
            gal_center_eq[0] as f32,
            gal_center_eq[1] as f32,
            gal_center_eq[2] as f32,
        ],
        galaxy_band_mode,
        field_stars,
    });
    for c in &mut consts {
        c.backdrop = Some(std::sync::Arc::clone(&backdrop));
    }
    (stars, consts)
}

/// Build an un-tilted 3D galaxy point cloud (`1,800` stars) as `6` floats per star
/// `[nx, ny, nz, r, g, b]` in normalized galactocentric coordinates (`nx = x_kpc / max_kpc`),
/// suitable for interactive 3D rotation around the galactic Z-axis.
pub fn synthesize_galaxy_cloud_normalized(galaxy_type: GalaxyType, seed: u64) -> Vec<f32> {
    let mut rng = Pcg64Mcg::from_seed_u64(seed ^ 0x9E37_79B9_7F4A_7C15);
    let count = 1800;
    let mut out = Vec::with_capacity(count * 6);
    let max_kpc = if galaxy_type == GalaxyType::GlobularCluster {
        0.020
    } else {
        15.0
    };
    let inv_max = (1.0 / max_kpc) as f32;

    for i in 0..count {
        let (pos_kpc, bv) = match galaxy_type {
            GalaxyType::Spiral | GalaxyType::BarredSpiral => {
                if i < 420 {
                    let r = rng.uniform(0.1, 2.4);
                    let phi = rng.uniform(0.0, 2.0 * PI);
                    let z = rng.gauss(0.0, 0.55);
                    let (x, y) = if galaxy_type == GalaxyType::BarredSpiral && i < 280 {
                        (
                            rng.gauss(0.0, 1.85).clamp(-3.6, 3.6),
                            rng.gauss(0.0, 0.55),
                        )
                    } else {
                        (r * phi.cos(), r * phi.sin())
                    };
                    ([x, y, z], rng.uniform(0.75, 1.45) as f32)
                } else if i < 1450 {
                    let r = rng.uniform(2.0, 13.5);
                    let arm = (i % 2) as f64;
                    let pitch = 0.26;
                    let angle = arm * PI + (r / 2.2).ln() / pitch + rng.gauss(0.0, 0.22);
                    let z = rng.gauss(0.0, 0.16);
                    let bv = if i % 3 == 0 {
                        rng.uniform(-0.22, 0.25) as f32
                    } else {
                        rng.uniform(0.30, 1.10) as f32
                    };
                    ([r * angle.cos(), r * angle.sin(), z], bv)
                } else {
                    let r = rng.uniform(1.5, 14.0);
                    let phi = rng.uniform(0.0, 2.0 * PI);
                    let z = rng.gauss(0.0, 0.65);
                    ([r * phi.cos(), r * phi.sin(), z], rng.uniform(0.55, 1.35) as f32)
                }
            }
            GalaxyType::Lenticular => {
                let r = if i < 600 {
                    rng.uniform(0.1, 2.8)
                } else {
                    rng.uniform(1.5, 13.5)
                };
                let phi = rng.uniform(0.0, 2.0 * PI);
                let z = rng.gauss(0.0, if i < 600 { 0.75 } else { 0.28 });
                ([r * phi.cos(), r * phi.sin(), z], rng.uniform(0.62, 1.45) as f32)
            }
            GalaxyType::Elliptical => {
                let r = rng.uniform(0.2, 12.5) * rng.uniform(0.25, 1.0);
                let u = rng.uniform(-1.0, 1.0);
                let phi = rng.uniform(0.0, 2.0 * PI);
                let s = (1.0 - u * u).sqrt();
                (
                    [r * s * phi.cos(), r * 0.76 * s * phi.sin(), r * 0.58 * u],
                    rng.uniform(0.75, 1.55) as f32,
                )
            }
            GalaxyType::Irregular => {
                let knot_idx = i % 6;
                let ka = (knot_idx as f64) * (2.0 * PI / 6.0) + 0.4;
                let kr = 2.5 + (knot_idx as f64) * 1.1;
                let x = kr * ka.cos() + rng.gauss(0.0, 1.6);
                let y = kr * ka.sin() + rng.gauss(0.0, 1.6);
                let z = rng.gauss(0.0, 1.1);
                let bv = if i % 2 == 0 {
                    rng.uniform(-0.25, 0.20) as f32
                } else {
                    rng.uniform(0.35, 1.25) as f32
                };
                ([x, y, z], bv)
            }
            GalaxyType::GlobularCluster => {
                let r = rng.uniform(0.0005, 0.016) * rng.uniform(0.2, 1.0);
                let u = rng.uniform(-1.0, 1.0);
                let phi = rng.uniform(0.0, 2.0 * PI);
                let s = (1.0 - u * u).sqrt();
                let bv = if i % 7 == 0 {
                    rng.uniform(-0.10, 0.22) as f32
                } else {
                    rng.uniform(0.70, 1.55) as f32
                };
                ([r * s * phi.cos(), r * s * phi.sin(), r * u], bv)
            }
        };
        let rgb = bv_to_rgb(bv);
        out.extend_from_slice(&[
            (pos_kpc[0] as f32 * inv_max).clamp(-1.05, 1.05),
            (pos_kpc[1] as f32 * inv_max).clamp(-1.05, 1.05),
            (pos_kpc[2] as f32 * inv_max).clamp(-1.05, 1.05),
            rgb[0],
            rgb[1],
            rgb[2],
        ]);
    }
    out
}

/// Build `(stars_6f, constellation_edges_3u32, metadata_json)` for rendering the 2D Northern & Southern
/// Celestial Hemisphere Star Maps (`drawDualHemisphereStarMap` in JS).
pub fn build_star_map_buffers_from_sky(
    is_real_hipparcos: bool,
    galaxy_type: GalaxyType,
    vantage: GalacticVantage,
    home_star: &HomeStar,
    planets: &[SolarSystemPlanet],
    stars: &[CelestialStar],
    constellations: &[Constellation],
) -> (Vec<f32>, Vec<u32>, String) {
    let mut stars_out = Vec::with_capacity(stars.len() * 6);
    for s in stars {
        let (ra_deg, dec_deg) = star_dir_to_ra_dec_deg(s.dir);
        stars_out.extend_from_slice(&[
            ra_deg.to_radians(),
            dec_deg,
            s.app_mag,
            s.base_rgb[0],
            s.base_rgb[1],
            s.base_rgb[2],
        ]);
    }

    let mut edges_out = Vec::new();
    for c in constellations {
        for &(ia, ib) in &c.edges {
            edges_out.extend_from_slice(&[ia as u32, ib as u32, c.index]);
        }
    }

    let galaxy_band_mode: u8 = match (galaxy_type, vantage) {
        (_, GalacticVantage::GalacticHalo) => 1,
        (GalaxyType::Elliptical | GalaxyType::GlobularCluster, _)
        | (_, GalacticVantage::GalacticBulge) => 2,
        (GalaxyType::Irregular, _) => 3,
        _ => 0,
    };
    let warm_glow = matches!(
        galaxy_type,
        GalaxyType::Elliptical | GalaxyType::Lenticular | GalaxyType::GlobularCluster
    );

    let backdrop = constellations.first().and_then(|c| c.backdrop.as_deref());
    let gp = backdrop
        .map(|b| b.galactic_pole)
        .unwrap_or([-0.86767, -0.19808, 0.45598]);
    let gc = backdrop
        .map(|b| b.galactic_center_dir)
        .unwrap_or([-0.05488, -0.87344, -0.48384]);
    let ep = [0.0f32, -0.398, 0.917];
    let sun_dir = [1.0, 0.0, 0.0];
    let (sun_ra_deg, sun_dec_deg) = star_dir_to_ra_dec_deg(sun_dir);
    let sr = home_star.rgb;

    let mut planet_entries = Vec::new();
    for p in planets {
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
    for c in constellations {
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

    let meta_json = format!(
        "{{\"isRealHipparcos\":{},\"starCount\":{},\"constellationCount\":{},\"galaxyBandMode\":{},\"warmGlow\":{},\"galacticPole\":[{:.5},{:.5},{:.5}],\"galacticCenterDir\":[{:.5},{:.5},{:.5}],\"eclipticPole\":[{:.5},{:.5},{:.5}],\"sun\":{{\"raRad\":{:.5},\"decDeg\":{:.3},\"rgb\":[{:.3},{:.3},{:.3}]}},\"planets\":[{}],\"constellations\":[{}]}}",
        is_real_hipparcos,
        stars.len(),
        constellations.len(),
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
    );

    (stars_out, edges_out, meta_json)
}

/// Generate a deterministic galaxy-rooted constellation catalog from `seed`.
/// `seed == 42` uses Earth's real Hipparcos sky around Sol; any other seed samples a host galaxy
/// location and synthesizes its surrounding 3D stellar neighborhood.
pub fn generate_default_constellations(seed: u64) -> Vec<Constellation> {
    if seed == 42 {
        return synthesize_galactic_sky_constellations(
            42,
            true,
            GalaxyType::Spiral,
            GalacticVantage::SpiralArm,
            [8.1, 0.0, 0.02],
        );
    }
    let mut rng = Pcg64Mcg::from_seed_u64(seed ^ 0xC0A5_7E11_A710_42u64);
    let (galaxy_type, vantage, gal_pos_kpc) = sample_galactic_home_with_rng(&mut rng, false);
    synthesize_galactic_sky_constellations(seed, false, galaxy_type, vantage, gal_pos_kpc)
}

/// Build constellation edges and extract connected `Constellation` asterisms (`3..7` stars each)
/// directly from the 3D `stars` catalog on the celestial sphere (up to `16` major cultural constellations).
pub fn build_constellations_from_stars(
    stars: &[CelestialStar],
) -> (Vec<(usize, usize)>, Vec<Constellation>) {
    build_constellations_from_stars_with_limit(stars, 16)
}

/// Build constellation edges and extract up to `max_constellations` connected `Constellation` asterisms
/// (`3..7` stars each) distributed across both Northern and Southern celestial hemispheres.
pub fn build_constellations_from_stars_with_limit(
    stars: &[CelestialStar],
    max_constellations: usize,
) -> (Vec<(usize, usize)>, Vec<Constellation>) {
    if stars.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut bright_indices: Vec<usize> = (0..stars.len())
        .filter(|&i| stars[i].app_mag <= 3.55)
        .collect();
    if bright_indices.len() < 80 {
        bright_indices = (0..stars.len()).collect();
    }
    bright_indices.sort_by(|&a, &b| {
        stars[a]
            .app_mag
            .partial_cmp(&stars[b].app_mag)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bright_indices.truncate(240);
    let nb = bright_indices.len();

    // Grow bounded connected asterism components (`<= 7` stars, within `23°` of their alpha seed)
    // so dense galactic star clouds divide into recognizable human-scale constellations instead of
    // merging into one giant chain, while foreground bright stars across all quadrants form clean asterisms.
    let mut comp_id: Vec<Option<usize>> = vec![None; nb];
    let mut degree = vec![0u8; nb];
    let mut local_adj: Vec<Vec<usize>> = vec![Vec::new(); nb];

    struct AsterismGroup {
        members: Vec<usize>, // indices into `bright_indices`
    }
    let mut groups: Vec<AsterismGroup> = Vec::new();

    for seed_loc in 0..nb {
        if comp_id[seed_loc].is_some() {
            continue;
        }
        let gid = groups.len();
        let alpha_dir = stars[bright_indices[seed_loc]].dir;
        comp_id[seed_loc] = Some(gid);
        let mut members = vec![seed_loc];

        // Iteratively attach the nearest unassigned bright star within `3.8°..20.5°` of any current member
        // (and within `23.5°` of `alpha_dir`), up to 7 stars per constellation:
        while members.len() < 7 {
            let mut best_pair: Option<(usize, usize, f64)> = None;
            for &u in &members {
                if degree[u] >= 3 {
                    continue;
                }
                let pu = stars[bright_indices[u]].dir;
                for cand in 0..nb {
                    if comp_id[cand].is_some() {
                        continue;
                    }
                    let pc = stars[bright_indices[cand]].dir;
                    if dot(alpha_dir, pc) < 0.917 {
                        continue; // > 23.5 deg from alpha seed
                    }
                    let cos_d = dot(pu, pc);
                    if (0.936..=0.9978).contains(&cos_d) {
                        // Prefer bright, crisp separations (~6..14 deg)
                        let score = cos_d - 0.018 * (stars[bright_indices[cand]].app_mag as f64);
                        if best_pair.map_or(true, |(_, _, bs)| score > bs) {
                            best_pair = Some((u, cand, score));
                        }
                    }
                }
            }
            if let Some((u, v, _)) = best_pair {
                comp_id[v] = Some(gid);
                degree[u] += 1;
                degree[v] += 1;
                local_adj[u].push(v);
                local_adj[v].push(u);
                members.push(v);
            } else {
                break;
            }
        }

        // Optionally close 1 polygon loop (dipper bowl / trapezoid / diamond) on asterisms of >= 4 stars:
        if members.len() >= 4 {
            let mut best_close: Option<(usize, usize, f64)> = None;
            for i in 0..members.len() {
                let u = members[i];
                if degree[u] >= 3 {
                    continue;
                }
                for j in (i + 1)..members.len() {
                    let v = members[j];
                    if degree[v] >= 3 || local_adj[u].contains(&v) {
                        continue;
                    }
                    let cos_d = dot(
                        stars[bright_indices[u]].dir,
                        stars[bright_indices[v]].dir,
                    );
                    if (0.968..=0.997).contains(&cos_d)
                        && best_close.map_or(true, |(_, _, bc)| cos_d > bc)
                    {
                        best_close = Some((u, v, cos_d));
                    }
                }
            }
            if let Some((u, v, _)) = best_close {
                degree[u] += 1;
                degree[v] += 1;
                local_adj[u].push(v);
                local_adj[v].push(u);
            }
        }

        groups.push(AsterismGroup { members });
    }

    struct CandidateConstellation {
        prominence: f32,
        constellation: Constellation,
    }

    let min_members = if groups.iter().filter(|g| g.members.len() >= 3).count() >= 10 {
        3
    } else {
        2
    };

    let mut candidates: Vec<CandidateConstellation> = Vec::new();
    for g in &groups {
        if g.members.len() < min_members {
            continue;
        }
        let mut sum_dir = [0.0f64; 3];
        let mut min_mag = f32::INFINITY;
        let mut lum_sum = 0.0f32;
        let mut star_indices = Vec::with_capacity(g.members.len());
        let mut resolved_stars = Vec::with_capacity(g.members.len());
        let mut loc_to_pos = std::collections::BTreeMap::new();

        for (pos_idx, &loc) in g.members.iter().enumerate() {
            let s_idx = bright_indices[loc];
            let st = &stars[s_idx];
            sum_dir[0] += st.dir[0];
            sum_dir[1] += st.dir[1];
            sum_dir[2] += st.dir[2];
            min_mag = min_mag.min(st.app_mag);
            lum_sum += (4.0 - st.app_mag).max(0.15);
            star_indices.push(s_idx);
            let (ra, dec) = star_dir_to_ra_dec_deg(st.dir);
            resolved_stars.push([ra, dec, st.app_mag]);
            loc_to_pos.insert(loc, pos_idx);
        }

        let mut global_edges = Vec::new();
        let mut local_edges = Vec::new();
        for &u_loc in &g.members {
            for &v_loc in &local_adj[u_loc] {
                if u_loc < v_loc {
                    if let (Some(&pu), Some(&pv)) =
                        (loc_to_pos.get(&u_loc), loc_to_pos.get(&v_loc))
                    {
                        global_edges.push((bright_indices[u_loc], bright_indices[v_loc]));
                        local_edges.push([pu, pv]);
                    }
                }
            }
        }

        let center_dir = normal(sum_dir);
        let (ra_deg, dec_deg) = star_dir_to_ra_dec_deg(center_dir);
        let prominence = (g.members.len() as f32) * 0.85 + lum_sum * 0.55;

        candidates.push(CandidateConstellation {
            prominence,
            constellation: Constellation {
                index: 0,
                star_count: g.members.len() as u32,
                star_indices,
                edges: global_edges,
                resolved_stars,
                local_edges,
                backdrop: None,
                center_dir,
                declination_deg: dec_deg,
                right_ascension_deg: ra_deg,
                brightest_mag: min_mag,
            },
        });
    }

    // Rank by visual prominence (NEVER north-first!) and select up to `max_constellations` well-separated
    // constellations across the celestial sphere, preserving circumpolar anchors (`|dec| > 60°`).
    candidates.sort_by(|a, b| {
        b.prominence
            .partial_cmp(&a.prominence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let max_c = max_constellations.max(4);
    let min_sep_cos = if max_c > 16 { 0.946 } else { 0.925 };
    let mut chosen: Vec<Constellation> = Vec::with_capacity(max_c);
    // Reserve the brightest deep-north (> +60°) and deep-south (< -60°) asterisms if physically present
    // in the star catalog so circumpolar skies aren't crowded out solely by low-declination giants:
    if let Some(nc) = candidates
        .iter()
        .find(|c| c.constellation.declination_deg > 60.0)
    {
        chosen.push(nc.constellation.clone());
    }
    if let Some(sc) = candidates
        .iter()
        .find(|c| c.constellation.declination_deg < -60.0)
    {
        chosen.push(sc.constellation.clone());
    }

    for cand in &candidates {
        if chosen.len() >= max_c {
            break;
        }
        let c_dir = cand.constellation.center_dir;
        let too_close = chosen
            .iter()
            .any(|existing| dot(existing.center_dir, c_dir) > min_sep_cos);
        if !too_close {
            chosen.push(cand.constellation.clone());
        }
    }
    if chosen.len() < (max_c * 3) / 4 {
        for cand in &candidates {
            if chosen.len() >= max_c.saturating_sub(2).max(4) {
                break;
            }
            let c_dir = cand.constellation.center_dir;
            let duplicate = chosen.iter().any(|existing| dot(existing.center_dir, c_dir) > 0.985);
            if !duplicate {
                chosen.push(cand.constellation.clone());
            }
        }
    }

    chosen.sort_by(|a, b| {
        b.declination_deg
            .partial_cmp(&a.declination_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut all_edges = Vec::new();
    for (idx, c) in chosen.iter_mut().enumerate() {
        c.index = idx as u32;
        all_edges.extend_from_slice(&c.edges);
    }

    (all_edges, chosen)
}


#[derive(Clone, Debug)]
pub struct ReconstructedSkyEnvironment {
    pub galaxy_type: GalaxyType,
    pub galactic_vantage: GalacticVantage,
    pub home_star: HomeStar,
    pub star_filter: u32,
    pub atmosphere: AtmosphereType,
    pub optics: PlanetaryOptics,
    pub has_atmosphere: bool,
    pub has_life: bool,
    pub moon_count: u32,
    pub moon_mass_lunar: f32,
    pub current_moon_dist_radii: f32,
    pub target_moon_dist_radii: u32,
    pub constellation_seed: u64,
    pub sys_rng_bytes: [u8; 16],
    pub is_sol_system: bool,
    pub solar_system: Vec<SolarSystemPlanet>,
    pub home_planet_index: usize,
    pub constellations: Vec<Constellation>,
}

impl ReconstructedSkyEnvironment {
    pub fn from_snapshot(
        snap: &tec11_proto::PlanetSnapshot,
        planet_radius_km: f32,
        fallback_land_pct: u32,
    ) -> Self {
        let ecliptic_pole = [0.0, -0.398, 0.917];
        let stage_flags = match &snap.stage {
            Some(tec11_proto::PlanetStageOneof::Tectonics(tec)) => (
                tec.atmosphere_ticks_remaining == 0,
                tec.life_ticks_remaining == 0,
            ),
            _ => (true, true),
        };

        if let Some(env) = &snap.environment {
            let galaxy_type = GalaxyType::from_id(env.galaxy_type);
            let galactic_vantage = GalacticVantage::from_id(env.galactic_vantage);
            let home_star = env
                .home_star
                .as_ref()
                .map(HomeStar::from_proto)
                .unwrap_or_else(HomeStar::sol);
            let star_filter = if env.star_filter > 0 {
                env.star_filter
            } else {
                home_star.star_filter_id()
            };
            let atmosphere = AtmosphereType::from_id(env.atmosphere_id);
            let optics = PlanetaryOptics::compute(&home_star, atmosphere);
            let sys_rng_bytes = <[u8; 16]>::try_from(env.sys_rng_state_16b.as_slice())
                .unwrap_or_else(|_| Pcg64Mcg::from_seed_u64(42 ^ 0x517C_C1B7_2722_0A95).to_bytes());
            let mut sys_rng = Pcg64Mcg::from_bytes(sys_rng_bytes);
            let mut solar_system = generate_solar_system_with_rng(
                &mut sys_rng,
                &home_star,
                ecliptic_pole,
                env.is_sol_system,
            );
            let land_pct = if env.land_pct > 0 {
                env.land_pct
            } else {
                fallback_land_pct
            };
            let home_planet_index = substitute_simulated_planet(
                &mut solar_system,
                &home_star,
                planet_radius_km,
                land_pct,
                env.moon_count,
                ecliptic_pole,
            );
            let constellation_seed = env.constellation_seed;
            let constellations = synthesize_galactic_sky_constellations(
                constellation_seed,
                env.is_sol_system,
                galaxy_type,
                galactic_vantage,
                home_star.gal_pos_kpc,
            );
            let target_dist = if env.target_moon_dist_radii >= 15 {
                env.target_moon_dist_radii
            } else {
                60
            };
            let cur_dist = if env.current_moon_dist_radii >= 10.0 {
                env.current_moon_dist_radii
            } else {
                target_dist as f32
            };
            let moon_mass = if env.moon_count == 0 {
                0.0
            } else if env.moon_mass_lunar > 0.0 {
                env.moon_mass_lunar
            } else {
                1.0
            };
            Self {
                galaxy_type,
                galactic_vantage,
                home_star,
                star_filter,
                atmosphere,
                optics,
                has_atmosphere: env.has_atmosphere,
                has_life: env.has_life,
                moon_count: env.moon_count,
                moon_mass_lunar: moon_mass,
                current_moon_dist_radii: cur_dist,
                target_moon_dist_radii: target_dist,
                constellation_seed,
                sys_rng_bytes,
                is_sol_system: env.is_sol_system,
                solar_system,
                home_planet_index,
                constellations,
            }
        } else {
            let home_star = HomeStar::sol();
            let atmosphere = AtmosphereType::EarthLikeN2O2;
            let optics = PlanetaryOptics::compute(&home_star, atmosphere);
            let mut sys_rng = Pcg64Mcg::from_seed_u64(42 ^ 0x517C_C1B7_2722_0A95);
            let sys_rng_bytes = sys_rng.to_bytes();
            let mut solar_system =
                generate_solar_system_with_rng(&mut sys_rng, &home_star, ecliptic_pole, true);
            let home_planet_index = substitute_simulated_planet(
                &mut solar_system,
                &home_star,
                planet_radius_km,
                fallback_land_pct,
                1,
                ecliptic_pole,
            );
            let constellations = generate_default_constellations(42);
            Self {
                galaxy_type: GalaxyType::Spiral,
                galactic_vantage: GalacticVantage::SpiralArm,
                home_star,
                star_filter: 2,
                atmosphere,
                optics,
                has_atmosphere: stage_flags.0,
                has_life: stage_flags.1,
                moon_count: 1,
                moon_mass_lunar: 1.0,
                current_moon_dist_radii: 60.0,
                target_moon_dist_radii: 60,
                constellation_seed: 42,
                sys_rng_bytes,
                is_sol_system: true,
                solar_system,
                home_planet_index,
                constellations,
            }
        }
    }
}

