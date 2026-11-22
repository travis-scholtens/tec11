//! Acoustic parameter tables for the full tec11 IPA phoneme inventory.
//!
//! Covers all 32 vowels (27 oral + 5 nasalized), all 63 consonants, and all
//! 5 lexical tones from `~/tec11/crates/tec11-geology/src/language.rs`.

#[derive(Clone, Copy, Debug)]
pub struct VowelAcoustic {
    pub symbol: &'static str,
    pub f1: f64,
    pub f2: f64,
    pub f3: f64,
    pub b1: f64,
    pub b2: f64,
    pub b3: f64,
    pub nasal: bool,
    pub fn_hz: f64,
    pub fz_hz: f64,
    pub duration: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsonantKind {
    Stop,
    Affricate,
    Fricative,
    Nasal,
    Liquid,
    Glide,
    Tap,
    Trill,
    GlottalStop,
    Aspirated,
    Ejective,
    Implosive,
    Click,
}

#[derive(Clone, Copy, Debug)]
pub struct ConsonantAcoustic {
    pub symbol: &'static str,
    pub kind: ConsonantKind,
    pub voiced: bool,
    pub f1: f64,
    pub f2: f64,
    pub f3: f64,
    /// Up to 3 parallel frication formants: (freq_hz, bw_hz, amp_db)
    pub fric_formants: &'static [(f64, f64, f64)],
    pub fric_amp: f64,
    pub asp_amp: f64,
    pub duration: f64,
    pub nasal_zero: f64,
    pub labialized: bool,
    pub lateral: bool,
    pub retroflex: bool,
}

const fn v_oral(symbol: &'static str, f1: f64, f2: f64, f3: f64, b1: f64, b2: f64, b3: f64) -> VowelAcoustic {
    VowelAcoustic {
        symbol, f1, f2, f3, b1, b2, b3,
        nasal: false, fn_hz: 280.0, fz_hz: 280.0, duration: 0.115,
    }
}

const fn v_nasal(symbol: &'static str, f1: f64, f2: f64, f3: f64, b1: f64, b2: f64, b3: f64, fn_hz: f64, fz_hz: f64) -> VowelAcoustic {
    VowelAcoustic {
        symbol, f1, f2, f3, b1, b2, b3,
        nasal: true, fn_hz, fz_hz, duration: 0.115,
    }
}

pub const VOWELS: &[VowelAcoustic] = &[
    // Close (height 0)
    v_oral("i",  280.0, 2250.0, 2950.0, 60.0,  90.0, 160.0),
    v_oral("y",  280.0, 1850.0, 2250.0, 60.0,  90.0, 150.0),
    v_oral("ɨ",  290.0, 1550.0, 2500.0, 65.0, 100.0, 160.0),
    v_oral("ʉ",  290.0, 1300.0, 2200.0, 65.0, 100.0, 150.0),
    v_oral("ɯ",  300.0, 1050.0, 2450.0, 65.0, 100.0, 160.0),
    v_oral("u",  300.0,  750.0, 2250.0, 65.0,  90.0, 150.0),
    // Near-close / Close-mid (height 1)
    v_oral("ɪ",  390.0, 1980.0, 2650.0, 70.0, 100.0, 165.0),
    v_oral("ʏ",  390.0, 1650.0, 2250.0, 70.0, 100.0, 155.0),
    v_oral("ʊ",  420.0,  980.0, 2300.0, 70.0, 100.0, 160.0),
    v_oral("e",  400.0, 2080.0, 2720.0, 70.0, 100.0, 165.0),
    v_oral("ø",  400.0, 1650.0, 2250.0, 70.0, 100.0, 155.0),
    v_oral("ɘ",  430.0, 1500.0, 2480.0, 75.0, 110.0, 165.0),
    v_oral("ɵ",  430.0, 1220.0, 2250.0, 75.0, 110.0, 155.0),
    v_oral("ɤ",  430.0, 1050.0, 2450.0, 75.0, 110.0, 165.0),
    v_oral("o",  420.0,  820.0, 2300.0, 70.0,  95.0, 155.0),
    // Mid (height 2)
    v_oral("e̞",  490.0, 1950.0, 2620.0, 75.0, 105.0, 170.0),
    v_oral("ø̞",  490.0, 1560.0, 2260.0, 75.0, 105.0, 160.0),
    VowelAcoustic {
        symbol: "ə", f1: 500.0, f2: 1480.0, f3: 2480.0, b1: 80.0, b2: 110.0, b3: 170.0,
        nasal: false, fn_hz: 280.0, fz_hz: 280.0, duration: 0.090,
    },
    v_oral("ɤ̞",  500.0, 1080.0, 2450.0, 80.0, 110.0, 170.0),
    v_oral("o̞",  500.0,  880.0, 2350.0, 75.0, 100.0, 160.0),
    // Open-mid (height 3)
    v_oral("ɛ",  580.0, 1820.0, 2550.0, 80.0, 110.0, 175.0),
    v_oral("œ",  580.0, 1480.0, 2280.0, 80.0, 110.0, 165.0),
    v_oral("ɜ",  580.0, 1420.0, 2480.0, 85.0, 115.0, 175.0),
    v_oral("ʌ",  600.0, 1150.0, 2450.0, 85.0, 115.0, 175.0),
    v_oral("ɔ",  580.0,  920.0, 2400.0, 80.0, 105.0, 165.0),
    // Open (height 4)
    v_oral("æ",  690.0, 1700.0, 2500.0, 90.0, 120.0, 180.0),
    v_oral("a",  740.0, 1220.0, 2500.0, 90.0, 120.0, 180.0),
    // Nasalized vowel series (indices 27..32)
    v_nasal("ĩ", 300.0, 2200.0, 2900.0,  90.0, 120.0, 180.0, 270.0, 520.0),
    v_nasal("ẽ", 500.0, 1900.0, 2600.0,  95.0, 130.0, 185.0, 280.0, 680.0),
    v_nasal("ã", 720.0, 1200.0, 2500.0, 110.0, 140.0, 190.0, 290.0, 900.0),
    v_nasal("õ", 490.0,  860.0, 2350.0,  95.0, 125.0, 180.0, 280.0, 650.0),
    v_nasal("ũ", 320.0,  780.0, 2250.0,  90.0, 120.0, 175.0, 270.0, 520.0),
];

use ConsonantKind::*;

macro_rules! cdef {
    ($sym:expr, $kind:expr, $voiced:expr, $f1:expr, $f2:expr, $f3:expr, $ff:expr, $fa:expr, $aa:expr, $dur:expr) => {
        ConsonantAcoustic {
            symbol: $sym, kind: $kind, voiced: $voiced, f1: $f1, f2: $f2, f3: $f3,
            fric_formants: $ff, fric_amp: $fa, asp_amp: $aa, duration: $dur,
            nasal_zero: 1000.0, labialized: false, lateral: false, retroflex: false,
        }
    };
}

pub const CONSONANTS: &[ConsonantAcoustic] = &[
    // Core plosives
    cdef!("p", Stop, false, 200.0,  750.0, 2100.0, &[(1200.0, 800.0, 34.0), (3200.0, 1000.0, 30.0)], 33.0, 36.0, 0.085),
    cdef!("b", Stop, true,  200.0,  750.0, 2100.0, &[(1100.0, 700.0, 32.0)], 26.0, 0.0, 0.075),
    cdef!("t", Stop, false, 200.0, 1750.0, 2700.0, &[(3800.0, 900.0, 35.0), (5200.0, 1000.0, 32.0)], 35.0, 38.0, 0.085),
    cdef!("d", Stop, true,  200.0, 1750.0, 2700.0, &[(3600.0, 900.0, 32.0)], 27.0, 0.0, 0.075),
    cdef!("c", Stop, false, 200.0, 2250.0, 3000.0, &[(3100.0, 700.0, 35.0), (4400.0,  900.0, 32.0)], 35.0, 38.0, 0.090),
    cdef!("ɟ", Stop, true,  200.0, 2250.0, 3000.0, &[(3000.0, 700.0, 32.0)], 27.0, 0.0, 0.080),
    cdef!("k", Stop, false, 200.0, 1350.0, 2200.0, &[(1800.0, 600.0, 35.0), (3200.0,  800.0, 31.0)], 35.0, 38.0, 0.090),
    cdef!("g", Stop, true,  200.0, 1350.0, 2200.0, &[(1700.0, 600.0, 32.0)], 27.0, 0.0, 0.078),
    cdef!("q", Stop, false, 260.0,  950.0, 2350.0, &[(1150.0, 500.0, 35.0), (2400.0,  700.0, 31.0)], 35.0, 38.0, 0.095),
    ConsonantAcoustic {
        symbol: "kʷ", kind: Stop, voiced: false, f1: 200.0, f2: 1000.0, f3: 2150.0,
        fric_formants: &[(1400.0, 550.0, 35.0), (2800.0, 800.0, 30.0)],
        fric_amp: 35.0, asp_amp: 38.0, duration: 0.105,
        nasal_zero: 1000.0, labialized: true, lateral: false, retroflex: false,
    },
    ConsonantAcoustic {
        symbol: "gʷ", kind: Stop, voiced: true, f1: 200.0, f2: 1000.0, f3: 2150.0,
        fric_formants: &[(1350.0, 550.0, 32.0)],
        fric_amp: 27.0, asp_amp: 0.0, duration: 0.095,
        nasal_zero: 1000.0, labialized: true, lateral: false, retroflex: false,
    },
    // Glottals
    cdef!("ʔ", GlottalStop, false, 500.0, 1450.0, 2450.0, &[], 0.0, 0.0, 0.065),
    cdef!("h", Fricative,   false, 500.0, 1450.0, 2450.0, &[(1500.0, 900.0, 28.0)], 20.0, 48.0, 0.085),
    // Sibilants
    cdef!("s", Fricative, false, 300.0, 1750.0, 2700.0, &[(4800.0, 700.0, 35.0), (6200.0, 800.0, 34.0)], 35.0, 0.0, 0.115),
    cdef!("z", Fricative, true,  300.0, 1750.0, 2700.0, &[(4600.0, 700.0, 34.0), (6000.0, 800.0, 32.0)], 31.0, 0.0, 0.095),
    cdef!("ʃ", Fricative, false, 300.0, 2050.0, 2650.0, &[(2600.0, 600.0, 35.0), (3800.0, 800.0, 33.0)], 34.0, 0.0, 0.115),
    cdef!("ʒ", Fricative, true,  300.0, 2050.0, 2650.0, &[(2500.0, 600.0, 33.0), (3700.0, 800.0, 31.0)], 30.0, 0.0, 0.095),
    // Non-sibilant fricatives
    cdef!("f", Fricative, false, 300.0, 1100.0, 2250.0, &[(1800.0, 1200.0, 32.0), (4500.0, 1500.0, 33.0)], 27.0, 0.0, 0.105),
    cdef!("v", Fricative, true,  300.0, 1100.0, 2250.0, &[(1800.0, 1200.0, 30.0), (4200.0, 1400.0, 30.0)], 25.0, 0.0, 0.085),
    cdef!("β", Fricative, true,  320.0,  950.0, 2200.0, &[(1200.0,  900.0, 30.0), (3000.0, 1200.0, 28.0)], 24.0, 0.0, 0.080),
    cdef!("θ", Fricative, false, 300.0, 1500.0, 2550.0, &[(2200.0, 1200.0, 31.0), (5000.0, 1500.0, 32.0)], 27.0, 0.0, 0.105),
    cdef!("ð", Fricative, true,  300.0, 1500.0, 2550.0, &[(2200.0, 1200.0, 29.0), (4800.0, 1400.0, 29.0)], 24.0, 0.0, 0.080),
    ConsonantAcoustic {
        symbol: "ɬ", kind: Fricative, voiced: false, f1: 320.0, f2: 1650.0, f3: 2550.0,
        fric_formants: &[(2100.0, 500.0, 34.0), (3600.0, 700.0, 34.0), (5000.0, 900.0, 31.0)],
        fric_amp: 32.0, asp_amp: 0.0, duration: 0.115,
        nasal_zero: 1000.0, labialized: false, lateral: true, retroflex: false,
    },
    cdef!("ç", Fricative, false, 280.0, 2250.0, 3050.0, &[(3000.0, 550.0, 34.0), (4400.0, 750.0, 32.0)], 32.0, 0.0, 0.110),
    cdef!("x", Fricative, false, 350.0, 1250.0, 2300.0, &[(1550.0, 550.0, 34.0), (3000.0, 900.0, 30.0)], 32.0, 0.0, 0.110),
    cdef!("ɣ", Fricative, true,  350.0, 1250.0, 2300.0, &[(1500.0, 550.0, 32.0), (2900.0, 900.0, 28.0)], 28.0, 0.0, 0.090),
    cdef!("χ", Fricative, false, 450.0, 1000.0, 2350.0, &[(1100.0, 450.0, 35.0), (2350.0, 700.0, 31.0)], 33.0, 0.0, 0.115),
    cdef!("ʁ", Fricative, true,  450.0, 1000.0, 2350.0, &[(1050.0, 450.0, 32.0), (2300.0, 700.0, 29.0)], 29.0, 0.0, 0.090),
    cdef!("ħ", Fricative, false, 680.0, 1180.0, 2450.0, &[(1250.0, 500.0, 33.0), (2450.0, 700.0, 32.0)], 30.0, 46.0, 0.110),
    cdef!("ʕ", Fricative, true,  680.0, 1180.0, 2450.0, &[(1200.0, 500.0, 30.0), (2400.0, 700.0, 29.0)], 26.0, 38.0, 0.095),
    // Nasals
    ConsonantAcoustic {
        symbol: "m", kind: Nasal, voiced: true, f1: 270.0, f2: 950.0, f3: 2200.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.085,
        nasal_zero: 1050.0, labialized: false, lateral: false, retroflex: false,
    },
    ConsonantAcoustic {
        symbol: "n", kind: Nasal, voiced: true, f1: 270.0, f2: 1650.0, f3: 2600.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.085,
        nasal_zero: 1750.0, labialized: false, lateral: false, retroflex: false,
    },
    ConsonantAcoustic {
        symbol: "ɲ", kind: Nasal, voiced: true, f1: 270.0, f2: 2200.0, f3: 2950.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.090,
        nasal_zero: 2350.0, labialized: false, lateral: false, retroflex: false,
    },
    ConsonantAcoustic {
        symbol: "ŋ", kind: Nasal, voiced: true, f1: 270.0, f2: 1250.0, f3: 2350.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.090,
        nasal_zero: 3000.0, labialized: false, lateral: false, retroflex: false,
    },
    // Liquids & taps
    ConsonantAcoustic {
        symbol: "l", kind: Liquid, voiced: true, f1: 340.0, f2: 1400.0, f3: 2650.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.080,
        nasal_zero: 1000.0, labialized: false, lateral: true, retroflex: false,
    },
    cdef!("r", Trill, true, 380.0, 1450.0, 2350.0, &[], 0.0, 0.0, 0.105),
    cdef!("ɾ", Tap,   true, 350.0, 1550.0, 2450.0, &[], 0.0, 0.0, 0.035),
    // Glides
    cdef!("j", Glide, true, 260.0, 2280.0, 3000.0, &[], 0.0, 0.0, 0.070),
    cdef!("w", Glide, true, 280.0,  680.0, 2200.0, &[], 0.0, 0.0, 0.075),
    // Affricates
    cdef!("tʃ", Affricate, false, 220.0, 2050.0, 2650.0, &[(2600.0, 600.0, 35.0), (3800.0, 800.0, 33.0)], 34.0, 0.0, 0.130),
    cdef!("dʒ", Affricate, true,  220.0, 2050.0, 2650.0, &[(2500.0, 600.0, 33.0), (3700.0, 800.0, 31.0)], 30.0, 0.0, 0.115),
    cdef!("ts", Affricate, false, 220.0, 1750.0, 2700.0, &[(4800.0, 700.0, 35.0), (6200.0, 800.0, 33.0)], 34.0, 0.0, 0.130),
    cdef!("dz", Affricate, true,  220.0, 1750.0, 2700.0, &[(4600.0, 700.0, 33.0), (6000.0, 800.0, 31.0)], 30.0, 0.0, 0.115),
    // Retroflex series
    ConsonantAcoustic {
        symbol: "ʈ", kind: Stop, voiced: false, f1: 220.0, f2: 1550.0, f3: 1850.0,
        fric_formants: &[(2600.0, 700.0, 35.0), (3800.0, 900.0, 32.0)],
        fric_amp: 35.0, asp_amp: 38.0, duration: 0.090,
        nasal_zero: 1000.0, labialized: false, lateral: false, retroflex: true,
    },
    ConsonantAcoustic {
        symbol: "ɖ", kind: Stop, voiced: true, f1: 220.0, f2: 1550.0, f3: 1850.0,
        fric_formants: &[(2500.0, 700.0, 32.0)],
        fric_amp: 27.0, asp_amp: 0.0, duration: 0.080,
        nasal_zero: 1000.0, labialized: false, lateral: false, retroflex: true,
    },
    ConsonantAcoustic {
        symbol: "ʂ", kind: Fricative, voiced: false, f1: 300.0, f2: 1550.0, f3: 1900.0,
        fric_formants: &[(2200.0, 550.0, 35.0), (3300.0, 750.0, 33.0)],
        fric_amp: 34.0, asp_amp: 0.0, duration: 0.115,
        nasal_zero: 1000.0, labialized: false, lateral: false, retroflex: true,
    },
    ConsonantAcoustic {
        symbol: "ɳ", kind: Nasal, voiced: true, f1: 270.0, f2: 1500.0, f3: 1900.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.090,
        nasal_zero: 1600.0, labialized: false, lateral: false, retroflex: true,
    },
    ConsonantAcoustic {
        symbol: "ɭ", kind: Liquid, voiced: true, f1: 350.0, f2: 1350.0, f3: 1900.0,
        fric_formants: &[], fric_amp: 0.0, asp_amp: 0.0, duration: 0.085,
        nasal_zero: 1000.0, labialized: false, lateral: true, retroflex: true,
    },
    // Aspirated stops
    cdef!("pʰ", Aspirated, false, 200.0,  750.0, 2100.0, &[(1200.0, 800.0, 34.0), (3200.0, 1000.0, 30.0)], 34.0, 49.0, 0.125),
    cdef!("tʰ", Aspirated, false, 200.0, 1750.0, 2700.0, &[(3800.0, 900.0, 35.0), (5200.0, 1000.0, 32.0)], 35.0, 49.0, 0.125),
    cdef!("kʰ", Aspirated, false, 200.0, 1350.0, 2200.0, &[(1800.0, 600.0, 35.0), (3200.0,  800.0, 31.0)], 35.0, 49.0, 0.130),
    // Ejectives
    cdef!("pʼ",  Ejective, false, 200.0,  750.0, 2100.0, &[(1300.0, 650.0, 37.0), (3400.0, 800.0, 34.0)], 39.0, 0.0, 0.115),
    cdef!("tʼ",  Ejective, false, 200.0, 1750.0, 2700.0, &[(4000.0, 750.0, 37.0), (5400.0, 850.0, 34.0)], 40.0, 0.0, 0.115),
    cdef!("kʼ",  Ejective, false, 200.0, 1350.0, 2200.0, &[(1900.0, 500.0, 37.0), (3300.0, 700.0, 34.0)], 40.0, 0.0, 0.120),
    cdef!("tsʼ", Ejective, false, 220.0, 1750.0, 2700.0, &[(4800.0, 700.0, 37.0), (6200.0, 800.0, 34.0)], 38.0, 0.0, 0.140),
    cdef!("tʃʼ", Ejective, false, 220.0, 2050.0, 2650.0, &[(2600.0, 600.0, 37.0), (3800.0, 800.0, 34.0)], 38.0, 0.0, 0.140),
    // Implosives
    cdef!("ɓ", Implosive, true, 200.0,  750.0, 2100.0, &[(1000.0, 700.0, 31.0)], 25.0, 0.0, 0.095),
    cdef!("ɗ", Implosive, true, 200.0, 1750.0, 2700.0, &[(3400.0, 900.0, 31.0)], 26.0, 0.0, 0.095),
    cdef!("ɠ", Implosive, true, 200.0, 1350.0, 2200.0, &[(1600.0, 600.0, 31.0)], 26.0, 0.0, 0.100),
    // Clicks
    cdef!("ǀ", Click, false, 250.0, 1800.0, 2700.0, &[(4200.0, 700.0, 38.0), (5800.0, 800.0, 36.0)], 41.0, 0.0, 0.075),
    cdef!("ǃ", Click, false, 250.0, 1450.0, 2300.0, &[(1650.0, 400.0, 39.0), (2600.0, 600.0, 35.0)], 42.0, 0.0, 0.070),
    ConsonantAcoustic {
        symbol: "ǁ", kind: Click, voiced: false, f1: 280.0, f2: 1600.0, f3: 2500.0,
        fric_formants: &[(2600.0, 600.0, 37.0), (4000.0, 800.0, 35.0)],
        fric_amp: 39.0, asp_amp: 0.0, duration: 0.085,
        nasal_zero: 1000.0, labialized: false, lateral: true, retroflex: false,
    },
    cdef!("ǂ", Click, false, 240.0, 2250.0, 3000.0, &[(3600.0, 550.0, 38.0), (5000.0, 750.0, 35.0)], 41.0, 0.0, 0.070),
];

/// Tone contours: (symbol, start_f0_ratio, end_f0_ratio).
/// Ordered longest-first so `˩˥` and `˥˩` match before `˩` / `˥`.
pub const TONES: &[(&str, f64, f64)] = &[
    ("˩˥", 0.80, 1.32), // Rising
    ("˥˩", 1.34, 0.76), // Falling
    ("˥",  1.30, 1.28), // High
    ("˧",  1.00, 0.98), // Mid
    ("˩",  0.78, 0.75), // Low
];

pub fn find_vowel(sym: &str) -> Option<&'static VowelAcoustic> {
    VOWELS.iter().find(|v| v.symbol == sym)
}

pub fn find_consonant(sym: &str) -> Option<&'static ConsonantAcoustic> {
    CONSONANTS.iter().find(|c| c.symbol == sym)
}

pub fn find_tone(sym: &str) -> Option<(f64, f64)> {
    TONES
        .iter()
        .find(|&&(s, _, _)| s == sym)
        .map(|&(_, r0, r1)| (r0, r1))
}
