use std::collections::BTreeSet;
use std::sync::Arc;

use tec11_math_grid::Pcg64Mcg;

pub const ALL_VOWEL_INDICES: [usize; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];

pub const ALL_ORAL_VOWEL_INDICES: [usize; 27] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26,
];

pub const ALL_CONSONANT_INDICES: [usize; 63] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62,
];

/// Core pulmonic consonant indices sampled independently by PHOIBLE frequency weight.
/// Marked phonological series (retroflex, aspirated, ejective, implosive, click) are sampled
/// as coherent series in `generate_phonemes` rather than isolated stray phonemes.
const CORE_CONSONANT_INDICES: [usize; 43] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
];
pub const RETROFLEX_SERIES: [usize; 5] = [43, 44, 45, 46, 47];
pub const ASPIRATED_SERIES: [usize; 3] = [48, 49, 50];
pub const EJECTIVE_SERIES: [usize; 5] = [51, 52, 53, 54, 55];
pub const IMPLOSIVE_SERIES: [usize; 3] = [56, 57, 58];
pub const CLICK_SERIES: [usize; 4] = [59, 60, 61, 62];
pub const NASAL_VOWEL_SERIES: [usize; 5] = [27, 28, 29, 30, 31];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VowelDef {
    pub symbol: &'static str,
    pub orth: &'static str,
    pub orth_acute: &'static str,
    pub height: i32,
    pub backness: i32,
    pub roundedness: i32,
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConsonantDef {
    pub symbol: &'static str,
    pub orth: &'static str,
    pub manner: i32,
    pub place: i32,
    pub voice: i32,
    pub weight: f64,
}

pub const VOWELS: [VowelDef; 32] = [
    VowelDef { symbol: "i", orth: "i", orth_acute: "í", height: 0, backness: 0, roundedness: 0, weight: 0.871 },
    VowelDef { symbol: "y", orth: "ü", orth_acute: "ǘ", height: 0, backness: 0, roundedness: 1, weight: 0.053 },
    VowelDef { symbol: "ɨ", orth: "ï", orth_acute: "Ḯ", height: 0, backness: 2, roundedness: 0, weight: 0.160 },
    VowelDef { symbol: "ʉ", orth: "ü", orth_acute: "ǘ", height: 0, backness: 2, roundedness: 1, weight: 0.015 },
    VowelDef { symbol: "ɯ", orth: "ï", orth_acute: "Ḯ", height: 0, backness: 4, roundedness: 0, weight: 0.091 },
    VowelDef { symbol: "u", orth: "u", orth_acute: "ú", height: 0, backness: 4, roundedness: 1, weight: 0.818 },
    VowelDef { symbol: "ɪ", orth: "i", orth_acute: "í", height: 1, backness: 1, roundedness: 0, weight: 0.162 },
    VowelDef { symbol: "ʏ", orth: "ü", orth_acute: "ǘ", height: 1, backness: 1, roundedness: 1, weight: 0.013 },
    VowelDef { symbol: "ʊ", orth: "u", orth_acute: "ú", height: 1, backness: 3, roundedness: 1, weight: 0.151 },
    VowelDef { symbol: "e", orth: "e", orth_acute: "é", height: 1, backness: 0, roundedness: 0, weight: 0.275 },
    VowelDef { symbol: "ø", orth: "ö", orth_acute: "ő", height: 1, backness: 0, roundedness: 1, weight: 0.029 },
    VowelDef { symbol: "ɘ", orth: "ë", orth_acute: "é", height: 1, backness: 2, roundedness: 0, weight: 0.004 },
    VowelDef { symbol: "ɵ", orth: "ö", orth_acute: "ő", height: 1, backness: 2, roundedness: 1, weight: 0.004 },
    VowelDef { symbol: "ɤ", orth: "ë", orth_acute: "é", height: 1, backness: 4, roundedness: 0, weight: 0.016 },
    VowelDef { symbol: "o", orth: "o", orth_acute: "ó", height: 1, backness: 4, roundedness: 1, weight: 0.291 },
    VowelDef { symbol: "e̞", orth: "e", orth_acute: "é", height: 2, backness: 0, roundedness: 0, weight: 0.377 },
    VowelDef { symbol: "ø̞", orth: "ö", orth_acute: "ő", height: 2, backness: 0, roundedness: 1, weight: 0.018 },
    VowelDef { symbol: "ə", orth: "ë", orth_acute: "é", height: 2, backness: 2, roundedness: 0, weight: 0.213 },
    VowelDef { symbol: "ɤ̞", orth: "ë", orth_acute: "é", height: 2, backness: 4, roundedness: 0, weight: 0.016 },
    VowelDef { symbol: "o̞", orth: "o", orth_acute: "ó", height: 2, backness: 4, roundedness: 1, weight: 0.400 },
    VowelDef { symbol: "ɛ", orth: "e", orth_acute: "é", height: 3, backness: 0, roundedness: 0, weight: 0.415 },
    VowelDef { symbol: "œ", orth: "ö", orth_acute: "ő", height: 3, backness: 0, roundedness: 1, weight: 0.018 },
    VowelDef { symbol: "ɜ", orth: "ë", orth_acute: "é", height: 3, backness: 2, roundedness: 0, weight: 0.020 },
    VowelDef { symbol: "ʌ", orth: "ë", orth_acute: "é", height: 3, backness: 4, roundedness: 0, weight: 0.022 },
    VowelDef { symbol: "ɔ", orth: "o", orth_acute: "ó", height: 3, backness: 4, roundedness: 1, weight: 0.359 },
    VowelDef { symbol: "æ", orth: "ä", orth_acute: "á", height: 4, backness: 0, roundedness: 0, weight: 0.086 },
    VowelDef { symbol: "a", orth: "a", orth_acute: "á", height: 4, backness: 2, roundedness: 0, weight: 0.869 },
    // Nasalized vowel series (indices 27..32)
    VowelDef { symbol: "ĩ", orth: "ĩ", orth_acute: "ĩ́", height: 0, backness: 0, roundedness: 0, weight: 0.195 },
    VowelDef { symbol: "ẽ", orth: "ẽ", orth_acute: "ẽ́", height: 2, backness: 0, roundedness: 0, weight: 0.165 },
    VowelDef { symbol: "ã", orth: "ã", orth_acute: "ã́", height: 4, backness: 2, roundedness: 0, weight: 0.220 },
    VowelDef { symbol: "õ", orth: "õ", orth_acute: "ṍ", height: 2, backness: 4, roundedness: 1, weight: 0.175 },
    VowelDef { symbol: "ũ", orth: "ũ", orth_acute: "ṹ", height: 0, backness: 4, roundedness: 1, weight: 0.185 },
];

pub const CONSONANTS: [ConsonantDef; 63] = [
    ConsonantDef { symbol: "p", orth: "p", manner: 0, place: 0, voice: 0, weight: 0.856 },
    ConsonantDef { symbol: "b", orth: "b", manner: 0, place: 0, voice: 1, weight: 0.636 },
    ConsonantDef { symbol: "t", orth: "t", manner: 0, place: 1, voice: 0, weight: 0.973 },
    ConsonantDef { symbol: "d", orth: "d", manner: 0, place: 1, voice: 1, weight: 0.634 },
    ConsonantDef { symbol: "c", orth: "c", manner: 0, place: 2, voice: 0, weight: 0.135 },
    ConsonantDef { symbol: "ɟ", orth: "j", manner: 0, place: 2, voice: 1, weight: 0.106 },
    ConsonantDef { symbol: "k", orth: "k", manner: 0, place: 3, voice: 0, weight: 0.894 },
    ConsonantDef { symbol: "g", orth: "g", manner: 0, place: 3, voice: 1, weight: 0.561 },
    ConsonantDef { symbol: "s", orth: "s", manner: 1, place: 1, voice: 0, weight: 0.825 },
    ConsonantDef { symbol: "z", orth: "z", manner: 1, place: 1, voice: 1, weight: 0.308 },
    ConsonantDef { symbol: "ʃ", orth: "sh", manner: 1, place: 2, voice: 0, weight: 0.415 },
    ConsonantDef { symbol: "ʒ", orth: "zh", manner: 1, place: 2, voice: 1, weight: 0.135 },
    ConsonantDef { symbol: "f", orth: "f", manner: 2, place: 0, voice: 0, weight: 0.439 },
    ConsonantDef { symbol: "v", orth: "v", manner: 2, place: 0, voice: 1, weight: 0.259 },
    ConsonantDef { symbol: "θ", orth: "th", manner: 2, place: 1, voice: 0, weight: 0.040 },
    ConsonantDef { symbol: "ð", orth: "dh", manner: 2, place: 1, voice: 1, weight: 0.055 },
    ConsonantDef { symbol: "x", orth: "kh", manner: 2, place: 3, voice: 0, weight: 0.208 },
    ConsonantDef { symbol: "ɣ", orth: "gh", manner: 2, place: 3, voice: 1, weight: 0.118 },
    ConsonantDef { symbol: "m", orth: "m", manner: 3, place: 0, voice: 1, weight: 0.942 },
    ConsonantDef { symbol: "n", orth: "n", manner: 3, place: 1, voice: 1, weight: 0.967 },
    ConsonantDef { symbol: "ɲ", orth: "nh", manner: 3, place: 2, voice: 1, weight: 0.313 },
    ConsonantDef { symbol: "ŋ", orth: "ng", manner: 3, place: 3, voice: 1, weight: 0.525 },
    ConsonantDef { symbol: "l", orth: "l", manner: 4, place: 1, voice: 1, weight: 0.767 },
    ConsonantDef { symbol: "r", orth: "r", manner: 4, place: 1, voice: 1, weight: 0.727 },
    ConsonantDef { symbol: "j", orth: "y", manner: 5, place: 2, voice: 1, weight: 0.838 },
    ConsonantDef { symbol: "w", orth: "w", manner: 5, place: 0, voice: 1, weight: 0.738 },
    // Glottals (26..28)
    ConsonantDef { symbol: "h", orth: "h", manner: 2, place: 6, voice: 0, weight: 0.620 },
    ConsonantDef { symbol: "ʔ", orth: "'", manner: 0, place: 6, voice: 0, weight: 0.480 },
    // Affricates (28..32, manner 6)
    ConsonantDef { symbol: "tʃ", orth: "ch", manner: 6, place: 2, voice: 0, weight: 0.450 },
    ConsonantDef { symbol: "dʒ", orth: "dj", manner: 6, place: 2, voice: 1, weight: 0.250 },
    ConsonantDef { symbol: "ts", orth: "ts", manner: 6, place: 1, voice: 0, weight: 0.250 },
    ConsonantDef { symbol: "dz", orth: "dz", manner: 6, place: 1, voice: 1, weight: 0.110 },
    // Common pulmonic consonants: tap, lateral fricative, bilabial/palatal fricatives, uvulars, pharyngeals, labialized velars (32..43)
    ConsonantDef { symbol: "ɾ", orth: "r", manner: 4, place: 1, voice: 1, weight: 0.280 },
    ConsonantDef { symbol: "ɬ", orth: "hl", manner: 2, place: 1, voice: 0, weight: 0.095 },
    ConsonantDef { symbol: "β", orth: "bh", manner: 2, place: 0, voice: 1, weight: 0.120 },
    ConsonantDef { symbol: "ç", orth: "hy", manner: 2, place: 2, voice: 0, weight: 0.075 },
    ConsonantDef { symbol: "q", orth: "q", manner: 0, place: 4, voice: 0, weight: 0.160 },
    ConsonantDef { symbol: "χ", orth: "qh", manner: 2, place: 4, voice: 0, weight: 0.095 },
    ConsonantDef { symbol: "ʁ", orth: "rh", manner: 2, place: 4, voice: 1, weight: 0.080 },
    ConsonantDef { symbol: "ħ", orth: "hh", manner: 2, place: 5, voice: 0, weight: 0.045 },
    ConsonantDef { symbol: "ʕ", orth: "ʻ", manner: 2, place: 5, voice: 1, weight: 0.040 },
    ConsonantDef { symbol: "kʷ", orth: "kw", manner: 0, place: 3, voice: 0, weight: 0.180 },
    ConsonantDef { symbol: "gʷ", orth: "gw", manner: 0, place: 3, voice: 1, weight: 0.100 },
    // Retroflex series (43..48)
    ConsonantDef { symbol: "ʈ", orth: "tr", manner: 0, place: 1, voice: 0, weight: 0.110 },
    ConsonantDef { symbol: "ɖ", orth: "dr", manner: 0, place: 1, voice: 1, weight: 0.095 },
    ConsonantDef { symbol: "ʂ", orth: "sr", manner: 1, place: 2, voice: 0, weight: 0.085 },
    ConsonantDef { symbol: "ɳ", orth: "nr", manner: 3, place: 1, voice: 1, weight: 0.090 },
    ConsonantDef { symbol: "ɭ", orth: "lr", manner: 4, place: 1, voice: 1, weight: 0.085 },
    // Aspirated stop series (48..51, manner 7)
    ConsonantDef { symbol: "pʰ", orth: "ph", manner: 7, place: 0, voice: 0, weight: 0.230 },
    ConsonantDef { symbol: "tʰ", orth: "th", manner: 7, place: 1, voice: 0, weight: 0.240 },
    ConsonantDef { symbol: "kʰ", orth: "kh", manner: 7, place: 3, voice: 0, weight: 0.250 },
    // Ejective series (51..56, manner 8)
    ConsonantDef { symbol: "pʼ", orth: "p'", manner: 8, place: 0, voice: 0, weight: 0.110 },
    ConsonantDef { symbol: "tʼ", orth: "t'", manner: 8, place: 1, voice: 0, weight: 0.145 },
    ConsonantDef { symbol: "kʼ", orth: "k'", manner: 8, place: 3, voice: 0, weight: 0.155 },
    ConsonantDef { symbol: "tsʼ", orth: "ts'", manner: 8, place: 1, voice: 0, weight: 0.085 },
    ConsonantDef { symbol: "tʃʼ", orth: "ch'", manner: 8, place: 2, voice: 0, weight: 0.105 },
    // Implosive series (56..59, manner 9)
    ConsonantDef { symbol: "ɓ", orth: "bb", manner: 9, place: 0, voice: 1, weight: 0.115 },
    ConsonantDef { symbol: "ɗ", orth: "dd", manner: 9, place: 1, voice: 1, weight: 0.105 },
    ConsonantDef { symbol: "ɠ", orth: "gg", manner: 9, place: 3, voice: 1, weight: 0.040 },
    // Click series (59..63, manner 10 — onset-only)
    ConsonantDef { symbol: "ǀ", orth: "ǀ", manner: 10, place: 1, voice: 0, weight: 0.025 },
    ConsonantDef { symbol: "ǃ", orth: "ǃ", manner: 10, place: 1, voice: 0, weight: 0.025 },
    ConsonantDef { symbol: "ǁ", orth: "ǁ", manner: 10, place: 1, voice: 0, weight: 0.022 },
    ConsonantDef { symbol: "ǂ", orth: "ǂ", manner: 10, place: 2, voice: 0, weight: 0.018 },
];

#[inline]
fn dist3(a: (i32, i32, i32), b: (i32, i32, i32)) -> f64 {
    let dx = (a.0 - b.0) as f64;
    let dy = (a.1 - b.1) as f64;
    let dz = (a.2 - b.2) as f64;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[inline]
pub fn vowel_pos(v: usize) -> (i32, i32, i32) {
    let vd = &VOWELS[v];
    (vd.height, vd.backness, vd.roundedness)
}

#[inline]
pub fn consonant_pos(c: usize) -> (i32, i32, i32) {
    let cd = &CONSONANTS[c];
    (cd.manner, cd.place, cd.voice)
}

#[inline]
pub fn anaptyxity(v: usize) -> f64 {
    let vp = vowel_pos(v);
    dist3(vp, (2, 2, 0)) + dist3(vp, (0, 0, 0))
}

pub fn nearest_vowel_to_pos(pos: (i32, i32, i32), candidates: &[usize]) -> usize {
    candidates
        .iter()
        .copied()
        .min_by(|&a, &b| {
            let da = dist3(vowel_pos(a), pos);
            let db = dist3(vowel_pos(b), pos);
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    VOWELS[b]
                        .weight
                        .partial_cmp(&VOWELS[a].weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .unwrap_or(0)
}

pub fn nearest_consonant_to_pos(pos: (i32, i32, i32), candidates: &[usize]) -> usize {
    candidates
        .iter()
        .copied()
        .min_by(|&a, &b| {
            let da = dist3(consonant_pos(a), pos);
            let db = dist3(consonant_pos(b), pos);
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    CONSONANTS[b]
                        .weight
                        .partial_cmp(&CONSONANTS[a].weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .unwrap_or(0)
}

/// Generate a language's vowel and consonant inventory (`language/phonemes.py:39-47`),
/// sampling core pulmonic phonemes by PHOIBLE frequency and adding marked phonological
/// series (nasal vowels, retroflexes, aspirates, ejectives, implosives, clicks) as coherent series.
pub fn generate_phonemes(rng: &mut Pcg64Mcg) -> (Vec<usize>, Vec<usize>) {
    let nv_oral = (rng.gauss(7.5, 2.6).round() as usize).clamp(3, 15);
    let nc_core = (rng.gauss(20.0, 6.5).round() as usize).clamp(8, 30);

    let mut vs = BTreeSet::new();
    while vs.len() < nv_oral {
        let idx = ALL_ORAL_VOWEL_INDICES[(rng.next_u64() as usize) % ALL_ORAL_VOWEL_INDICES.len()];
        if rng.uniform(0.0, 1.0) < VOWELS[idx].weight {
            vs.insert(idx);
        }
    }
    // ~22% of languages have a nasalized vowel series (e.g. French, Portuguese, Hindi, Yoruba, Navajo)
    if rng.uniform(0.0, 1.0) < 0.22 {
        for &nv_idx in &NASAL_VOWEL_SERIES {
            if rng.uniform(0.0, 1.0) < 0.78 {
                vs.insert(nv_idx);
            }
        }
    }

    let mut cs = BTreeSet::new();
    while cs.len() < nc_core {
        let idx = CORE_CONSONANT_INDICES[(rng.next_u64() as usize) % CORE_CONSONANT_INDICES.len()];
        if rng.uniform(0.0, 1.0) < CONSONANTS[idx].weight {
            cs.insert(idx);
        }
    }
    // Coherent marked consonant series (matching cross-linguistic feature economy):
    if rng.uniform(0.0, 1.0) < 0.25 {
        for &idx in &ASPIRATED_SERIES {
            if rng.uniform(0.0, 1.0) < 0.85 {
                cs.insert(idx);
            }
        }
    }
    if rng.uniform(0.0, 1.0) < 0.16 {
        for &idx in &EJECTIVE_SERIES {
            if rng.uniform(0.0, 1.0) < 0.75 {
                cs.insert(idx);
            }
        }
    }
    if rng.uniform(0.0, 1.0) < 0.14 {
        for &idx in &RETROFLEX_SERIES {
            if rng.uniform(0.0, 1.0) < 0.72 {
                cs.insert(idx);
            }
        }
    }
    if rng.uniform(0.0, 1.0) < 0.11 {
        for &idx in &IMPLOSIVE_SERIES {
            if rng.uniform(0.0, 1.0) < 0.80 {
                cs.insert(idx);
            }
        }
    }
    if rng.uniform(0.0, 1.0) < 0.04 {
        for &idx in &CLICK_SERIES {
            if rng.uniform(0.0, 1.0) < 0.82 {
                cs.insert(idx);
            }
        }
    }

    (vs.into_iter().collect(), cs.into_iter().collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhonotacticConstraint {
    VoicedAfterNasal,
    MatchPlosiveVoice,
    LiquidsAfterPlosive,
    FricativesBeforePlosive,
    SinglePlosive,
    SingleFricative,
    SingleLiquid,
}

impl PhonotacticConstraint {
    pub const ALL: [Self; 7] = [
        Self::VoicedAfterNasal,
        Self::MatchPlosiveVoice,
        Self::LiquidsAfterPlosive,
        Self::FricativesBeforePlosive,
        Self::SinglePlosive,
        Self::SingleFricative,
        Self::SingleLiquid,
    ];

    pub fn apply(self, cluster: &[usize], is_onset: bool, epenthetic: Option<usize>) -> Vec<Option<usize>> {
        // Returns a sequence where `Some(c)` is consonant `c` and `None` is the epenthetic vowel marker
        // separating split clusters (`phonotactics.py:11-56`).
        let mut out: Vec<Option<usize>> = cluster.iter().copied().map(Some).collect();
        match self {
            Self::VoicedAfterNasal => {
                for i in 0..out.len().saturating_sub(1) {
                    if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                        if CONSONANTS[c0].manner == 3 && CONSONANTS[c1].voice == 0 {
                            let cd = &CONSONANTS[c1];
                            out[i + 1] = Some(nearest_consonant_to_pos((cd.manner, cd.place, 1), &ALL_CONSONANT_INDICES));
                        }
                    }
                }
            }
            Self::MatchPlosiveVoice => {
                for i in 0..out.len().saturating_sub(1) {
                    if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                        if CONSONANTS[c0].manner == 0 && CONSONANTS[c1].manner == 0 && CONSONANTS[c0].voice != CONSONANTS[c1].voice {
                            let cd = &CONSONANTS[c1];
                            out[i + 1] = Some(nearest_consonant_to_pos((cd.manner, cd.place, CONSONANTS[c0].voice), &ALL_CONSONANT_INDICES));
                        }
                    }
                }
            }
            Self::LiquidsAfterPlosive => {
                if is_onset {
                    let mut i = 0;
                    while i + 1 < out.len() {
                        if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                            if CONSONANTS[c0].manner == 4 && CONSONANTS[c1].manner == 0 {
                                if epenthetic.is_some() {
                                    out.insert(i + 1, None);
                                    i += 2;
                                    continue;
                                } else {
                                    out.swap(i, i + 1);
                                }
                            }
                        }
                        i += 1;
                    }
                } else {
                    let mut i = 0;
                    while i + 1 < out.len() {
                        if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                            if CONSONANTS[c0].manner == 0 && CONSONANTS[c1].manner == 4 {
                                if epenthetic.is_some() {
                                    out.insert(i + 1, None);
                                    i += 2;
                                    continue;
                                } else {
                                    out.swap(i, i + 1);
                                }
                            }
                        }
                        i += 1;
                    }
                }
            }
            Self::FricativesBeforePlosive => {
                if is_onset {
                    let mut i = 0;
                    while i + 1 < out.len() {
                        if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                            if CONSONANTS[c0].manner == 0 && matches!(CONSONANTS[c1].manner, 1 | 2) {
                                if epenthetic.is_some() {
                                    out.insert(i + 1, None);
                                    i += 2;
                                    continue;
                                } else {
                                    out.swap(i, i + 1);
                                }
                            }
                        }
                        i += 1;
                    }
                } else {
                    let mut i = 0;
                    while i + 1 < out.len() {
                        if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                            if matches!(CONSONANTS[c0].manner, 1 | 2) && CONSONANTS[c1].manner == 0 {
                                if epenthetic.is_some() {
                                    out.insert(i + 1, None);
                                    i += 2;
                                    continue;
                                } else {
                                    out.swap(i, i + 1);
                                }
                            }
                        }
                        i += 1;
                    }
                }
            }
            Self::SinglePlosive => {
                Self::dedup_manner(&mut out, &[0], epenthetic);
            }
            Self::SingleFricative => {
                Self::dedup_manner(&mut out, &[1, 2], epenthetic);
            }
            Self::SingleLiquid => {
                Self::dedup_manner(&mut out, &[4], epenthetic);
            }
        }
        out
    }

    fn dedup_manner(out: &mut Vec<Option<usize>>, manners: &[i32], epenthetic: Option<usize>) {
        let mut i = 0;
        while i + 1 < out.len() {
            if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                if manners.contains(&CONSONANTS[c0].manner) && manners.contains(&CONSONANTS[c1].manner) {
                    if epenthetic.is_some() {
                        out.insert(i + 1, None);
                        i += 2;
                        continue;
                    } else {
                        out.remove(i + 1);
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
}

/// Lexical tone on a syllable. Proto-languages always start with `Tone::None`;
/// tones arise historically through tonogenesis (`TonogenesisOnset`, `TonogenesisCoda`)
/// and subsequently evolve via `ToneShift`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum Tone {
    #[default]
    None = 0,
    High = 1,
    Mid = 2,
    Low = 3,
    Rising = 4,
    Falling = 5,
}

impl Tone {
    pub fn ipa_symbol(self) -> &'static str {
        match self {
            Self::None => "",
            Self::High => "˥",
            Self::Mid => "˧",
            Self::Low => "˩",
            Self::Rising => "˩˥",
            Self::Falling => "˥˩",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::High => "High",
            Self::Mid => "Mid",
            Self::Low => "Low",
            Self::Rising => "Rising",
            Self::Falling => "Falling",
        }
    }
}

/// Apply a tone diacritic to a vowel's base Romanized orthography string.
fn apply_tone_orth(base: &str, tone: Tone) -> String {
    match tone {
        Tone::None | Tone::Mid => base.to_string(),
        Tone::High => match base {
            "a" => "á".to_string(),
            "e" => "é".to_string(),
            "i" => "í".to_string(),
            "o" => "ó".to_string(),
            "u" => "ú".to_string(),
            _ => format!("{base}\u{0301}"),
        },
        Tone::Low => match base {
            "a" => "à".to_string(),
            "e" => "è".to_string(),
            "i" => "ì".to_string(),
            "o" => "ò".to_string(),
            "u" => "ù".to_string(),
            _ => format!("{base}\u{0300}"),
        },
        Tone::Rising => match base {
            "a" => "ǎ".to_string(),
            "e" => "ě".to_string(),
            "i" => "ǐ".to_string(),
            "o" => "ǒ".to_string(),
            "u" => "ǔ".to_string(),
            _ => format!("{base}\u{030C}"),
        },
        Tone::Falling => match base {
            "a" => "â".to_string(),
            "e" => "ê".to_string(),
            "i" => "î".to_string(),
            "o" => "ô".to_string(),
            "u" => "û".to_string(),
            _ => format!("{base}\u{0302}"),
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Syllable {
    pub onset: Vec<usize>,
    pub nucleus: Vec<usize>,
    pub coda: Vec<usize>,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word {
    pub syllables: Vec<Syllable>,
    /// `1` = ultimate (`syllables.len() - 1`), `2` = penultimate (`syllables.len().saturating_sub(2)`).
    pub stress: usize,
}

impl Word {
    /// Phonemic uniqueness key (`language/lexicon.py:7-8`):
    /// maps vowels to `(true, VOWELS[v].height)` and consonants to `(false, c as i32)`.
    pub fn wordkey(&self) -> Vec<(bool, i32)> {
        let mut key = Vec::with_capacity(self.syllables.len() * 3);
        for s in &self.syllables {
            for &c in &s.onset {
                key.push((false, c as i32));
            }
            for &v in &s.nucleus {
                key.push((true, VOWELS[v].height));
            }
            for &c in &s.coda {
                key.push((false, c as i32));
            }
        }
        key
    }

    pub fn stressed_index(&self) -> usize {
        if self.syllables.len() <= 1 || self.stress <= 1 {
            self.syllables.len().saturating_sub(1)
        } else {
            self.syllables.len().saturating_sub(self.stress)
        }
    }

    pub fn is_tonal(&self) -> bool {
        self.syllables.iter().any(|s| s.tone != Tone::None)
    }

    /// Format in IPA (`language/ipa.py` and `language/output.py:pronounce`).
    pub fn pronounce(&self) -> String {
        let tonal = self.is_tonal();
        let stressed_idx = self.stressed_index();
        let multi = self.syllables.len() > 1;
        let mut out = String::with_capacity(self.syllables.len() * 5);

        for (idx, syl) in self.syllables.iter().enumerate() {
            if !tonal && multi && idx == stressed_idx {
                out.push('ˈ');
            } else if idx > 0 {
                out.push('.');
            }
            for &c in &syl.onset {
                out.push_str(CONSONANTS[c].symbol);
            }
            if syl.nucleus.len() == 2 && syl.nucleus[0] == syl.nucleus[1] {
                out.push_str(VOWELS[syl.nucleus[0]].symbol);
                out.push('ː');
            } else {
                for &v in &syl.nucleus {
                    out.push_str(VOWELS[v].symbol);
                }
            }
            for &c in &syl.coda {
                out.push_str(CONSONANTS[c].symbol);
            }
            if syl.tone != Tone::None {
                out.push_str(syl.tone.ipa_symbol());
            }
        }
        out
    }

    /// Format in Romanized orthography (`language/orthography.py` and `language/output.py:write`).
    pub fn write(&self, capitalize: bool) -> String {
        let tonal = self.is_tonal();
        let stressed_idx = self.stressed_index();
        let mark_stress = !tonal && self.stress != 1 && self.syllables.len() > 1;
        let mut out = String::with_capacity(self.syllables.len() * 5);
        let mut need_cap = capitalize;

        let mut push_segment = |seg: &str| {
            if need_cap && !seg.is_empty() {
                for ch in seg.chars() {
                    if need_cap && ch.is_lowercase() {
                        for up in ch.to_uppercase() {
                            out.push(up);
                        }
                        need_cap = false;
                    } else {
                        out.push(ch);
                    }
                }
                return;
            }
            out.push_str(seg);
        };

        for (idx, syl) in self.syllables.iter().enumerate() {
            for &c in &syl.onset {
                push_segment(CONSONANTS[c].orth);
            }
            for (v_pos, &v) in syl.nucleus.iter().enumerate() {
                if tonal && v_pos == 0 && syl.tone != Tone::None {
                    let toned = apply_tone_orth(VOWELS[v].orth, syl.tone);
                    push_segment(&toned);
                } else if mark_stress && idx == stressed_idx && v_pos == 0 {
                    push_segment(VOWELS[v].orth_acute);
                } else {
                    push_segment(VOWELS[v].orth);
                }
            }
            for &c in &syl.coda {
                push_segment(CONSONANTS[c].orth);
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct Language {
    pub vowels: Vec<usize>,
    pub consonants: Vec<usize>,
    /// Active lexical tones in this language. Always empty for newly generated proto-languages;
    /// populated dynamically via `refresh_from_word_iter` once tonogenesis occurs.
    pub tones: Vec<Tone>,
    pub onsetp: Vec<f64>,
    pub codap: Vec<f64>,
    pub constraints: Vec<PhonotacticConstraint>,
    pub attested_onsets: BTreeSet<Vec<usize>>,
    pub attested_codae: BTreeSet<Vec<usize>>,
    pub stress: usize,
}

impl Language {
    pub fn generate(rng: &mut Pcg64Mcg) -> Self {
        let (vowels, consonants) = generate_phonemes(rng);
        let onset_prob = rng.gauss(0.5, 0.1).clamp(0.15, 0.95);
        let coda_prob = rng.gauss(0.5, 0.1).clamp(0.10, 0.85);
        let onsetp = vec![onset_prob];
        let codap = vec![coda_prob];

        let num_constraints =
            3 + ((rng.next_u64() as usize) % (PhonotacticConstraint::ALL.len() - 2));
        let mut c_indices: Vec<usize> = (0..PhonotacticConstraint::ALL.len()).collect();
        for i in (1..c_indices.len()).rev() {
            let j = (rng.next_u64() as usize) % (i + 1);
            c_indices.swap(i, j);
        }
        let constraints = c_indices
            .into_iter()
            .take(num_constraints)
            .map(|idx| PhonotacticConstraint::ALL[idx])
            .collect();

        let stress = if rng.gauss(-0.5, 1.0).round() as i32 == -1 {
            1
        } else {
            2
        };

        Self {
            vowels,
            consonants,
            tones: Vec::new(),
            onsetp,
            codap,
            constraints,
            attested_onsets: BTreeSet::new(),
            attested_codae: BTreeSet::new(),
            stress,
        }
    }

    /// Recompute emergent phoneme inventory, syllable onset/coda probabilities, and attested
    /// onset/coda cluster constraints from the living lexicon (`language/stats.py:3-33`, `phonotactics.py:9-15`).
    pub fn refresh_from_lexicon(&mut self, words: &[Word]) {
        self.refresh_from_word_iter(words.iter());
    }

    pub fn refresh_from_word_iter<'a>(&mut self, words: impl Iterator<Item = &'a Word>) {
        let mut vs = [false; VOWELS.len()];
        let mut cs = [false; CONSONANTS.len()];
        let mut active_tones = BTreeSet::new();
        let mut onsets = BTreeSet::new();
        let mut codae = BTreeSet::new();
        let mut num_syllables = 0usize;
        let mut num_onsets = 0usize;
        let mut num_codas = 0usize;
        let mut any_word = false;

        for w in words {
            any_word = true;
            for s in &w.syllables {
                if s.tone != Tone::None {
                    active_tones.insert(s.tone);
                }
                for &c in &s.onset {
                    cs[c] = true;
                }
                for &v in &s.nucleus {
                    vs[v] = true;
                }
                for &c in &s.coda {
                    cs[c] = true;
                }
                if !s.onset.is_empty() {
                    num_onsets += 1;
                }
                if !s.coda.is_empty() {
                    num_codas += 1;
                }
                num_syllables += 1;
                if !onsets.contains(&s.onset) {
                    onsets.insert(s.onset.clone());
                }
                if !codae.contains(&s.coda) {
                    codae.insert(s.coda.clone());
                }
            }
        }
        if !any_word {
            return;
        }
        onsets.insert(Vec::new());
        codae.insert(Vec::new());

        if vs.iter().any(|&b| b) {
            self.vowels.clear();
            for (idx, &present) in vs.iter().enumerate() {
                if present {
                    self.vowels.push(idx);
                }
            }
        }
        if cs.iter().any(|&b| b) {
            self.consonants.clear();
            for (idx, &present) in cs.iter().enumerate() {
                if present {
                    self.consonants.push(idx);
                }
            }
        }
        if num_syllables > 0 {
            let op = (num_onsets as f64 / num_syllables as f64).clamp(0.1, 0.95);
            let cp = (num_codas as f64 / num_syllables as f64).clamp(0.05, 0.90);
            if self.onsetp.len() == 1 {
                self.onsetp[0] = op;
            } else {
                self.onsetp = vec![op];
            }
            if self.codap.len() == 1 {
                self.codap[0] = cp;
            } else {
                self.codap = vec![cp];
            }
        }
        self.tones = active_tones.into_iter().collect();
        self.attested_onsets = onsets;
        self.attested_codae = codae;
    }

    pub fn epenthetic_vowel(&self) -> usize {
        self.vowels
            .iter()
            .copied()
            .min_by(|&a, &b| {
                anaptyxity(a)
                    .partial_cmp(&anaptyxity(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0)
    }

    pub fn generate_syllable(&self, rng: &mut Pcg64Mcg) -> Syllable {
        let mut onset = Vec::new();
        for &p in &self.onsetp {
            if rng.uniform(0.0, 1.0) < p && !self.consonants.is_empty() {
                let c = self.consonants[(rng.next_u64() as usize) % self.consonants.len()];
                onset.push(c);
            }
        }
        for c_rule in &self.constraints {
            onset = c_rule
                .apply(&onset, true, None)
                .into_iter()
                .flatten()
                .collect();
        }

        let v = if self.vowels.is_empty() {
            0
        } else {
            self.vowels[(rng.next_u64() as usize) % self.vowels.len()]
        };
        let nucleus = vec![v];

        let mut coda = Vec::new();
        for &p in &self.codap {
            if rng.uniform(0.0, 1.0) < p && !self.consonants.is_empty() {
                let c = self.consonants[(rng.next_u64() as usize) % self.consonants.len()];
                // Clicks (manner 10) are onset-only in natural languages.
                if CONSONANTS[c].manner != 10 {
                    coda.push(c);
                }
            }
        }
        for c_rule in &self.constraints {
            coda = c_rule
                .apply(&coda, false, None)
                .into_iter()
                .flatten()
                .collect();
        }

        let tone = if self.tones.is_empty() {
            Tone::None
        } else {
            self.tones[(rng.next_u64() as usize) % self.tones.len()]
        };

        Syllable {
            onset,
            nucleus,
            coda,
            tone,
        }
    }

    /// Grow a new word syllable-by-syllable until its `wordkey` (consonants + vowel heights) is unique
    /// (`language/lexicon.py:10-28`).
    pub fn generate_word_unique(
        &self,
        seen_keys: &mut BTreeSet<Vec<(bool, i32)>>,
        existing_words: &BTreeSet<Word>,
        rng: &mut Pcg64Mcg,
    ) -> Word {
        let mut ss = Vec::new();
        for _ in 0..6 {
            let s = self.generate_syllable(rng);
            let constrained_s = self.constrain(&Word {
                syllables: vec![s],
                stress: self.stress,
            });
            ss.extend(constrained_s.syllables);
            let w = Word {
                syllables: ss.clone(),
                stress: self.stress,
            };
            let key = w.wordkey();
            if !seen_keys.contains(&key) && !existing_words.contains(&w) {
                seen_keys.insert(key);
                return w;
            }
            if ss.len() >= 4 {
                ss.clear();
            }
        }
        let w = Word {
            syllables: if ss.is_empty() {
                vec![self.generate_syllable(rng)]
            } else {
                ss
            },
            stress: self.stress,
        };
        seen_keys.insert(w.wordkey());
        w
    }

    pub fn generate_word(&self, rng: &mut Pcg64Mcg) -> Word {
        let mut syllables = vec![self.generate_syllable(rng)];
        while syllables.len() < 4 && rng.uniform(0.0, 1.0) < 0.45 {
            syllables.push(self.generate_syllable(rng));
        }
        Word {
            syllables,
            stress: self.stress,
        }
    }

    /// Adapt foreign phonemes in `word` to the nearest phonemes in `self` (`lexicon.py:18-24`).
    pub fn adapt_sounds(&self, word: &Word) -> Word {
        let default_tone = self.tones.first().copied().unwrap_or(Tone::None);
        let syllables = word
            .syllables
            .iter()
            .map(|s| Syllable {
                onset: s
                    .onset
                    .iter()
                    .map(|&c| nearest_consonant_to_pos(consonant_pos(c), &self.consonants))
                    .collect(),
                nucleus: s
                    .nucleus
                    .iter()
                    .map(|&v| nearest_vowel_to_pos(vowel_pos(v), &self.vowels))
                    .collect(),
                coda: s
                    .coda
                    .iter()
                    .map(|&c| nearest_consonant_to_pos(consonant_pos(c), &self.consonants))
                    .collect(),
                tone: if self.tones.is_empty() {
                    Tone::None
                } else if self.tones.contains(&s.tone) {
                    s.tone
                } else {
                    default_tone
                },
            })
            .collect();
        Word {
            syllables,
            stress: self.stress,
        }
    }

    /// Constrain a syllable's onset using attested onset/coda clusters (`language/phonotactics.py:23-37`).
    fn constrain_onset_attested(&self, syllable: &Syllable, filler: usize) -> Option<Vec<Syllable>> {
        if syllable.onset.is_empty() {
            return Some(vec![syllable.clone()]);
        }
        let ep_tone = if self.tones.is_empty() {
            Tone::None
        } else {
            Tone::Mid
        };
        let len = syllable.onset.len();
        for n in (1..=len).rev() {
            let clusters: Vec<Vec<usize>> =
                syllable.onset.chunks(n).map(|c| c.to_vec()).collect();
            if clusters.iter().all(|c| self.attested_onsets.contains(c)) {
                let mut out: Vec<Syllable> = clusters[..clusters.len() - 1]
                    .iter()
                    .map(|c| Syllable {
                        onset: c.clone(),
                        nucleus: vec![filler],
                        coda: Vec::new(),
                        tone: ep_tone,
                    })
                    .collect();
                out.push(Syllable {
                    onset: clusters.last().cloned().unwrap_or_default(),
                    nucleus: syllable.nucleus.clone(),
                    coda: syllable.coda.clone(),
                    tone: syllable.tone,
                });
                return Some(out);
            }
            if self.attested_codae.contains(&clusters[0])
                && clusters[1..].iter().all(|c| self.attested_onsets.contains(c))
            {
                let mut out = vec![Syllable {
                    onset: Vec::new(),
                    nucleus: vec![filler],
                    coda: clusters[0].clone(),
                    tone: ep_tone,
                }];
                if clusters.len() > 2 {
                    for c in &clusters[1..clusters.len() - 1] {
                        out.push(Syllable {
                            onset: c.clone(),
                            nucleus: vec![filler],
                            coda: Vec::new(),
                            tone: ep_tone,
                        });
                    }
                }
                out.push(Syllable {
                    onset: if clusters.len() > 1 {
                        clusters.last().cloned().unwrap_or_default()
                    } else {
                        Vec::new()
                    },
                    nucleus: syllable.nucleus.clone(),
                    coda: syllable.coda.clone(),
                    tone: syllable.tone,
                });
                return Some(out);
            }
        }
        None
    }

    /// Constrain a syllable's coda using attested onset/coda clusters (`language/phonotactics.py:39-53`).
    fn constrain_coda_attested(&self, syllable: &Syllable, filler: usize) -> Option<Vec<Syllable>> {
        if syllable.coda.is_empty() {
            return Some(vec![syllable.clone()]);
        }
        let ep_tone = if self.tones.is_empty() {
            Tone::None
        } else {
            Tone::Mid
        };
        let len = syllable.coda.len();
        for n in (1..=len).rev() {
            let clusters: Vec<Vec<usize>> =
                syllable.coda.chunks(n).map(|c| c.to_vec()).collect();
            if clusters.iter().all(|c| self.attested_codae.contains(c)) {
                let mut out = vec![Syllable {
                    onset: syllable.onset.clone(),
                    nucleus: syllable.nucleus.clone(),
                    coda: clusters[0].clone(),
                    tone: syllable.tone,
                }];
                for c in &clusters[1..] {
                    out.push(Syllable {
                        onset: Vec::new(),
                        nucleus: vec![filler],
                        coda: c.clone(),
                        tone: ep_tone,
                    });
                }
                return Some(out);
            }
            let last_idx = clusters.len() - 1;
            if self.attested_onsets.contains(&clusters[last_idx])
                && clusters[..last_idx]
                    .iter()
                    .all(|c| self.attested_codae.contains(c))
            {
                let mut out = vec![Syllable {
                    onset: syllable.onset.clone(),
                    nucleus: syllable.nucleus.clone(),
                    coda: if clusters.len() > 1 {
                        clusters[0].clone()
                    } else {
                        Vec::new()
                    },
                    tone: syllable.tone,
                }];
                if clusters.len() > 2 {
                    for c in &clusters[1..last_idx] {
                        out.push(Syllable {
                            onset: Vec::new(),
                            nucleus: vec![filler],
                            coda: c.clone(),
                            tone: ep_tone,
                        });
                    }
                }
                out.push(Syllable {
                    onset: clusters[last_idx].clone(),
                    nucleus: vec![filler],
                    coda: Vec::new(),
                    tone: ep_tone,
                });
                return Some(out);
            }
        }
        None
    }

    /// Enforce this language's syllable structure and phonotactic constraints via epenthesis
    /// (`language/phonotactics.py:23-58` and `lexicon.py:27-76`).
    pub fn constrain(&self, word: &Word) -> Word {
        let ep = self.epenthetic_vowel();
        let mut new_syllables = Vec::new();

        // If emergent attested cluster constraints exist (`stats.Language`), try `constrainonset` + `constraincoda` first
        if !self.attested_onsets.is_empty() && !self.attested_codae.is_empty() {
            let mut all_matched = true;
            let mut attested_syllables = Vec::new();
            for s in &word.syllables {
                if let Some(mut onset_ss) = self.constrain_onset_attested(s, ep) {
                    if let Some(last_s) = onset_ss.pop() {
                        if let Some(coda_ss) = self.constrain_coda_attested(&last_s, ep) {
                            onset_ss.extend(coda_ss);
                            attested_syllables.extend(onset_ss);
                            continue;
                        }
                    }
                }
                all_matched = false;
                break;
            }
            if all_matched && !attested_syllables.is_empty() {
                return Word {
                    syllables: attested_syllables,
                    stress: self.stress,
                };
            }
        }

        let max_o = self.onsetp.len().max(1);
        let max_c = self.codap.len().max(1);
        let ep_tone = if self.tones.is_empty() {
            Tone::None
        } else {
            Tone::Mid
        };

        for s in &word.syllables {
            let mut onset = s.onset.clone();
            while onset.len() > max_o {
                let extra = onset.drain(0..1).collect();
                new_syllables.push(Syllable {
                    onset: extra,
                    nucleus: vec![ep],
                    coda: Vec::new(),
                    tone: ep_tone,
                });
            }

            let mut onset_seq: Vec<Option<usize>> = onset.into_iter().map(Some).collect();
            for rule in &self.constraints {
                let flat: Vec<usize> = onset_seq.iter().copied().flatten().collect();
                if onset_seq.iter().all(|x| x.is_some()) {
                    onset_seq = rule.apply(&flat, true, Some(ep));
                }
            }
            let mut onset_parts: Vec<Vec<usize>> = vec![Vec::new()];
            for item in onset_seq {
                match item {
                    Some(c) => onset_parts.last_mut().unwrap().push(c),
                    None => onset_parts.push(Vec::new()),
                }
            }
            for part in onset_parts.iter().take(onset_parts.len().saturating_sub(1)) {
                new_syllables.push(Syllable {
                    onset: part.clone(),
                    nucleus: vec![ep],
                    coda: Vec::new(),
                    tone: ep_tone,
                });
            }
            let final_onset = onset_parts.pop().unwrap_or_default();

            let mut coda = s.coda.clone();
            let mut trailing_syllables = Vec::new();
            while coda.len() > max_c {
                let extra = coda.split_off(coda.len() - 1);
                trailing_syllables.insert(
                    0,
                    Syllable {
                        onset: Vec::new(),
                        nucleus: vec![ep],
                        coda: extra,
                        tone: ep_tone,
                    },
                );
            }

            let mut coda_seq: Vec<Option<usize>> = coda.into_iter().map(Some).collect();
            for rule in &self.constraints {
                let flat: Vec<usize> = coda_seq.iter().copied().flatten().collect();
                if coda_seq.iter().all(|x| x.is_some()) {
                    coda_seq = rule.apply(&flat, false, Some(ep));
                }
            }
            let mut coda_parts: Vec<Vec<usize>> = vec![Vec::new()];
            for item in coda_seq {
                match item {
                    Some(c) => coda_parts.last_mut().unwrap().push(c),
                    None => coda_parts.push(Vec::new()),
                }
            }
            let first_coda = coda_parts.remove(0);

            new_syllables.push(Syllable {
                onset: final_onset,
                nucleus: if s.nucleus.is_empty() {
                    vec![ep]
                } else {
                    s.nucleus.clone()
                },
                coda: first_coda,
                tone: s.tone,
            });

            for part in coda_parts {
                new_syllables.push(Syllable {
                    onset: Vec::new(),
                    nucleus: vec![ep],
                    coda: part,
                    tone: ep_tone,
                });
            }
            new_syllables.extend(trailing_syllables);
        }

        if new_syllables.is_empty() {
            new_syllables.push(Syllable {
                onset: Vec::new(),
                nucleus: vec![ep],
                coda: Vec::new(),
                tone: ep_tone,
            });
        }

        Word {
            syllables: new_syllables,
            stress: self.stress,
        }
    }

    pub fn borrow_word(&self, foreign: &Word) -> Word {
        let adapted = self.adapt_sounds(foreign);
        self.constrain(&adapted)
    }

    /// Borrow `foreign` into `self`, avoiding homophone collisions with `existing_words` and falling back
    /// to suffix derivation if adapted forms collide (`language/lazy.py:23-30`).
    pub fn borrow_word_unique(
        &self,
        foreign: &Word,
        existing_words: &BTreeSet<Word>,
        rng: &mut Pcg64Mcg,
    ) -> Word {
        let borrowed = self.borrow_word(foreign);
        if !existing_words.contains(&borrowed) {
            return borrowed;
        }
        self.derive_word_unique(foreign, existing_words, rng)
    }

    /// Derive a new word from `base` by adapting sounds, constraining clusters, and appending a 1-syllable
    /// suffix until unique (`language/lazy.py:11-21`).
    pub fn derive_word_unique(
        &self,
        base: &Word,
        existing_words: &BTreeSet<Word>,
        rng: &mut Pcg64Mcg,
    ) -> Word {
        let adapted = self.constrain(&self.adapt_sounds(base));
        for _ in 0..16 {
            let suffix = self.constrain(&Word {
                syllables: vec![self.generate_syllable(rng)],
                stress: self.stress,
            });
            let mut syllables = adapted.syllables.clone();
            syllables.extend(suffix.syllables);
            let candidate = Word {
                syllables,
                stress: self.stress,
            };
            if !existing_words.contains(&candidate) {
                return candidate;
            }
        }
        self.derive_word(base, rng)
    }

    /// Derive a new word from `base` by appending a 1-syllable suffix (`language/lazy.py:11-21`).
    pub fn derive_word(&self, base: &Word, rng: &mut Pcg64Mcg) -> Word {
        let adapted = self.constrain(&self.adapt_sounds(base));
        let suffix = self.constrain(&Word {
            syllables: vec![self.generate_syllable(rng)],
            stress: self.stress,
        });
        let mut syllables = adapted.syllables;
        syllables.extend(suffix.syllables);
        Word {
            syllables,
            stress: self.stress,
        }
    }
}

/// All 14 historical sound changes (`metaphony.py`, `breaking.py`, `lenition.py`, `elision.py`,
/// onset/coda tonogenesis, diachronic tone shifting, palatalization, and vowel nasalization).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SoundChangeKind {
    AMutate = 0,
    IMutate = 1,
    BreakVowels = 2,
    OpenConsonants = 3,
    SonorizeConsonants = 4,
    VocalizeConsonants = 5,
    Apheresize = 6,
    Syncopate = 7,
    Apocopate = 8,
    TonogenesisOnset = 9,
    TonogenesisCoda = 10,
    ToneShift = 11,
    Palatalize = 12,
    Nasalize = 13,
}

impl SoundChangeKind {
    pub const ALL: [Self; 14] = [
        Self::AMutate,
        Self::IMutate,
        Self::BreakVowels,
        Self::OpenConsonants,
        Self::SonorizeConsonants,
        Self::VocalizeConsonants,
        Self::Apheresize,
        Self::Syncopate,
        Self::Apocopate,
        Self::TonogenesisOnset,
        Self::TonogenesisCoda,
        Self::ToneShift,
        Self::Palatalize,
        Self::Nasalize,
    ];

    /// Regular segmental sound changes (excluding tonogenesis/tone-shift which use their own cadence).
    pub const SEGMENTAL: [Self; 11] = [
        Self::AMutate,
        Self::IMutate,
        Self::BreakVowels,
        Self::OpenConsonants,
        Self::SonorizeConsonants,
        Self::VocalizeConsonants,
        Self::Apheresize,
        Self::Syncopate,
        Self::Apocopate,
        Self::Palatalize,
        Self::Nasalize,
    ];

    pub fn from_u32(v: u32) -> Self {
        match v % 14 {
            0 => Self::AMutate,
            1 => Self::IMutate,
            2 => Self::BreakVowels,
            3 => Self::OpenConsonants,
            4 => Self::SonorizeConsonants,
            5 => Self::VocalizeConsonants,
            6 => Self::Apheresize,
            7 => Self::Syncopate,
            8 => Self::Apocopate,
            9 => Self::TonogenesisOnset,
            10 => Self::TonogenesisCoda,
            11 => Self::ToneShift,
            12 => Self::Palatalize,
            _ => Self::Nasalize,
        }
    }

    pub fn apply(self, word: &Word) -> Word {
        self.apply_opt(word).unwrap_or_else(|| word.clone())
    }

    /// Apply sound change to `word`, returning `Some(mutated_word)` only if `word` actually
    /// changed and `None` (with zero heap allocations) if `word` is unaffected.
    pub fn apply_opt(self, word: &Word) -> Option<Word> {
        match self {
            Self::AMutate => a_mutate_opt(word),
            Self::IMutate => i_mutate_opt(word),
            Self::BreakVowels => break_vowels_opt(word),
            Self::OpenConsonants => open_consonants_opt(word),
            Self::SonorizeConsonants => sonorize_consonants_opt(word),
            Self::VocalizeConsonants => vocalize_consonants_opt(word),
            Self::Apheresize => apheresize_opt(word),
            Self::Syncopate => syncopate_opt(word),
            Self::Apocopate => apocopate_opt(word),
            Self::TonogenesisOnset => tonogenesis_onset_opt(word),
            Self::TonogenesisCoda => tonogenesis_coda_opt(word),
            Self::ToneShift => tone_shift_opt(word),
            Self::Palatalize => palatalize_opt(word),
            Self::Nasalize => nasalize_opt(word),
        }
    }
}

/// Onset-voicing tonogenesis: voiced obstruent onsets devoice and depress pitch to `Tone::Low`,
/// while voiceless obstruent onsets raise pitch to `Tone::High` (register split).
pub fn tonogenesis_onset(word: &Word) -> Word {
    tonogenesis_onset_opt(word).unwrap_or_else(|| word.clone())
}

fn tonogenesis_onset_opt(word: &Word) -> Option<Word> {
    let mut out_opt: Option<Word> = None;
    let mut became_tonal = false;
    for (i, syl) in word.syllables.iter().enumerate() {
        let Some(&c0) = syl.onset.first() else {
            continue;
        };
        let cd = &CONSONANTS[c0];
        if !matches!(cd.manner, 0 | 1 | 2 | 6) || cd.place > 5 {
            continue;
        }
        if cd.voice == 1 {
            let devoiced = nearest_consonant_to_pos((cd.manner, cd.place, 0), &ALL_CONSONANT_INDICES);
            let new_tone = match syl.tone {
                Tone::Falling => Tone::Falling,
                _ => Tone::Low,
            };
            if devoiced != c0 || new_tone != syl.tone {
                let out = out_opt.get_or_insert_with(|| word.clone());
                out.syllables[i].onset[0] = devoiced;
                out.syllables[i].tone = new_tone;
                became_tonal = true;
            }
        } else if matches!(syl.tone, Tone::None | Tone::Mid) {
            let out = out_opt.get_or_insert_with(|| word.clone());
            out.syllables[i].tone = Tone::High;
            became_tonal = true;
        }
    }
    if became_tonal {
        if let Some(ref mut out) = out_opt {
            for syl in &mut out.syllables {
                if syl.tone == Tone::None {
                    syl.tone = Tone::Mid;
                }
            }
        }
    }
    out_opt
}

/// Coda-loss contour tonogenesis: dropping a glottal/voiceless-stop coda yields `Tone::Rising`,
/// while dropping a `-h`/fricative coda yields `Tone::Falling`.
pub fn tonogenesis_coda(word: &Word) -> Word {
    tonogenesis_coda_opt(word).unwrap_or_else(|| word.clone())
}

fn tonogenesis_coda_opt(word: &Word) -> Option<Word> {
    let mut out_opt: Option<Word> = None;
    let mut became_tonal = false;
    for (i, syl) in word.syllables.iter().enumerate() {
        let Some(&c_last) = syl.coda.last() else {
            continue;
        };
        let cd = &CONSONANTS[c_last];
        let new_tone = if c_last == 27 || (matches!(cd.manner, 0 | 7 | 8) && cd.voice == 0) {
            // Glottal stop `ʔ` or voiceless/aspirated/ejective stop coda -> Rising tone
            Some(Tone::Rising)
        } else if c_last == 26 || matches!(cd.manner, 1 | 2) {
            // Glottal `h` or fricative coda -> Falling tone
            Some(Tone::Falling)
        } else {
            None
        };
        if let Some(tone) = new_tone {
            let out = out_opt.get_or_insert_with(|| word.clone());
            out.syllables[i].coda.pop();
            out.syllables[i].tone = tone;
            became_tonal = true;
        }
    }
    if became_tonal {
        if let Some(ref mut out) = out_opt {
            for syl in &mut out.syllables {
                if syl.tone == Tone::None {
                    syl.tone = Tone::Mid;
                }
            }
        }
    }
    out_opt
}

/// Diachronic chain tone shift (`High -> Falling -> Low -> Rising -> High`; `Mid` stays `Mid`).
/// Has no effect on non-tonal words.
pub fn tone_shift(word: &Word) -> Word {
    tone_shift_opt(word).unwrap_or_else(|| word.clone())
}

fn tone_shift_opt(word: &Word) -> Option<Word> {
    if !word.is_tonal() {
        return None;
    }
    let mut out_opt: Option<Word> = None;
    for (i, syl) in word.syllables.iter().enumerate() {
        let next = match syl.tone {
            Tone::High => Tone::Falling,
            Tone::Falling => Tone::Low,
            Tone::Low => Tone::Rising,
            Tone::Rising => Tone::High,
            Tone::Mid => Tone::Mid,
            Tone::None => Tone::None,
        };
        if next != syl.tone {
            let out = out_opt.get_or_insert_with(|| word.clone());
            out.syllables[i].tone = next;
        }
    }
    out_opt
}

/// Palatalization of velar/alveolar onsets before front high/mid vowels (`i, e, ...`) or yod (`j`),
/// including yod-coalescence (`kj/tj -> tʃ`, `gj/dj -> dʒ`, `sj -> ʃ`, `zj -> ʒ`, `nj -> ɲ`).
pub fn palatalize(word: &Word) -> Word {
    palatalize_opt(word).unwrap_or_else(|| word.clone())
}

fn palatalize_consonant(c: usize) -> Option<usize> {
    match c {
        2 | 6 | 36 | 49 | 50 => Some(28), // t, k, q, tʰ, kʰ -> tʃ
        3 | 7 => Some(29),                // d, g -> dʒ
        8 => Some(10),                    // s -> ʃ
        9 => Some(11),                    // z -> ʒ
        16 | 37 => Some(35),              // x, χ -> ç
        19 | 21 => Some(20),              // n, ŋ -> ɲ
        _ => None,
    }
}

fn palatalize_opt(word: &Word) -> Option<Word> {
    let mut out_opt: Option<Word> = None;
    for (i, syl) in word.syllables.iter().enumerate() {
        if syl.onset.is_empty() {
            continue;
        }
        // Case 1: Yod-coalescence in onset cluster (`Cj` -> palatalized `C`, dropping `j` = idx 24)
        let len = syl.onset.len();
        if len >= 2 && syl.onset[len - 1] == 24 {
            if let Some(pal) = palatalize_consonant(syl.onset[len - 2]) {
                let out = out_opt.get_or_insert_with(|| word.clone());
                let olen = out.syllables[i].onset.len();
                out.syllables[i].onset[olen - 2] = pal;
                out.syllables[i].onset.pop();
                continue;
            }
        }
        // Case 2: Onset consonant immediately preceding a front high/mid vowel
        let Some(&v0) = syl.nucleus.first() else {
            continue;
        };
        let vd = &VOWELS[v0];
        if vd.backness <= 1 && vd.height <= 2 {
            if let Some(pal) = palatalize_consonant(syl.onset[len - 1]) {
                let out = out_opt.get_or_insert_with(|| word.clone());
                let olen = out.syllables[i].onset.len();
                out.syllables[i].onset[olen - 1] = pal;
            }
        }
    }
    out_opt
}

/// Regressive vowel nasalization with nasal coda loss (`VN -> Ṽ`), mapping oral vowels
/// to the nearest vowel in `NASAL_VOWEL_SERIES` (`ĩ ẽ ã õ ũ`).
pub fn nasalize(word: &Word) -> Word {
    nasalize_opt(word).unwrap_or_else(|| word.clone())
}

fn nasalize_opt(word: &Word) -> Option<Word> {
    let mut out_opt: Option<Word> = None;
    for (i, syl) in word.syllables.iter().enumerate() {
        let Some(&c0) = syl.coda.first() else {
            continue;
        };
        if CONSONANTS[c0].manner != 3 {
            continue;
        }
        let Some(&v_last) = syl.nucleus.last() else {
            continue;
        };
        let nv = if v_last >= 27 {
            v_last
        } else {
            nearest_vowel_to_pos(vowel_pos(v_last), &NASAL_VOWEL_SERIES)
        };
        let out = out_opt.get_or_insert_with(|| word.clone());
        let nlen = out.syllables[i].nucleus.len();
        out.syllables[i].nucleus[nlen - 1] = nv;
        out.syllables[i].coda.remove(0);
    }
    out_opt
}

/// Height metaphony (`metaphony.py:21-22`).
pub fn a_mutate(word: &Word) -> Word {
    a_mutate_opt(word).unwrap_or_else(|| word.clone())
}

fn a_mutate_opt(word: &Word) -> Option<Word> {
    if word.syllables.len() < 2 {
        return None;
    }
    let si = word.stressed_index();
    if si + 1 >= word.syllables.len() {
        return None;
    }
    let s_curr = &word.syllables[si];
    let s_next = &word.syllables[si + 1];
    if s_curr.nucleus.len() > 1 || s_curr.nucleus.is_empty() || s_next.nucleus.is_empty() {
        return None;
    }
    let v0 = s_curr.nucleus[0];
    let v1 = s_next.nucleus[0];
    let (h0, b0, r0) = vowel_pos(v0);
    let (h1, _, _) = vowel_pos(v1);
    let dh = (h1 - h0).signum();
    if dh == 0 {
        return None;
    }
    let pool: &[usize] = if v0 >= 27 {
        &NASAL_VOWEL_SERIES
    } else {
        &ALL_ORAL_VOWEL_INDICES
    };
    let nv = nearest_vowel_to_pos((h0 + dh, b0, r0), pool);
    if nv == v0 {
        return None;
    }
    let mut out = word.clone();
    out.syllables[si].nucleus[0] = nv;
    Some(out)
}

/// Backness/roundedness umlaut (`metaphony.py:25-26`).
pub fn i_mutate(word: &Word) -> Word {
    i_mutate_opt(word).unwrap_or_else(|| word.clone())
}

fn i_mutate_opt(word: &Word) -> Option<Word> {
    if word.syllables.len() < 2 {
        return None;
    }
    let si = word.stressed_index();
    if si + 1 >= word.syllables.len() {
        return None;
    }
    let s_curr = &word.syllables[si];
    let s_next = &word.syllables[si + 1];
    if s_curr.nucleus.len() > 1 || s_curr.nucleus.is_empty() || s_next.nucleus.is_empty() {
        return None;
    }
    let v0 = s_curr.nucleus[0];
    let v1 = s_next.nucleus[0];
    let (h0, b0, _) = vowel_pos(v0);
    let (_, b1, r1) = vowel_pos(v1);
    let db = (b1 - b0).signum();
    let pool: &[usize] = if v0 >= 27 {
        &NASAL_VOWEL_SERIES
    } else {
        &ALL_ORAL_VOWEL_INDICES
    };
    let nv = nearest_vowel_to_pos((h0, b0 + db, r1), pool);
    if nv == v0 {
        return None;
    }
    let mut out = word.clone();
    out.syllables[si].nucleus[0] = nv;
    Some(out)
}

/// Vowel breaking of stressed monophthongs (`breaking.py:5-19`).
pub fn break_vowels(word: &Word) -> Word {
    break_vowels_opt(word).unwrap_or_else(|| word.clone())
}

fn break_vowels_opt(word: &Word) -> Option<Word> {
    if word.syllables.is_empty() {
        return None;
    }
    let si = word.stressed_index();
    let s = &word.syllables[si];
    if s.nucleus.len() != 1 {
        return None;
    }
    let v = s.nucleus[0];
    let (h, b, r) = vowel_pos(v);
    let pair = if h == 0 {
        let onglide = nearest_vowel_to_pos((2, 2, r), &ALL_ORAL_VOWEL_INDICES);
        vec![onglide, v]
    } else {
        let target_b = if b < 2 { 0 } else if b > 2 { 4 } else { 2 };
        let onglide = nearest_vowel_to_pos((0, target_b, r), &ALL_ORAL_VOWEL_INDICES);
        vec![onglide, v]
    };
    let mut out = word.clone();
    out.syllables[si].nucleus = pair;
    Some(out)
}

/// Spirantization: intervocalic plosive -> fricative (`lenition.py:41-52`).
pub fn open_consonants(word: &Word) -> Word {
    open_consonants_opt(word).unwrap_or_else(|| word.clone())
}

fn open_consonants_opt(word: &Word) -> Option<Word> {
    lenite_intervocalic_opt(word, |c| {
        let cd = &CONSONANTS[c];
        if cd.manner == 0 {
            nearest_consonant_to_pos((2, cd.place, cd.voice), &ALL_CONSONANT_INDICES)
        } else if cd.manner == 6 {
            // Deaffrication: intervocalic affricate (`tʃ`/`ts`) -> sibilant (`ʃ`/`s`)
            nearest_consonant_to_pos((1, cd.place, cd.voice), &ALL_CONSONANT_INDICES)
        } else {
            c
        }
    })
}

/// Intervocalic voicing: unvoiced -> voiced (`lenition.py:55-64`).
pub fn sonorize_consonants(word: &Word) -> Word {
    sonorize_consonants_opt(word).unwrap_or_else(|| word.clone())
}

fn sonorize_consonants_opt(word: &Word) -> Option<Word> {
    lenite_intervocalic_opt(word, |c| {
        let cd = &CONSONANTS[c];
        if cd.voice == 0 && matches!(cd.manner, 0 | 1 | 2 | 6) && cd.place <= 5 {
            nearest_consonant_to_pos((cd.manner, cd.place, 1), &ALL_CONSONANT_INDICES)
        } else {
            c
        }
    })
}

fn lenite_intervocalic_opt<F: Fn(usize) -> usize>(word: &Word, f: F) -> Option<Word> {
    let mut out_opt: Option<Word> = None;
    let n = word.syllables.len();
    for i in 0..n {
        let prev_coda_empty = i > 0
            && out_opt
                .as_ref()
                .map_or(&word.syllables[i - 1], |w| &w.syllables[i - 1])
                .coda
                .is_empty();
        let curr_onset_len = out_opt
            .as_ref()
            .map_or(&word.syllables[i], |w| &w.syllables[i])
            .onset
            .len();
        if prev_coda_empty && curr_onset_len == 1 {
            let c = out_opt
                .as_ref()
                .map_or(word.syllables[i].onset[0], |w| w.syllables[i].onset[0]);
            let nc = f(c);
            if nc != c {
                let out = out_opt.get_or_insert_with(|| word.clone());
                out.syllables[i].onset[0] = nc;
            }
        }
        let next_onset_empty = i + 1 < n
            && out_opt
                .as_ref()
                .map_or(&word.syllables[i + 1], |w| &w.syllables[i + 1])
                .onset
                .is_empty();
        let curr_coda_len = out_opt
            .as_ref()
            .map_or(&word.syllables[i], |w| &w.syllables[i])
            .coda
            .len();
        if next_onset_empty && curr_coda_len == 1 {
            let c = out_opt
                .as_ref()
                .map_or(word.syllables[i].coda[0], |w| w.syllables[i].coda[0]);
            let nc = f(c);
            if nc != c {
                let out = out_opt.get_or_insert_with(|| word.clone());
                out.syllables[i].coda[0] = nc;
            }
        }
    }
    out_opt
}

/// Vocalization of coda consonant before another consonant into semivowel `j`/`w` (`lenition.py:67-76`).
pub fn vocalize_consonants(word: &Word) -> Word {
    vocalize_consonants_opt(word).unwrap_or_else(|| word.clone())
}

fn vocalize_consonants_opt(word: &Word) -> Option<Word> {
    let mut out_opt: Option<Word> = None;
    let n = word.syllables.len();
    for i in 0..n {
        let has_following_c = word.syllables[i].coda.len() > 1
            || (i + 1 < n && !word.syllables[i + 1].onset.is_empty());
        if has_following_c && !word.syllables[i].coda.is_empty() {
            let c = word.syllables[i].coda[0];
            let cd = &CONSONANTS[c];
            let sv = nearest_consonant_to_pos((5, cd.place, 1), &ALL_CONSONANT_INDICES);
            if sv != c {
                let out = out_opt.get_or_insert_with(|| word.clone());
                out.syllables[i].coda[0] = sv;
            }
        }
    }
    out_opt
}

/// Drop initial unstressed vowel (`elision.py:16-24`).
pub fn apheresize(word: &Word) -> Word {
    apheresize_opt(word).unwrap_or_else(|| word.clone())
}

fn apheresize_opt(word: &Word) -> Option<Word> {
    if word.syllables.len() < 2 || word.stressed_index() == 0 {
        return None;
    }
    if !word.syllables[0].onset.is_empty() {
        return None;
    }
    let mut out = word.clone();
    let s0 = out.syllables.remove(0);
    let mut prefix = s0.coda;
    prefix.extend(out.syllables[0].onset.iter().copied());
    out.syllables[0].onset = prefix;
    Some(out)
}

/// Drop medial unstressed vowel (`elision.py:27-37`).
pub fn syncopate(word: &Word) -> Word {
    syncopate_opt(word).unwrap_or_else(|| word.clone())
}

fn syncopate_opt(word: &Word) -> Option<Word> {
    if word.syllables.len() < 3 {
        return None;
    }
    let si = word.stressed_index();
    let target = (1..word.syllables.len() - 1).find(|&idx| idx != si);
    let idx = target?;
    let mut out = word.clone();
    let removed = out.syllables.remove(idx);
    out.syllables[idx - 1].coda.extend(removed.onset);
    let mut next_onset = removed.coda;
    next_onset.extend(out.syllables[idx].onset.iter().copied());
    out.syllables[idx].onset = next_onset;
    Some(out)
}

/// Drop final unstressed vowel (`elision.py:40-48`).
pub fn apocopate(word: &Word) -> Word {
    apocopate_opt(word).unwrap_or_else(|| word.clone())
}

fn apocopate_opt(word: &Word) -> Option<Word> {
    if word.syllables.len() < 2 {
        return None;
    }
    let last_idx = word.syllables.len() - 1;
    if word.stressed_index() == last_idx || !word.syllables[last_idx].coda.is_empty() {
        return None;
    }
    let mut out = word.clone();
    let last = out.syllables.pop().unwrap();
    let prev_last = out.syllables.len() - 1;
    out.syllables[prev_last].coda.extend(last.onset);
    Some(out)
}

/// Etymology chain for a word (`language/dictionary.py:Origin`).
#[derive(Clone, Debug)]
pub struct WordOrigin {
    pub word: Word,
    pub lang_name: String,
    pub parent: Option<Arc<WordOrigin>>,
}

impl WordOrigin {
    pub fn new(word: Word, lang_name: String, parent: Option<WordOrigin>) -> Self {
        Self {
            word,
            lang_name,
            parent: parent.map(Arc::new),
        }
    }

    pub fn with_arc_parent(
        word: Word,
        lang_name: String,
        parent: Option<Arc<WordOrigin>>,
    ) -> Self {
        Self {
            word,
            lang_name,
            parent,
        }
    }

    pub fn mutate(&self, change: SoundChangeKind) -> Self {
        if let Some(mutated) = change.apply_opt(&self.word) {
            Self::new(mutated, self.lang_name.clone(), Some(self.clone()))
        } else {
            self.clone()
        }
    }

    /// Format historical etymology chain (`language/output.py:etymology`).
    pub fn format_etymology(&self) -> String {
        let mut steps = Vec::new();
        let mut curr = self.parent.as_deref();
        let mut prev_lang = self.lang_name.as_str();
        while let Some(node) = curr {
            let w_str = format!("{} [{}]", node.word.write(false), node.word.pronounce());
            if node.lang_name != prev_lang && !node.lang_name.is_empty() {
                steps.push(format!("{} {}", node.lang_name, w_str));
            } else {
                steps.push(w_str);
            }
            prev_lang = node.lang_name.as_str();
            curr = node.parent.as_deref();
        }
        if steps.is_empty() {
            "Root coinage".to_string()
        } else {
            format!("< {}", steps.join(" < "))
        }
    }
}

/// Distinctive marked phonological series that can diffuse across language-family boundaries
/// in a sprachbund (e.g. clicks from Khoisan to Bantu, retroflexes from Dravidian to Indo-Aryan,
/// ejectives in the Caucasus, aspirates in South Asia, nasal vowels in West Africa).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArealFeature {
    Retroflex,
    Aspirated,
    Ejective,
    Implosive,
    Clicks,
    NasalVowels,
}

impl ArealFeature {
    pub const ALL: [Self; 6] = [
        Self::Retroflex,
        Self::Aspirated,
        Self::Ejective,
        Self::Implosive,
        Self::Clicks,
        Self::NasalVowels,
    ];

    /// Whether `lang` attests at least one phoneme from this marked series.
    pub fn present_in(self, lang: &Language) -> bool {
        match self {
            Self::Retroflex => lang.consonants.iter().any(|c| RETROFLEX_SERIES.contains(c)),
            Self::Aspirated => lang.consonants.iter().any(|c| ASPIRATED_SERIES.contains(c)),
            Self::Ejective => lang.consonants.iter().any(|c| EJECTIVE_SERIES.contains(c)),
            Self::Implosive => lang.consonants.iter().any(|c| IMPLOSIVE_SERIES.contains(c)),
            Self::Clicks => lang.consonants.iter().any(|c| CLICK_SERIES.contains(c)),
            Self::NasalVowels => lang.vowels.iter().any(|v| NASAL_VOWEL_SERIES.contains(v)),
        }
    }

    /// Marked features attested in `donor` that `recipient` currently lacks.
    pub fn candidates_from(donor: &Language, recipient: &Language) -> Vec<Self> {
        Self::ALL
            .iter()
            .copied()
            .filter(|f| f.present_in(donor) && !f.present_in(recipient))
            .collect()
    }

    /// Apply a localized contact-induced substitution of this areal feature to at most one
    /// syllable in `word`, using phonemes attested in `donor`.
    pub fn apply_to_word(self, word: &Word, donor: &Language, rng: &mut Pcg64Mcg) -> Option<Word> {
        let mut out = word.clone();
        let mut changed = false;
        match self {
            Self::Retroflex => {
                for syl in &mut out.syllables {
                    for c in syl.onset.iter_mut().chain(syl.coda.iter_mut()) {
                        let mapped = match *c {
                            2 if donor.consonants.contains(&43) => Some(43), // t → ʈ
                            3 if donor.consonants.contains(&44) => Some(44), // d → ɖ
                            8 if donor.consonants.contains(&45) => Some(45), // s → ʂ
                            19 if donor.consonants.contains(&46) => Some(46), // n → ɳ
                            22 if donor.consonants.contains(&47) => Some(47), // l → ɭ
                            _ => None,
                        };
                        if let Some(nc) = mapped {
                            *c = nc;
                            changed = true;
                            break;
                        }
                    }
                    if changed {
                        break;
                    }
                }
            }
            Self::Aspirated => {
                for syl in &mut out.syllables {
                    if let Some(first) = syl.onset.first_mut() {
                        let mapped = match *first {
                            0 if donor.consonants.contains(&48) => Some(48), // p → pʰ
                            2 if donor.consonants.contains(&49) => Some(49), // t → tʰ
                            6 if donor.consonants.contains(&50) => Some(50), // k → kʰ
                            _ => None,
                        };
                        if let Some(nc) = mapped {
                            *first = nc;
                            changed = true;
                            break;
                        }
                    }
                }
            }
            Self::Ejective => {
                for syl in &mut out.syllables {
                    if let Some(first) = syl.onset.first_mut() {
                        let mapped = match *first {
                            0 if donor.consonants.contains(&51) => Some(51),  // p → pʼ
                            2 if donor.consonants.contains(&52) => Some(52),  // t → tʼ
                            6 if donor.consonants.contains(&53) => Some(53),  // k → kʼ
                            30 if donor.consonants.contains(&54) => Some(54), // ts → tsʼ
                            28 if donor.consonants.contains(&55) => Some(55), // tʃ → tʃʼ
                            _ => None,
                        };
                        if let Some(nc) = mapped {
                            *first = nc;
                            changed = true;
                            break;
                        }
                    }
                }
            }
            Self::Implosive => {
                for syl in &mut out.syllables {
                    if let Some(first) = syl.onset.first_mut() {
                        let mapped = match *first {
                            1 if donor.consonants.contains(&56) => Some(56), // b → ɓ
                            3 if donor.consonants.contains(&57) => Some(57), // d → ɗ
                            7 if donor.consonants.contains(&58) => Some(58), // g → ɠ
                            _ => None,
                        };
                        if let Some(nc) = mapped {
                            *first = nc;
                            changed = true;
                            break;
                        }
                    }
                }
            }
            Self::Clicks => {
                let donor_clicks: Vec<usize> = CLICK_SERIES
                    .iter()
                    .copied()
                    .filter(|c| donor.consonants.contains(c))
                    .collect();
                if donor_clicks.is_empty() {
                    return None;
                }
                for syl in &mut out.syllables {
                    if syl.onset.len() == 1 && matches!(syl.onset[0], 2 | 4 | 6) {
                        syl.onset[0] =
                            donor_clicks[(rng.next_u64() as usize) % donor_clicks.len()];
                        changed = true;
                        break;
                    }
                }
            }
            Self::NasalVowels => {
                let donor_nasals: Vec<usize> = NASAL_VOWEL_SERIES
                    .iter()
                    .copied()
                    .filter(|v| donor.vowels.contains(v))
                    .collect();
                if donor_nasals.is_empty() {
                    return None;
                }
                for syl in &mut out.syllables {
                    if let Some(last_v) = syl.nucleus.last_mut() {
                        if *last_v < 27 {
                            *last_v = nearest_vowel_to_pos(vowel_pos(*last_v), &donor_nasals);
                            changed = true;
                            break;
                        }
                    }
                }
            }
        }
        changed.then_some(out)
    }
}

/// Format phoneme inventory string `[vowels] / [consonants]` (`languagepresenter.py:13-15`).
pub fn format_phoneme_inventory(vowels: &[usize], consonants: &[usize]) -> String {
    let mut v_syms: Vec<&str> = vowels.iter().map(|&v| VOWELS[v].symbol).collect();
    v_syms.sort_unstable();
    v_syms.dedup();
    let mut c_syms: Vec<&str> = consonants.iter().map(|&c| CONSONANTS[c].symbol).collect();
    c_syms.sort_unstable();
    c_syms.dedup();
    format!("Vowels ({}): {}  |  Consonants ({}): {}", v_syms.len(), v_syms.join(" "), c_syms.len(), c_syms.join(" "))
}

pub fn extract_inventory_from_words(words: &[Word]) -> (Vec<usize>, Vec<usize>) {
    let mut vs = BTreeSet::new();
    let mut cs = BTreeSet::new();
    for w in words {
        for s in &w.syllables {
            for &c in &s.onset {
                cs.insert(c);
            }
            for &v in &s.nucleus {
                vs.insert(v);
            }
            for &c in &s.coda {
                cs.insert(c);
            }
        }
    }
    (vs.into_iter().collect(), cs.into_iter().collect())
}

/// Standalone Language sandbox (`languagesimulation.py`, `languagepresenter.py`, `languageui.py`).
pub struct LanguageSandbox {
    dest_lang: Language,
    source_origins: Vec<WordOrigin>,
    dest_origins: Vec<WordOrigin>,
    lang_counter: u32,
    rng: Pcg64Mcg,
}

impl LanguageSandbox {
    pub const WORD_COUNT: usize = 20;

    /// Core lexical concepts in the Language sandbox, beginning with celestial concepts
    /// for the Sun, Moons, and Planets alongside biosphere, nation, state, and language concepts.
    pub const SANDBOX_CONCEPTS: [&'static str; Self::WORD_COUNT] = [
        "sun (host star)",
        "moon (primary moon)",
        "outer moon (resonant M2)",
        "world (home planet)",
        "inner planet (swift star)",
        "morning / evening planet",
        "red outer planet",
        "bright giant planet",
        "ringed outer planet",
        "sky / heavens",
        "water / ocean",
        "earth / stone",
        "river / stream",
        "grain / crop",
        "tree / forest",
        "herd / livestock",
        "hound / predator",
        "people / nation",
        "city / state",
        "tongue / language",
    ];

    pub fn new(seed: u128) -> Self {
        let mut rng = Pcg64Mcg::new(seed);
        let dest_lang = Language::generate(&mut rng);
        let mut s = Self {
            dest_lang,
            source_origins: Vec::new(),
            dest_origins: Vec::new(),
            lang_counter: 0,
            rng,
        };
        s.generate();
        s
    }

    pub fn generate(&mut self) {
        self.dest_lang = Language::generate(&mut self.rng);
        self.lang_counter += 1;
        let lang_name = format!("Lang_{}", self.lang_counter);
        let mut seen_keys = BTreeSet::new();
        let mut used_words = BTreeSet::new();
        let mut origins = Vec::with_capacity(Self::WORD_COUNT);
        while origins.len() < Self::WORD_COUNT {
            let w = self
                .dest_lang
                .generate_word_unique(&mut seen_keys, &used_words, &mut self.rng);
            if used_words.insert(w.clone()) {
                origins.push(WordOrigin::new(w, lang_name.clone(), None));
            }
        }
        let words: Vec<Word> = origins.iter().map(|o| o.word.clone()).collect();
        self.dest_lang.refresh_from_lexicon(&words);
        self.source_origins = origins.clone();
        self.dest_origins = origins;
    }

    pub fn borrow(&mut self) {
        let donor_lang = Language::generate(&mut self.rng);
        self.lang_counter += 1;
        let donor_name = format!("Donor_{}", self.lang_counter);
        let recip_name = format!("Lang_{}", self.lang_counter);

        let dest_words: Vec<Word> = self.dest_origins.iter().map(|o| o.word.clone()).collect();
        self.dest_lang.refresh_from_lexicon(&dest_words);

        let mut donor_keys = BTreeSet::new();
        let mut used_donor = BTreeSet::new();
        let mut existing_recip = BTreeSet::new();
        let mut src = Vec::with_capacity(Self::WORD_COUNT);
        let mut dst = Vec::with_capacity(Self::WORD_COUNT);

        while src.len() < Self::WORD_COUNT {
            let w = donor_lang.generate_word_unique(&mut donor_keys, &used_donor, &mut self.rng);
            if used_donor.insert(w.clone()) {
                let donor_origin = WordOrigin::new(w.clone(), donor_name.clone(), None);
                let borrowed =
                    self.dest_lang
                        .borrow_word_unique(&w, &existing_recip, &mut self.rng);
                existing_recip.insert(borrowed.clone());
                let recip_origin =
                    WordOrigin::new(borrowed, recip_name.clone(), Some(donor_origin.clone()));
                src.push(donor_origin);
                dst.push(recip_origin);
            }
        }
        let new_words: Vec<Word> = dst.iter().map(|o| o.word.clone()).collect();
        self.dest_lang.refresh_from_lexicon(&new_words);
        self.source_origins = src;
        self.dest_origins = dst;
    }

    pub fn apply_sound_change(&mut self, kind: SoundChangeKind) {
        let mut seen: BTreeSet<Word> = self.dest_origins.iter().map(|o| o.word.clone()).collect();
        let mut updated = Vec::with_capacity(self.dest_origins.len());
        for o in &self.dest_origins {
            let candidate = kind.apply(&o.word);
            if candidate != o.word && !seen.contains(&candidate) {
                seen.remove(&o.word);
                seen.insert(candidate);
                updated.push(o.mutate(kind));
            } else {
                updated.push(o.clone());
            }
        }
        self.dest_origins = updated;
        let words: Vec<Word> = self.dest_origins.iter().map(|o| o.word.clone()).collect();
        self.dest_lang.refresh_from_lexicon(&words);
    }

    pub fn source_phonemes_text(&self) -> String {
        let words: Vec<Word> = self.source_origins.iter().map(|o| o.word.clone()).collect();
        let (vs, cs) = extract_inventory_from_words(&words);
        format_phoneme_inventory(&vs, &cs)
    }

    pub fn dest_phonemes_text(&self) -> String {
        let words: Vec<Word> = self.dest_origins.iter().map(|o| o.word.clone()).collect();
        let (vs, cs) = extract_inventory_from_words(&words);
        format_phoneme_inventory(&vs, &cs)
    }

    pub fn word_count(&self) -> usize {
        self.dest_origins.len()
    }

    pub fn concept_label(&self, idx: usize) -> String {
        Self::SANDBOX_CONCEPTS
            .get(idx)
            .copied()
            .unwrap_or("concept")
            .to_string()
    }

    pub fn source_orth(&self, idx: usize) -> String {
        self.source_origins
            .get(idx)
            .map(|o| o.word.write(false))
            .unwrap_or_default()
    }

    pub fn source_ipa(&self, idx: usize) -> String {
        self.source_origins
            .get(idx)
            .map(|o| o.word.pronounce())
            .unwrap_or_default()
    }

    pub fn dest_orth(&self, idx: usize) -> String {
        self.dest_origins
            .get(idx)
            .map(|o| o.word.write(false))
            .unwrap_or_default()
    }

    pub fn dest_ipa(&self, idx: usize) -> String {
        self.dest_origins
            .get(idx)
            .map(|o| o.word.pronounce())
            .unwrap_or_default()
    }

    pub fn dest_etymology(&self, idx: usize) -> String {
        self.dest_origins
            .get(idx)
            .map(|o| o.format_etymology())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_sandbox_and_sound_changes() {
        let mut sandbox = LanguageSandbox::new(42);
        assert_eq!(sandbox.word_count(), 20);
        assert!(!sandbox.source_phonemes_text().is_empty());

        for kind in SoundChangeKind::ALL {
            sandbox.apply_sound_change(kind);
        }
        assert_eq!(sandbox.word_count(), 20);
        for i in 0..20 {
            assert!(!sandbox.dest_orth(i).is_empty());
            assert!(!sandbox.dest_ipa(i).is_empty());
        }

        sandbox.borrow();
        assert_eq!(sandbox.word_count(), 20);
        assert!(sandbox.dest_etymology(0).starts_with("< "));
    }

    #[test]
    fn test_expanded_phoneme_inventory_series_and_orthography() {
        let mut rng = Pcg64Mcg::new(12345);
        let mut saw_glottal = 0usize;
        let mut saw_affricate = 0usize;
        let mut saw_uvular = 0usize;
        let mut saw_retroflex = 0usize;
        let mut saw_aspirate = 0usize;
        let mut saw_ejective = 0usize;
        let mut saw_implosive = 0usize;
        let mut saw_click = 0usize;
        let mut saw_nasal_vowel = 0usize;

        for _ in 0..200 {
            let lang = Language::generate(&mut rng);
            if lang.consonants.iter().any(|&c| c == 26 || c == 27) {
                saw_glottal += 1;
            }
            if lang.consonants.iter().any(|&c| (28..32).contains(&c)) {
                saw_affricate += 1;
            }
            if lang.consonants.iter().any(|&c| (36..39).contains(&c)) {
                saw_uvular += 1;
            }
            if lang.consonants.iter().any(|&c| RETROFLEX_SERIES.contains(&c)) {
                saw_retroflex += 1;
            }
            if lang.consonants.iter().any(|&c| ASPIRATED_SERIES.contains(&c)) {
                saw_aspirate += 1;
            }
            if lang.consonants.iter().any(|&c| EJECTIVE_SERIES.contains(&c)) {
                saw_ejective += 1;
            }
            if lang.consonants.iter().any(|&c| IMPLOSIVE_SERIES.contains(&c)) {
                saw_implosive += 1;
            }
            if lang.consonants.iter().any(|&c| CLICK_SERIES.contains(&c)) {
                saw_click += 1;
                // Clicks must never appear in syllable codas
                for _ in 0..20 {
                    let syl = lang.generate_syllable(&mut rng);
                    assert!(
                        syl.coda.iter().all(|&c| CONSONANTS[c].manner != 10),
                        "Click phoneme must not appear in syllable coda"
                    );
                }
            }
            if lang.vowels.iter().any(|&v| NASAL_VOWEL_SERIES.contains(&v)) {
                saw_nasal_vowel += 1;
            }
        }

        assert!(saw_glottal > 50, "Expected glottals (h/ʔ) in many languages, got {saw_glottal}");
        assert!(saw_affricate > 40, "Expected affricates in many languages, got {saw_affricate}");
        assert!(saw_uvular > 10, "Expected uvulars in some languages, got {saw_uvular}");
        assert!(saw_retroflex > 5, "Expected retroflex series, got {saw_retroflex}");
        assert!(saw_aspirate > 15, "Expected aspirated series, got {saw_aspirate}");
        assert!(saw_ejective > 10, "Expected ejective series, got {saw_ejective}");
        assert!(saw_implosive > 5, "Expected implosive series, got {saw_implosive}");
        assert!(saw_click > 0, "Expected click series in a small fraction of languages");
        assert!(saw_nasal_vowel > 15, "Expected nasalized vowel series, got {saw_nasal_vowel}");

        // Verify capitalization skips leading click/glottal marks cleanly (`ǃka` -> `ǃKa`, `'a` -> `'A`)
        let click_word = Word {
            syllables: vec![Syllable {
                onset: vec![60, 6], // ǃ + k
                nucleus: vec![26],  // a
                coda: vec![],
                tone: Tone::None,
            }],
            stress: 0,
        };
        assert_eq!(click_word.write(true), "ǃKa");
        assert_eq!(click_word.pronounce(), "ǃka");
    }

    #[test]
    fn test_tonogenesis_and_tone_shift_emerge_from_nontonal_proto_languages() {
        let mut rng = Pcg64Mcg::new(99);
        for _ in 0..30 {
            let lang = Language::generate(&mut rng);
            assert!(lang.tones.is_empty(), "Proto-languages must start non-tonal");
        }

        // Onset tonogenesis: voiced `b` (1) + `a` (26) -> devoiced `p` (0) + Low tone (`pà`, `pa˩`);
        // voiceless `t` (2) + `a` (26) -> `t` + High tone (`tá`, `ta˥`).
        let w_onset = Word {
            syllables: vec![
                Syllable { onset: vec![1], nucleus: vec![26], coda: vec![], tone: Tone::None },
                Syllable { onset: vec![2], nucleus: vec![26], coda: vec![], tone: Tone::None },
            ],
            stress: 1,
        };
        let t_onset = tonogenesis_onset(&w_onset);
        assert_eq!(t_onset.syllables[0].onset[0], 0); // b -> p
        assert_eq!(t_onset.syllables[0].tone, Tone::Low);
        assert_eq!(t_onset.syllables[1].tone, Tone::High);
        assert_eq!(t_onset.write(true), "Pàtá");
        assert_eq!(t_onset.pronounce(), "pa˩.ta˥");

        // Coda tonogenesis: `kaʔ` -> `kǎ` (`ka˩˥`), `tah` -> `tâ` (`ta˥˩`).
        let w_coda = Word {
            syllables: vec![
                Syllable { onset: vec![6], nucleus: vec![26], coda: vec![27], tone: Tone::None },
                Syllable { onset: vec![2], nucleus: vec![26], coda: vec![26], tone: Tone::None },
            ],
            stress: 1,
        };
        let t_coda = tonogenesis_coda(&w_coda);
        assert!(t_coda.syllables[0].coda.is_empty());
        assert!(t_coda.syllables[1].coda.is_empty());
        assert_eq!(t_coda.syllables[0].tone, Tone::Rising);
        assert_eq!(t_coda.syllables[1].tone, Tone::Falling);
        assert_eq!(t_coda.write(true), "Kǎtâ");
        assert_eq!(t_coda.pronounce(), "ka˩˥.ta˥˩");

        // Tone shift leaves non-tonal words untouched, and cycles tonal words.
        assert!(tone_shift_opt(&w_onset).is_none());
        let shifted = tone_shift(&t_coda);
        assert_eq!(shifted.syllables[0].tone, Tone::High);
        assert_eq!(shifted.syllables[1].tone, Tone::Low);
    }

    #[test]
    fn test_palatalization_and_nasalization_sound_changes() {
        // Palatalization: `ki` -> `tʃi` (`chi`), `ni` -> `ɲi` (`nhi`), `ka` unchanged
        let w_pal = Word {
            syllables: vec![
                Syllable { onset: vec![6], nucleus: vec![0], coda: vec![], tone: Tone::None },  // ki
                Syllable { onset: vec![19], nucleus: vec![0], coda: vec![], tone: Tone::None }, // ni
                Syllable { onset: vec![6], nucleus: vec![26], coda: vec![], tone: Tone::None }, // ka
            ],
            stress: 1,
        };
        let pal = palatalize(&w_pal);
        assert_eq!(pal.syllables[0].onset, vec![28]); // k -> tʃ
        assert_eq!(pal.syllables[1].onset, vec![20]); // n -> ɲ
        assert_eq!(pal.syllables[2].onset, vec![6]);  // k before a unchanged

        // Yod-coalescence: `kja` -> `tʃa`
        let w_yod = Word {
            syllables: vec![Syllable {
                onset: vec![6, 24],
                nucleus: vec![26],
                coda: vec![],
                tone: Tone::None,
            }],
            stress: 1,
        };
        let yod = palatalize(&w_yod);
        assert_eq!(yod.syllables[0].onset, vec![28]);

        // Nasalization: `kan` -> `kã`, `tin` -> `tĩ`, `kat` unchanged
        let w_nas = Word {
            syllables: vec![
                Syllable { onset: vec![6], nucleus: vec![26], coda: vec![19], tone: Tone::None }, // kan
                Syllable { onset: vec![2], nucleus: vec![0], coda: vec![19], tone: Tone::None },  // tin
                Syllable { onset: vec![6], nucleus: vec![26], coda: vec![2], tone: Tone::None },  // kat
            ],
            stress: 1,
        };
        let nas = nasalize(&w_nas);
        assert_eq!(nas.syllables[0].nucleus, vec![29]); // ã
        assert!(nas.syllables[0].coda.is_empty());
        assert_eq!(nas.syllables[1].nucleus, vec![27]); // ĩ
        assert!(nas.syllables[1].coda.is_empty());
        assert_eq!(nas.syllables[2].nucleus, vec![26]); // a unchanged
        assert_eq!(nas.syllables[2].coda, vec![2]);     // t unchanged
    }
}
