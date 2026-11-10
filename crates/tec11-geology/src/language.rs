use std::collections::BTreeSet;

use tec11_math_grid::Pcg64Mcg;

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

pub const VOWELS: [VowelDef; 27] = [
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
];

pub const CONSONANTS: [ConsonantDef; 26] = [
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

/// Generate a language's vowel and consonant inventory (`language/phonemes.py:39-47`).
pub fn generate_phonemes(rng: &mut Pcg64Mcg) -> (Vec<usize>, Vec<usize>) {
    let nv = (rng.gauss(8.7, 3.0).round() as usize).clamp(3, 18);
    let nc = (rng.gauss(22.5, 9.0).round() as usize).clamp(6, 24);

    let mut vs = BTreeSet::new();
    while vs.len() < nv {
        let idx = (rng.next_u64() as usize) % VOWELS.len();
        if rng.uniform(0.0, 1.0) < VOWELS[idx].weight {
            vs.insert(idx);
        }
    }

    let mut cs = BTreeSet::new();
    while cs.len() < nc {
        let idx = (rng.next_u64() as usize) % CONSONANTS.len();
        if rng.uniform(0.0, 1.0) < CONSONANTS[idx].weight {
            cs.insert(idx);
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
                let all_c: Vec<usize> = (0..CONSONANTS.len()).collect();
                for i in 0..out.len().saturating_sub(1) {
                    if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                        if CONSONANTS[c0].manner == 3 && CONSONANTS[c1].voice == 0 {
                            let cd = &CONSONANTS[c1];
                            out[i + 1] = Some(nearest_consonant_to_pos((cd.manner, cd.place, 1), &all_c));
                        }
                    }
                }
            }
            Self::MatchPlosiveVoice => {
                let all_c: Vec<usize> = (0..CONSONANTS.len()).collect();
                for i in 0..out.len().saturating_sub(1) {
                    if let (Some(c0), Some(c1)) = (out[i], out[i + 1]) {
                        if CONSONANTS[c0].manner == 0 && CONSONANTS[c1].manner == 0 && CONSONANTS[c0].voice != CONSONANTS[c1].voice {
                            let cd = &CONSONANTS[c1];
                            out[i + 1] = Some(nearest_consonant_to_pos((cd.manner, cd.place, CONSONANTS[c0].voice), &all_c));
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Syllable {
    pub onset: Vec<usize>,
    pub nucleus: Vec<usize>,
    pub coda: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word {
    pub syllables: Vec<Syllable>,
    /// `1` = ultimate (`syllables.len() - 1`), `2` = penultimate (`syllables.len().saturating_sub(2)`).
    pub stress: usize,
}

impl Word {
    pub fn stressed_index(&self) -> usize {
        if self.syllables.len() <= 1 || self.stress <= 1 {
            self.syllables.len().saturating_sub(1)
        } else {
            self.syllables.len().saturating_sub(self.stress)
        }
    }

    /// Format in IPA (`language/ipa.py` and `language/output.py:pronounce`).
    pub fn pronounce(&self) -> String {
        let stressed_idx = self.stressed_index();
        let multi = self.syllables.len() > 1;
        let mut out = String::new();

        for (idx, syl) in self.syllables.iter().enumerate() {
            if multi && idx == stressed_idx {
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
        }
        out
    }

    /// Format in Romanized orthography (`language/orthography.py` and `language/output.py:write`).
    pub fn write(&self, capitalize: bool) -> String {
        let stressed_idx = self.stressed_index();
        let mark_stress = self.stress != 1 && self.syllables.len() > 1;
        let mut raw = String::new();

        for (idx, syl) in self.syllables.iter().enumerate() {
            for &c in &syl.onset {
                raw.push_str(CONSONANTS[c].orth);
            }
            for (v_pos, &v) in syl.nucleus.iter().enumerate() {
                if mark_stress && idx == stressed_idx && v_pos == 0 {
                    raw.push_str(VOWELS[v].orth_acute);
                } else {
                    raw.push_str(VOWELS[v].orth);
                }
            }
            for &c in &syl.coda {
                raw.push_str(CONSONANTS[c].orth);
            }
        }

        if capitalize {
            let mut chars = raw.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => raw,
            }
        } else {
            raw
        }
    }
}

#[derive(Clone, Debug)]
pub struct Language {
    pub vowels: Vec<usize>,
    pub consonants: Vec<usize>,
    pub onsetp: Vec<f64>,
    pub codap: Vec<f64>,
    pub constraints: Vec<PhonotacticConstraint>,
    pub stress: usize,
}

impl Language {
    pub fn generate(rng: &mut Pcg64Mcg) -> Self {
        let (vowels, consonants) = generate_phonemes(rng);
        let max_onset = (rng.next_u64() % 3) as usize; // 0..=2
        let mut onsetp = Vec::with_capacity(max_onset);
        for _ in 0..max_onset {
            onsetp.push(rng.uniform(0.0, 1.0));
        }
        let max_coda = (rng.next_u64() % 3) as usize; // 0..=2
        let mut codap = Vec::with_capacity(max_coda);
        for _ in 0..max_coda {
            codap.push(rng.uniform(0.0, 1.0));
        }

        let num_constraints = 3 + ((rng.next_u64() as usize) % (PhonotacticConstraint::ALL.len() - 2));
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

        let stress = if (rng.next_u64() & 1) == 0 { 1 } else { 2 };

        Self {
            vowels,
            consonants,
            onsetp,
            codap,
            constraints,
            stress,
        }
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
            if rng.uniform(0.0, 1.0) < p {
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

        let v = self.vowels[(rng.next_u64() as usize) % self.vowels.len()];
        let nucleus = vec![v];

        let mut coda = Vec::new();
        for &p in &self.codap {
            if rng.uniform(0.0, 1.0) < p {
                let c = self.consonants[(rng.next_u64() as usize) % self.consonants.len()];
                coda.push(c);
            }
        }
        for c_rule in &self.constraints {
            coda = c_rule
                .apply(&coda, false, None)
                .into_iter()
                .flatten()
                .collect();
        }

        Syllable {
            onset,
            nucleus,
            coda,
        }
    }

    pub fn generate_word(&self, rng: &mut Pcg64Mcg) -> Word {
        let mut syllables = vec![self.generate_syllable(rng)];
        while syllables.len() < 4 && rng.uniform(0.0, 1.0) < 0.55 {
            syllables.push(self.generate_syllable(rng));
        }
        Word {
            syllables,
            stress: self.stress,
        }
    }

    /// Adapt foreign phonemes in `word` to the nearest phonemes in `self` (`lexicon.py:18-24`).
    pub fn adapt_sounds(&self, word: &Word) -> Word {
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
            })
            .collect();
        Word {
            syllables,
            stress: self.stress,
        }
    }

    /// Enforce this language's syllable structure and phonotactic constraints via epenthesis (`lexicon.py:27-76`).
    pub fn constrain(&self, word: &Word) -> Word {
        let ep = self.epenthetic_vowel();
        let max_o = self.onsetp.len();
        let max_c = self.codap.len();
        let mut new_syllables = Vec::new();

        for s in &word.syllables {
            let mut onset = s.onset.clone();
            while onset.len() > max_o {
                let extra = onset.drain(0..1).collect();
                new_syllables.push(Syllable {
                    onset: extra,
                    nucleus: vec![ep],
                    coda: Vec::new(),
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
            });

            for part in coda_parts {
                new_syllables.push(Syllable {
                    onset: Vec::new(),
                    nucleus: vec![ep],
                    coda: part,
                });
            }
            new_syllables.extend(trailing_syllables);
        }

        if new_syllables.is_empty() {
            new_syllables.push(Syllable {
                onset: Vec::new(),
                nucleus: vec![ep],
                coda: Vec::new(),
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

    /// Derive a new word from `base` by prepending or appending a 1-syllable affix (`lexicon.py:85-92`).
    pub fn derive_word(&self, base: &Word, rng: &mut Pcg64Mcg) -> Word {
        let affix = self.generate_syllable(rng);
        let mut syllables = base.syllables.clone();
        if (rng.next_u64() & 1) == 0 {
            syllables.insert(0, affix);
        } else {
            syllables.push(affix);
        }
        let w = Word {
            syllables,
            stress: self.stress,
        };
        self.constrain(&w)
    }
}

/// All 9 historical sound changes (`metaphony.py`, `breaking.py`, `lenition.py`, `elision.py`).
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
}

impl SoundChangeKind {
    pub const ALL: [Self; 9] = [
        Self::AMutate,
        Self::IMutate,
        Self::BreakVowels,
        Self::OpenConsonants,
        Self::SonorizeConsonants,
        Self::VocalizeConsonants,
        Self::Apheresize,
        Self::Syncopate,
        Self::Apocopate,
    ];

    pub fn from_u32(v: u32) -> Self {
        match v % 9 {
            0 => Self::AMutate,
            1 => Self::IMutate,
            2 => Self::BreakVowels,
            3 => Self::OpenConsonants,
            4 => Self::SonorizeConsonants,
            5 => Self::VocalizeConsonants,
            6 => Self::Apheresize,
            7 => Self::Syncopate,
            _ => Self::Apocopate,
        }
    }

    pub fn apply(self, word: &Word) -> Word {
        match self {
            Self::AMutate => a_mutate(word),
            Self::IMutate => i_mutate(word),
            Self::BreakVowels => break_vowels(word),
            Self::OpenConsonants => open_consonants(word),
            Self::SonorizeConsonants => sonorize_consonants(word),
            Self::VocalizeConsonants => vocalize_consonants(word),
            Self::Apheresize => apheresize(word),
            Self::Syncopate => syncopate(word),
            Self::Apocopate => apocopate(word),
        }
    }
}

/// Height metaphony (`metaphony.py:21-22`).
pub fn a_mutate(word: &Word) -> Word {
    if word.syllables.len() < 2 {
        return word.clone();
    }
    let si = word.stressed_index();
    if si + 1 >= word.syllables.len() {
        return word.clone();
    }
    let s_curr = &word.syllables[si];
    let s_next = &word.syllables[si + 1];
    if s_curr.nucleus.len() > 1 || s_curr.nucleus.is_empty() || s_next.nucleus.is_empty() {
        return word.clone();
    }
    let v0 = s_curr.nucleus[0];
    let v1 = s_next.nucleus[0];
    let (h0, b0, r0) = vowel_pos(v0);
    let (h1, _, _) = vowel_pos(v1);
    let dh = (h1 - h0).signum();
    if dh == 0 {
        return word.clone();
    }
    let all_v: Vec<usize> = (0..VOWELS.len()).collect();
    let nv = nearest_vowel_to_pos((h0 + dh, b0, r0), &all_v);
    let mut out = word.clone();
    out.syllables[si].nucleus = vec![nv];
    out
}

/// Backness/roundedness umlaut (`metaphony.py:25-26`).
pub fn i_mutate(word: &Word) -> Word {
    if word.syllables.len() < 2 {
        return word.clone();
    }
    let si = word.stressed_index();
    if si + 1 >= word.syllables.len() {
        return word.clone();
    }
    let s_curr = &word.syllables[si];
    let s_next = &word.syllables[si + 1];
    if s_curr.nucleus.len() > 1 || s_curr.nucleus.is_empty() || s_next.nucleus.is_empty() {
        return word.clone();
    }
    let v0 = s_curr.nucleus[0];
    let v1 = s_next.nucleus[0];
    let (h0, b0, _) = vowel_pos(v0);
    let (_, b1, r1) = vowel_pos(v1);
    let db = (b1 - b0).signum();
    let all_v: Vec<usize> = (0..VOWELS.len()).collect();
    let nv = nearest_vowel_to_pos((h0, b0 + db, r1), &all_v);
    let mut out = word.clone();
    out.syllables[si].nucleus = vec![nv];
    out
}

/// Vowel breaking of stressed monophthongs (`breaking.py:5-19`).
pub fn break_vowels(word: &Word) -> Word {
    if word.syllables.is_empty() {
        return word.clone();
    }
    let si = word.stressed_index();
    let s = &word.syllables[si];
    if s.nucleus.len() != 1 {
        return word.clone();
    }
    let v = s.nucleus[0];
    let (h, b, r) = vowel_pos(v);
    let all_v: Vec<usize> = (0..VOWELS.len()).collect();
    let pair = if h == 0 {
        let onglide = nearest_vowel_to_pos((2, 2, r), &all_v);
        vec![onglide, v]
    } else {
        let target_b = if b < 2 { 0 } else if b > 2 { 4 } else { 2 };
        let onglide = nearest_vowel_to_pos((0, target_b, r), &all_v);
        vec![onglide, v]
    };
    let mut out = word.clone();
    out.syllables[si].nucleus = pair;
    out
}

/// Spirantization: intervocalic plosive -> fricative (`lenition.py:41-52`).
pub fn open_consonants(word: &Word) -> Word {
    lenite_intervocalic(word, |c| {
        let cd = &CONSONANTS[c];
        if cd.manner == 0 {
            let all_c: Vec<usize> = (0..CONSONANTS.len()).collect();
            nearest_consonant_to_pos((2, cd.place, cd.voice), &all_c)
        } else {
            c
        }
    })
}

/// Intervocalic voicing: unvoiced -> voiced (`lenition.py:55-64`).
pub fn sonorize_consonants(word: &Word) -> Word {
    lenite_intervocalic(word, |c| {
        let cd = &CONSONANTS[c];
        if cd.voice == 0 {
            let all_c: Vec<usize> = (0..CONSONANTS.len()).collect();
            nearest_consonant_to_pos((cd.manner, cd.place, 1), &all_c)
        } else {
            c
        }
    })
}

fn lenite_intervocalic<F: Fn(usize) -> usize>(word: &Word, f: F) -> Word {
    let mut out = word.clone();
    let n = out.syllables.len();
    for i in 0..n {
        // Single onset consonant preceded by a vowel-final previous syllable (`coda.is_empty()`)
        if i > 0
            && out.syllables[i - 1].coda.is_empty()
            && out.syllables[i].onset.len() == 1
        {
            let c = out.syllables[i].onset[0];
            out.syllables[i].onset[0] = f(c);
        }
        // Single coda consonant followed by a vowel-initial next syllable (`onset.is_empty()`)
        if i + 1 < n
            && out.syllables[i + 1].onset.is_empty()
            && out.syllables[i].coda.len() == 1
        {
            let c = out.syllables[i].coda[0];
            out.syllables[i].coda[0] = f(c);
        }
    }
    out
}

/// Vocalization of coda consonant before another consonant into semivowel `j`/`w` (`lenition.py:67-76`).
pub fn vocalize_consonants(word: &Word) -> Word {
    let mut out = word.clone();
    let all_c: Vec<usize> = (0..CONSONANTS.len()).collect();
    let n = out.syllables.len();
    for i in 0..n {
        let has_following_c = out.syllables[i].coda.len() > 1
            || (i + 1 < n && !out.syllables[i + 1].onset.is_empty());
        if has_following_c && !out.syllables[i].coda.is_empty() {
            let c = out.syllables[i].coda[0];
            let cd = &CONSONANTS[c];
            let sv = nearest_consonant_to_pos((5, cd.place, 1), &all_c);
            out.syllables[i].coda[0] = sv;
        }
    }
    out
}

/// Drop initial unstressed vowel (`elision.py:16-24`).
pub fn apheresize(word: &Word) -> Word {
    if word.syllables.len() < 2 || word.stressed_index() == 0 {
        return word.clone();
    }
    if !word.syllables[0].onset.is_empty() {
        return word.clone();
    }
    let mut out = word.clone();
    let s0 = out.syllables.remove(0);
    let mut prefix = s0.coda;
    prefix.extend(out.syllables[0].onset.iter().copied());
    out.syllables[0].onset = prefix;
    out
}

/// Drop medial unstressed vowel (`elision.py:27-37`).
pub fn syncopate(word: &Word) -> Word {
    if word.syllables.len() < 3 {
        return word.clone();
    }
    let si = word.stressed_index();
    let target = (1..word.syllables.len() - 1).find(|&idx| idx != si);
    let Some(idx) = target else {
        return word.clone();
    };
    let mut out = word.clone();
    let removed = out.syllables.remove(idx);
    out.syllables[idx - 1].coda.extend(removed.onset);
    let mut next_onset = removed.coda;
    next_onset.extend(out.syllables[idx].onset.iter().copied());
    out.syllables[idx].onset = next_onset;
    out
}

/// Drop final unstressed vowel (`elision.py:40-48`).
pub fn apocopate(word: &Word) -> Word {
    if word.syllables.len() < 2 {
        return word.clone();
    }
    let last_idx = word.syllables.len() - 1;
    if word.stressed_index() == last_idx || !word.syllables[last_idx].coda.is_empty() {
        return word.clone();
    }
    let mut out = word.clone();
    let last = out.syllables.pop().unwrap();
    let prev_last = out.syllables.len() - 1;
    out.syllables[prev_last].coda.extend(last.onset);
    out
}

/// Etymology chain for a word (`language/dictionary.py:Origin`).
#[derive(Clone, Debug)]
pub struct WordOrigin {
    pub word: Word,
    pub lang_name: String,
    pub parent: Option<Box<WordOrigin>>,
}

impl WordOrigin {
    pub fn new(word: Word, lang_name: String, parent: Option<WordOrigin>) -> Self {
        Self {
            word,
            lang_name,
            parent: parent.map(Box::new),
        }
    }

    pub fn mutate(&self, change: SoundChangeKind) -> Self {
        let mutated = change.apply(&self.word);
        if mutated == self.word {
            self.clone()
        } else {
            Self::new(mutated, self.lang_name.clone(), Some(self.clone()))
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
        let mut used = BTreeSet::new();
        let mut origins = Vec::with_capacity(Self::WORD_COUNT);
        while origins.len() < Self::WORD_COUNT {
            let w = self.dest_lang.generate_word(&mut self.rng);
            if used.insert(w.clone()) {
                origins.push(WordOrigin::new(w, lang_name.clone(), None));
            }
        }
        self.source_origins = origins.clone();
        self.dest_origins = origins;
    }

    pub fn borrow(&mut self) {
        let donor_lang = Language::generate(&mut self.rng);
        self.lang_counter += 1;
        let donor_name = format!("Donor_{}", self.lang_counter);
        let recip_name = format!("Lang_{}", self.lang_counter);

        let mut used = BTreeSet::new();
        let mut src = Vec::with_capacity(Self::WORD_COUNT);
        let mut dst = Vec::with_capacity(Self::WORD_COUNT);

        while src.len() < Self::WORD_COUNT {
            let w = donor_lang.generate_word(&mut self.rng);
            if used.insert(w.clone()) {
                let donor_origin = WordOrigin::new(w.clone(), donor_name.clone(), None);
                let borrowed = self.dest_lang.borrow_word(&w);
                let recip_origin = WordOrigin::new(borrowed, recip_name.clone(), Some(donor_origin.clone()));
                src.push(donor_origin);
                dst.push(recip_origin);
            }
        }
        self.source_origins = src;
        self.dest_origins = dst;
    }

    pub fn apply_sound_change(&mut self, kind: SoundChangeKind) {
        self.dest_origins = self
            .dest_origins
            .iter()
            .map(|o| o.mutate(kind))
            .collect();
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
}
