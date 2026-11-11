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

const CONSTELLATION_SHAPE_LABELS: [&str; 16] = [
    "Northern Crown",
    "Polar Serpent",
    "Great Wain",
    "Chained Chariot",
    "Hunter's Bow",
    "Twin Pillars",
    "River Herd",
    "Winged Steed",
    "Golden Scales",
    "Sea Leviathan",
    "Scorpion's Tail",
    "Archer's Arrow",
    "Southern Ship",
    "Altar Flame",
    "Southern Cross",
    "South Pole Octant",
];

/// A constellation (connected asterism of bright stars on the celestial sphere).
#[derive(Clone, Debug)]
pub struct Constellation {
    /// 0-based constellation index (`0..N-1`).
    pub index: u32,
    /// Indices into `stars` belonging to this constellation.
    pub star_indices: Vec<usize>,
    /// Pairs of star indices `(ia, ib)` forming the constellation's line segments.
    pub edges: Vec<(usize, usize)>,
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
    /// Descriptive asterism figure label.
    pub shape_label: &'static str,
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
            "constellation #{} ({}, {} stars, dec {:+.0}° {})",
            self.index + 1,
            self.shape_label,
            self.star_count,
            self.declination_deg,
            self.hemisphere_band_label()
        )
    }
}

/// Generate a deterministic 16-constellation sky catalog spanning all declination bands
/// from north circumpolar (`+78°`) through the equatorial zodiac (`0°`) to south circumpolar (`-78°`).
pub fn generate_default_constellations(seed: u64) -> Vec<Constellation> {
    let mut rng = Pcg64Mcg::from_seed_u64(seed ^ 0xC0A5_7E11_A710_42u64);
    let base_declinations: [f32; 16] = [
        78.0, 64.0, 52.0, 38.0, 26.0, 14.0, 5.0, -4.0, -13.0, -24.0, -36.0, -48.0, -58.0, -67.0,
        -75.0, -82.0,
    ];
    let mut out = Vec::with_capacity(16);
    for (idx, &base_dec) in base_declinations.iter().enumerate() {
        let dec_deg = (base_dec + (rng.uniform(-3.0, 3.0) as f32)).clamp(-87.0, 87.0);
        let ra_deg =
            ((idx as f32) * 22.5 + 15.0 + (rng.uniform(-6.0, 6.0) as f32)).rem_euclid(360.0);
        let dec_rad = (dec_deg as f64) * PI / 180.0;
        let ra_rad = (ra_deg as f64) * PI / 180.0;
        let center_dir = normal([
            dec_rad.cos() * ra_rad.cos(),
            dec_rad.cos() * ra_rad.sin(),
            dec_rad.sin(),
        ]);
        let star_count = 3 + ((rng.next_u64() % 5) as u32);
        let brightest_mag = rng.uniform(0.2, 2.2) as f32;
        out.push(Constellation {
            index: idx as u32,
            star_indices: Vec::new(),
            edges: Vec::new(),
            center_dir,
            declination_deg: dec_deg,
            right_ascension_deg: ra_deg,
            star_count,
            brightest_mag,
            shape_label: CONSTELLATION_SHAPE_LABELS[idx % CONSTELLATION_SHAPE_LABELS.len()],
        });
    }
    out
}

/// Build constellation edges and extract connected `Constellation` asterisms across the celestial sphere.
pub fn build_constellations_from_stars(
    stars: &[CelestialStar],
) -> (Vec<(usize, usize)>, Vec<Constellation>) {
    let mut bright_indices: Vec<usize> = stars
        .iter()
        .enumerate()
        .filter(|(_, s)| s.app_mag <= 2.85)
        .map(|(i, _)| i)
        .collect();
    bright_indices.sort_by(|&a, &b| {
        stars[a]
            .app_mag
            .partial_cmp(&stars[b].app_mag)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bright_indices.truncate(220);

    let mut edges = Vec::new();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); bright_indices.len()];
    let mut degree = vec![0u8; bright_indices.len()];
    for i in 0..bright_indices.len() {
        if degree[i] >= 3 {
            continue;
        }
        let ia = bright_indices[i];
        let pa = stars[ia].dir;
        let mut best: Option<(usize, f64)> = None;
        for j in (i + 1)..bright_indices.len() {
            if degree[j] >= 3 {
                continue;
            }
            let ib = bright_indices[j];
            let pb = stars[ib].dir;
            let cos_d = dot(pa, pb);
            if (0.950..=0.998).contains(&cos_d) {
                if best.map_or(true, |(_, b_cos)| cos_d > b_cos) {
                    best = Some((j, cos_d));
                }
            }
        }
        if let Some((j, _)) = best {
            degree[i] += 1;
            degree[j] += 1;
            adj[i].push(j);
            adj[j].push(i);
            edges.push((bright_indices[i], bright_indices[j]));
        }
    }

    // Extract connected components from `adj`:
    let mut visited = vec![false; bright_indices.len()];
    let mut raw_groups: Vec<(Vec<usize>, Vec<(usize, usize)>, Vec3, f32, f32, f32)> = Vec::new();
    for start in 0..bright_indices.len() {
        if visited[start] || adj[start].is_empty() {
            continue;
        }
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        visited[start] = true;
        let mut comp_local = Vec::new();
        while let Some(u) = queue.pop_front() {
            comp_local.push(u);
            for &v in &adj[u] {
                if !visited[v] {
                    visited[v] = true;
                    queue.push_back(v);
                }
            }
        }
        if comp_local.len() < 2 {
            continue;
        }
        let mut comp_stars = Vec::with_capacity(comp_local.len());
        let mut sum = [0.0f64; 3];
        let mut min_mag = f32::INFINITY;
        for &loc in &comp_local {
            let s_idx = bright_indices[loc];
            comp_stars.push(s_idx);
            let d = stars[s_idx].dir;
            sum[0] += d[0];
            sum[1] += d[1];
            sum[2] += d[2];
            if stars[s_idx].app_mag < min_mag {
                min_mag = stars[s_idx].app_mag;
            }
        }
        let mut comp_edges = Vec::new();
        for &u in &comp_local {
            for &v in &adj[u] {
                if u < v {
                    comp_edges.push((bright_indices[u], bright_indices[v]));
                }
            }
        }
        let center_dir = normal(sum);
        let dec_deg = (center_dir[2].clamp(-1.0, 1.0).asin() * 180.0 / PI) as f32;
        let ra_deg = ((center_dir[1].atan2(center_dir[0]) * 180.0 / PI).rem_euclid(360.0)) as f32;
        raw_groups.push((comp_stars, comp_edges, center_dir, dec_deg, ra_deg, min_mag));
    }

    // Sort from northernmost declination to southernmost declination and cap at 16:
    raw_groups.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    raw_groups.truncate(14);

    // Ensure both deep northern (`dec >= +58°`) and deep southern (`dec <= -58°`) circumpolar bands
    // have at least one constellation even if the galaxy vantage concentrated bright stars in one hemisphere:
    let has_deep_north = raw_groups.iter().any(|g| g.3 >= 58.0);
    let has_deep_south = raw_groups.iter().any(|g| g.3 <= -58.0);
    let defaults = generate_default_constellations(stars.len() as u64);
    let mut constellations = Vec::new();
    if !has_deep_north {
        if let Some(dn) = defaults.first() {
            constellations.push(dn.clone());
        }
    }
    for (comp_stars, comp_edges, center_dir, dec_deg, ra_deg, min_mag) in raw_groups {
        let idx = constellations.len();
        constellations.push(Constellation {
            index: idx as u32,
            star_count: comp_stars.len() as u32,
            star_indices: comp_stars,
            edges: comp_edges,
            center_dir,
            declination_deg: dec_deg,
            right_ascension_deg: ra_deg,
            brightest_mag: min_mag,
            shape_label: CONSTELLATION_SHAPE_LABELS[idx % CONSTELLATION_SHAPE_LABELS.len()],
        });
    }
    if !has_deep_south {
        if let Some(ds) = defaults.last() {
            constellations.push(ds.clone());
        }
    }
    if constellations.len() < 8 {
        for d in defaults {
            if constellations.len() >= 12 {
                break;
            }
            if !constellations
                .iter()
                .any(|c| (c.declination_deg - d.declination_deg).abs() < 12.0)
            {
                constellations.push(d);
            }
        }
    }
    constellations.sort_by(|a, b| {
        b.declination_deg
            .partial_cmp(&a.declination_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (idx, c) in constellations.iter_mut().enumerate() {
        c.index = idx as u32;
        c.shape_label = CONSTELLATION_SHAPE_LABELS[idx % CONSTELLATION_SHAPE_LABELS.len()];
    }

    (edges, constellations)
}

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

    /// Build automatic constellation / asterism lines connecting neighboring bright stars (`app_mag <= 2.85`)
    /// and extract connected `Constellation` groups with celestial declinations.
    fn build_constellation_edges(&mut self) {
        let (edges, constellations) = build_constellations_from_stars(&self.stars);
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
                let mut best_idx = self.home_planet_index.min(np - 1);
                let mut best_dist = f32::INFINITY;
                for (idx, pl) in self.planets.iter().enumerate() {
                    let r_norm = 0.18 + 0.72 * ((idx as f32) / ((np - 1).max(1) as f32));
                    let pos = Self::tilt_3d_plane(
                        r_norm * pl.orbit_angle_rad.cos(),
                        r_norm * pl.orbit_angle_rad.sin(),
                        0.0,
                    );
                    let dx = (p[0] as f32) - pos[0];
                    let dy = (p[1] as f32) - pos[1];
                    let d = (dx * dx + dy * dy).sqrt();
                    if d < best_dist {
                        best_dist = d;
                        best_idx = idx;
                    }
                }
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

    /// Detailed multi-line inspection of the selected star on the celestial sphere, the Home Sun,
    /// the generated Planetary System, and the planet's Sky, Sunset, and Photosynthetic Plant Foliage colors.
    pub fn inspect_tile(&self, tile_idx: usize) -> String {
        let hs = &self.home_star;
        let op = &self.optics;
        let Some(star_idx) = self.nearest_star_to_tile(tile_idx) else {
            return String::new();
        };
        let st = &self.stars[star_idx];
        let (letter, sub, lum_cls, lum_desc) = classify_star(st.bv_color, st.abs_mag);
        let st_temp = bv_to_temp_k(st.bv_color);
        let st_lum = 10.0_f32.powf((4.83 - st.abs_mag) / 2.5);
        let gal_lat_deg = (90.0 - (st.galactic_offset_rad as f64 * 180.0 / PI)).abs();
        let cat_prefix = if self.is_real_hipparcos { "HIP " } else { "Star #" };
        let sys_summary = self.planets_summary_text();

        format!(
            "Selected Night-Sky Star: {cat_prefix}{} — {letter}{sub}{lum_cls} ({lum_desc})\n\
             Apparent Magnitude: m_V = {:+.2} (naked-eye limit ≤ +6.0) | Absolute Magnitude: M_V = {:+.2} ({:.1} L☉)\n\
             Distance: {:.1} light-years ({:.1} pc) | Color Index: B-V = {:+.2} (T_eff ≈ {:.0} K) | Galactic Latitude: {:.1}°\n\
             Home Sun: {} ({}) — {:.0} K, {:.2} M☉, {:.2} R☉, {:.2} L☉ | Habitable Orbit: {:.2} AU ({:.0} Earth days/yr, Solar Tides {:.2}x)\n\
             Solar System ({} Planets): {}\n\
             Atmosphere: {} — Daytime Sky: {} ({})\n\
             Sunset: {} ({})\n\
             Main Plant Foliage: {} — {}",
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
            hs.solar_tide_factor,
            self.planets.len(),
            sys_summary,
            self.atmosphere.name(),
            op.sky_color_name,
            op.sky_mechanism,
            op.sunset_color_name,
            op.sunset_mechanism,
            op.plant_color_name,
            op.plant_pigment_desc
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
}
