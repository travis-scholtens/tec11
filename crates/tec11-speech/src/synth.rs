//! IPA tokenizer and KlattGrid timeline builder.

use crate::dsp::KlattGrid;
use crate::phonemes::{
    find_consonant, find_tone, find_vowel, ConsonantKind, VowelAcoustic, CONSONANTS, TONES, VOWELS,
};

#[derive(Clone, Copy, Debug)]
pub struct SynthOptions {
    pub sample_rate: u32,
    pub base_f0: f64,
    pub rate: f64,
}

impl Default for SynthOptions {
    fn default() -> Self {
        Self {
            sample_rate: 44_100,
            base_f0: 125.0,
            rate: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Segment {
    symbol: &'static str,
    is_vowel: bool,
    long: bool,
    stressed: bool,
    tone: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
struct SyllableTokens {
    segments: Vec<Segment>,
    stressed: bool,
    tone: Option<&'static str>,
}

fn match_longest(rest: &str) -> Option<&'static str> {
    let mut best: Option<&'static str> = None;
    let mut best_len = 0usize;

    for v in VOWELS {
        if rest.starts_with(v.symbol) && v.symbol.len() > best_len {
            best = Some(v.symbol);
            best_len = v.symbol.len();
        }
    }
    for c in CONSONANTS {
        if rest.starts_with(c.symbol) && c.symbol.len() > best_len {
            best = Some(c.symbol);
            best_len = c.symbol.len();
        }
    }
    for &(t_sym, _, _) in TONES {
        if rest.starts_with(t_sym) && t_sym.len() > best_len {
            best = Some(t_sym);
            best_len = t_sym.len();
        }
    }
    for &meta in &["ˈ", "ˌ", "ː", ".", " "] {
        if rest.starts_with(meta) && meta.len() > best_len {
            best = Some(meta);
            best_len = meta.len();
        }
    }
    best
}

fn tokenize_ipa(ipa: &str) -> Result<Vec<Vec<SyllableTokens>>, String> {
    let cleaned = ipa.trim().trim_matches(|c| c == '/' || c == '[' || c == ']');
    let mut words: Vec<Vec<SyllableTokens>> = Vec::new();
    let mut cur_word: Vec<SyllableTokens> = Vec::new();
    let mut cur_syl = SyllableTokens::default();

    let mut idx = 0usize;
    while idx < cleaned.len() {
        let rest = &cleaned[idx..];
        let matched = match match_longest(rest) {
            Some(m) => m,
            None => {
                let ch = rest.chars().next().unwrap();
                if matches!(ch, '-' | '‿' | '|' | '‖' | '\t' | '\n') {
                    idx += ch.len_utf8();
                    continue;
                }
                return Err(format!(
                    "Unknown IPA symbol {:?} (U+{:04X}) at byte {} in {:?}",
                    ch, ch as u32, idx, ipa
                ));
            }
        };
        idx += matched.len();

        match matched {
            " " => {
                if !cur_syl.segments.is_empty() {
                    cur_word.push(cur_syl);
                    cur_syl = SyllableTokens::default();
                }
                if !cur_word.is_empty() {
                    words.push(cur_word);
                    cur_word = Vec::new();
                }
            }
            "." => {
                if !cur_syl.segments.is_empty() {
                    cur_word.push(cur_syl);
                    cur_syl = SyllableTokens::default();
                }
            }
            "ˈ" | "ˌ" => {
                if !cur_syl.segments.is_empty() {
                    cur_word.push(cur_syl);
                    cur_syl = SyllableTokens::default();
                }
                cur_syl.stressed = true;
            }
            "ː" => {
                if let Some(last) = cur_syl.segments.last_mut() {
                    last.long = true;
                }
            }
            sym if find_tone(sym).is_some() => {
                cur_syl.tone = Some(sym);
            }
            sym if find_vowel(sym).is_some() => {
                cur_syl.segments.push(Segment {
                    symbol: sym,
                    is_vowel: true,
                    long: false,
                    stressed: false,
                    tone: None,
                });
            }
            sym if find_consonant(sym).is_some() => {
                cur_syl.segments.push(Segment {
                    symbol: sym,
                    is_vowel: false,
                    long: false,
                    stressed: false,
                    tone: None,
                });
            }
            _ => {}
        }
    }

    if !cur_syl.segments.is_empty() {
        cur_word.push(cur_syl);
    }
    if !cur_word.is_empty() {
        words.push(cur_word);
    }

    for word in &mut words {
        for syl in word {
            for seg in &mut syl.segments {
                seg.stressed = syl.stressed;
                seg.tone = syl.tone;
            }
        }
    }

    Ok(words)
}

fn neighbor_vowel(segs: &[Segment], idx: usize) -> &'static VowelAcoustic {
    for s in &segs[(idx + 1)..] {
        if s.is_vowel {
            return find_vowel(s.symbol).unwrap();
        }
    }
    for s in segs[..idx].iter().rev() {
        if s.is_vowel {
            return find_vowel(s.symbol).unwrap();
        }
    }
    find_vowel("ə").unwrap()
}

fn seg_duration(seg: &Segment, rate: f64) -> f64 {
    let base = if seg.is_vowel {
        let mut d = find_vowel(seg.symbol).unwrap().duration;
        if seg.stressed {
            d *= 1.22;
        }
        if seg.long {
            d *= 1.85;
        }
        d
    } else {
        let mut d = find_consonant(seg.symbol).unwrap().duration;
        if seg.long {
            d *= 1.65;
        }
        d
    };
    base / rate
}

fn set_oral(kg: &mut KlattGrid, t: f64, f1: f64, f2: f64, f3: f64, b1: f64, b2: f64, b3: f64) {
    let fs = [f1, f2, f3, 3500.0, 4500.0];
    let bs = [b1, b2, b3, 250.0, 350.0];
    for k in 0..5 {
        kg.oral_f[k].add(t, fs[k]);
        kg.oral_b[k].add(t, bs[k]);
    }
}

fn set_nasal_coupling(kg: &mut KlattGrid, t: f64, fn_hz: f64, fz_hz: f64, bn: f64, bz: f64) {
    kg.nasal_f.add(t, fn_hz);
    kg.nasal_b.add(t, bn);
    kg.nasal_af.add(t, fz_hz);
    kg.nasal_ab.add(t, bz);
}

fn set_fric_formants(kg: &mut KlattGrid, t: f64, formants: &[(f64, f64, f64)]) {
    for k in 0..3 {
        if let Some(&(freq, bw, amp)) = formants.get(k) {
            kg.fric_f[k].add(t, freq);
            kg.fric_b[k].add(t, bw);
            kg.fric_a[k].add(t, amp);
        } else {
            kg.fric_f[k].add(t, 4000.0 + k as f64 * 1000.0);
            kg.fric_b[k].add(t, 500.0);
            kg.fric_a[k].add(t, 0.0);
        }
    }
}

pub fn build_klatt_grid(ipa: &str, opts: SynthOptions) -> Result<KlattGrid, String> {
    let words = tokenize_ipa(ipa)?;
    if words.is_empty() {
        return Err("Empty IPA input".to_string());
    }

    let base_f0 = opts.base_f0;
    let rate = opts.rate.max(0.1);
    let mut kg = KlattGrid::new(0.0);

    let mut t = 0.04;
    kg.voicing.add(0.0, 0.0);
    kg.aspiration.add(0.0, 0.0);
    kg.frication.add(0.0, 0.0);
    kg.open_phase.add(0.0, 0.65);
    kg.spectral_tilt.add(0.0, 12.0);
    set_nasal_coupling(&mut kg, 0.0, 280.0, 280.0, 100.0, 100.0);
    set_oral(&mut kg, 0.0, 500.0, 1480.0, 2480.0, 80.0, 110.0, 170.0);
    set_fric_formants(&mut kg, 0.0, &[]);

    for (w_idx, word) in words.iter().enumerate() {
        if w_idx > 0 {
            kg.voicing.add(t, 0.0);
            kg.aspiration.add(t, 0.0);
            kg.frication.add(t, 0.0);
            t += 0.14 / rate;
            kg.voicing.add(t, 0.0);
            kg.aspiration.add(t, 0.0);
            kg.frication.add(t, 0.0);
        }

        let mut flat: Vec<(usize, Segment)> = Vec::new();
        for (s_idx, syl) in word.iter().enumerate() {
            for seg in &syl.segments {
                flat.push((s_idx, seg.clone()));
            }
        }
        let flat_segs: Vec<Segment> = flat.iter().map(|(_, s)| s.clone()).collect();
        let n_syls = word.len();
        let is_tonal = word.iter().any(|s| s.tone.is_some());

        for (idx, (s_idx, seg)) in flat.iter().enumerate() {
            let dur = seg_duration(seg, rate);
            let t0 = t;
            let t1 = t + dur;
            let nv = neighbor_vowel(&flat_segs, idx);
            let decl = if n_syls > 1 {
                1.04 - 0.14 * (*s_idx as f64 / (n_syls - 1) as f64)
            } else {
                1.0
            };

            if seg.is_vowel {
                let v = find_vowel(seg.symbol).unwrap();
                let v_amp = if seg.stressed { 78.0 } else { 75.0 };
                let ramp = (dur * 0.18).min(0.015);

                kg.voicing.add(t0 + ramp, v_amp);
                kg.voicing.add(t1 - ramp, v_amp);
                kg.aspiration.add(t0 + ramp, 0.0);
                kg.aspiration.add(t1 - ramp, 0.0);
                kg.frication.add(t0 + ramp, 0.0);
                kg.frication.add(t1 - ramp, 0.0);

                let edge = (dur * 0.25).min(0.025);
                set_oral(&mut kg, t0 + edge, v.f1, v.f2, v.f3, v.b1, v.b2, v.b3);
                set_oral(&mut kg, t1 - edge, v.f1, v.f2, v.f3, v.b1, v.b2, v.b3);

                if v.nasal {
                    set_nasal_coupling(&mut kg, t0 + ramp, v.fn_hz, v.fz_hz, 120.0, 120.0);
                    set_nasal_coupling(&mut kg, t1 - ramp, v.fn_hz, v.fz_hz, 120.0, 120.0);
                } else {
                    set_nasal_coupling(&mut kg, t0 + ramp, 280.0, 280.0, 100.0, 100.0);
                    set_nasal_coupling(&mut kg, t1 - ramp, 280.0, 280.0, 100.0, 100.0);
                }

                if is_tonal {
                    if let Some((r0, r1)) = seg.tone.and_then(find_tone) {
                        kg.pitch.add(t0 + ramp, base_f0 * r0 * (0.98 + 0.04 * decl));
                        kg.pitch.add(t1 - ramp, base_f0 * r1 * (0.96 + 0.04 * decl));
                    } else {
                        kg.pitch.add(t0 + ramp, base_f0 * decl);
                        kg.pitch.add(t1 - ramp, base_f0 * decl * 0.96);
                    }
                } else {
                    let stress_boost = if seg.stressed { 1.18 } else { 1.0 };
                    let f0_start = base_f0 * decl * stress_boost;
                    let end_mul = if *s_idx + 1 == n_syls { 0.92 } else { 0.96 };
                    let f0_end = base_f0 * decl * stress_boost * end_mul;
                    kg.pitch.add(t0 + ramp, f0_start);
                    if seg.stressed {
                        kg.pitch.add(t0 + dur * 0.45, f0_start * 1.05);
                    }
                    kg.pitch.add(t1 - ramp, f0_end);
                }
            } else {
                let c = find_consonant(seg.symbol).unwrap();
                let f0_c = base_f0 * decl;
                kg.pitch.add(t0, f0_c);

                match c.kind {
                    ConsonantKind::Glide => {
                        kg.voicing.add(t0 + 0.008, 71.0);
                        kg.voicing.add(t1 - 0.008, 71.0);
                        kg.aspiration.add(t0 + 0.005, 0.0);
                        kg.frication.add(t0 + 0.005, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.008, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, t0 + dur * 0.45, c.f1, c.f2, c.f3, 75.0, 100.0, 160.0);
                    }
                    ConsonantKind::Liquid => {
                        kg.voicing.add(t0 + 0.008, 70.0);
                        kg.voicing.add(t1 - 0.008, 70.0);
                        kg.aspiration.add(t0 + 0.005, 0.0);
                        kg.frication.add(t0 + 0.005, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.008, 280.0, 280.0, 100.0, 100.0);
                        let f3 = if c.retroflex { 1850.0 } else { c.f3 };
                        set_oral(&mut kg, t0 + dur * 0.25, c.f1, c.f2, f3, 90.0, 130.0, 180.0);
                        set_oral(&mut kg, t0 + dur * 0.75, c.f1, c.f2, f3, 90.0, 130.0, 180.0);
                    }
                    ConsonantKind::Tap => {
                        let tm = t0 + dur * 0.5;
                        kg.voicing.add(t0, 72.0);
                        kg.voicing.add(tm, 48.0);
                        kg.voicing.add(t1, 72.0);
                        kg.aspiration.add(tm, 0.0);
                        kg.frication.add(tm, 0.0);
                        set_nasal_coupling(&mut kg, tm, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, tm, 260.0, c.f2, c.f3, 120.0, 140.0, 180.0);
                    }
                    ConsonantKind::Trill => {
                        let n_taps = 3usize;
                        let step = dur / n_taps as f64;
                        kg.aspiration.add(t0, 0.0);
                        kg.frication.add(t0, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.005, 280.0, 280.0, 100.0, 100.0);
                        for k in 0..n_taps {
                            let ta = t0 + k as f64 * step;
                            let tb = ta + step * 0.45;
                            let tc = ta + step * 0.90;
                            kg.voicing.add(ta, 71.0);
                            kg.voicing.add(tb, 42.0);
                            kg.voicing.add(tc, 71.0);
                            set_oral(&mut kg, tb, 230.0, c.f2, c.f3, 120.0, 140.0, 190.0);
                            set_oral(&mut kg, tc, 420.0, c.f2, c.f3, 90.0, 120.0, 180.0);
                        }
                    }
                    ConsonantKind::Nasal => {
                        let ramp = (dur * 0.2).min(0.012);
                        kg.voicing.add(t0 + ramp, 68.0);
                        kg.voicing.add(t1 - ramp, 68.0);
                        kg.aspiration.add(t0 + ramp, 0.0);
                        kg.frication.add(t0 + ramp, 0.0);
                        set_nasal_coupling(&mut kg, t0 + ramp, 270.0, c.nasal_zero, 90.0, 140.0);
                        set_nasal_coupling(&mut kg, t1 - ramp, 270.0, c.nasal_zero, 90.0, 140.0);
                        set_nasal_coupling(&mut kg, t1 + 0.006, 280.0, 280.0, 100.0, 100.0);
                        let f3 = if c.retroflex { 1850.0 } else { c.f3 };
                        set_oral(&mut kg, t0 + ramp, c.f1, c.f2, f3, 100.0, 180.0, 220.0);
                        set_oral(&mut kg, t1 - ramp, c.f1, c.f2, f3, 100.0, 180.0, 220.0);
                    }
                    ConsonantKind::Fricative => {
                        let ramp = (dur * 0.2).min(0.015);
                        let v_amp = if c.voiced { 64.0 } else { 0.0 };
                        kg.voicing.add(t0 + ramp, v_amp);
                        kg.voicing.add(t1 - ramp, v_amp);
                        set_nasal_coupling(&mut kg, t0 + ramp, 280.0, 280.0, 100.0, 100.0);

                        if c.symbol == "h" {
                            set_oral(&mut kg, t0 + ramp, nv.f1, nv.f2, nv.f3, 160.0, 200.0, 250.0);
                            set_oral(&mut kg, t1 - ramp, nv.f1, nv.f2, nv.f3, 160.0, 200.0, 250.0);
                        } else {
                            let f3 = if c.retroflex { 1900.0 } else { c.f3 };
                            set_oral(&mut kg, t0 + ramp, c.f1, c.f2, f3, 120.0, 160.0, 220.0);
                            set_oral(&mut kg, t1 - ramp, c.f1, c.f2, f3, 120.0, 160.0, 220.0);
                        }

                        kg.frication.add(t0, 0.0);
                        kg.frication.add(t0 + ramp, c.fric_amp);
                        kg.frication.add(t1 - ramp, c.fric_amp);
                        kg.frication.add(t1, 0.0);
                        set_fric_formants(&mut kg, t0 + ramp, c.fric_formants);
                        set_fric_formants(&mut kg, t1 - ramp, c.fric_formants);

                        kg.aspiration.add(t0, 0.0);
                        kg.aspiration.add(t0 + ramp, c.asp_amp);
                        kg.aspiration.add(t1 - ramp, c.asp_amp);
                        kg.aspiration.add(t1, 0.0);
                    }
                    ConsonantKind::GlottalStop => {
                        kg.voicing.add(t0, if idx > 0 { 68.0 } else { 0.0 });
                        kg.voicing.add(t0 + 0.010, 0.0);
                        kg.voicing.add(t1 - 0.010, 0.0);
                        kg.aspiration.add(t0 + 0.010, 0.0);
                        kg.frication.add(t0 + 0.010, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.010, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, t0 + dur * 0.5, nv.f1, nv.f2, nv.f3, 80.0, 110.0, 170.0);
                        kg.pitch.add(t0 + 0.008, f0_c * 0.88);
                        kg.pitch.add(t1 - 0.008, f0_c * 0.90);
                    }
                    ConsonantKind::Stop | ConsonantKind::Aspirated => {
                        let is_asp = c.kind == ConsonantKind::Aspirated;
                        let t_clos_end = t0 + dur * if is_asp { 0.48 } else { 0.62 };
                        let t_burst_end = (t_clos_end + 0.014 / rate).min(t1 - 0.005);
                        let clos_v = if c.voiced { 58.0 } else { 0.0 };

                        kg.voicing.add(t0 + 0.006, clos_v);
                        kg.voicing.add(t_clos_end - 0.004, clos_v);
                        kg.frication.add(t0, 0.0);
                        kg.frication.add(t_clos_end - 0.003, 0.0);
                        kg.aspiration.add(t0, 0.0);
                        kg.aspiration.add(t_clos_end - 0.003, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.006, 280.0, 280.0, 100.0, 100.0);

                        let f3 = if c.retroflex { 1850.0 } else { c.f3 };
                        set_oral(&mut kg, t0 + 0.010, c.f1, c.f2, f3, 120.0, 150.0, 200.0);
                        set_oral(&mut kg, t_clos_end, c.f1, c.f2, f3, 120.0, 150.0, 200.0);

                        kg.frication.add(t_clos_end, c.fric_amp);
                        kg.frication.add(t_burst_end, c.fric_amp * 0.6);
                        kg.frication.add((t_burst_end + 0.008).min(t1), 0.0);
                        set_fric_formants(&mut kg, t_clos_end, c.fric_formants);
                        set_fric_formants(&mut kg, t_burst_end, c.fric_formants);

                        if !c.voiced {
                            kg.voicing.add(t_clos_end, 0.0);
                            kg.voicing.add(t1 - 0.004, 0.0);
                            kg.aspiration.add(t_burst_end, c.asp_amp);
                            kg.aspiration.add(t1 - 0.006, c.asp_amp * 0.8);
                            kg.aspiration.add(t1, 0.0);
                        } else {
                            kg.voicing.add(t_burst_end, 68.0);
                        }

                        if c.labialized {
                            set_oral(&mut kg, t1 - 0.008, 300.0, 720.0, 2200.0, 80.0, 100.0, 160.0);
                        }
                    }
                    ConsonantKind::Affricate => {
                        let t_clos_end = t0 + dur * 0.40;
                        let clos_v = if c.voiced { 58.0 } else { 0.0 };
                        let fric_v = if c.voiced { 62.0 } else { 0.0 };
                        kg.voicing.add(t0 + 0.006, clos_v);
                        kg.voicing.add(t_clos_end - 0.004, clos_v);
                        kg.voicing.add(t_clos_end + 0.006, fric_v);
                        kg.voicing.add(t1 - 0.008, fric_v);

                        kg.aspiration.add(t0, 0.0);
                        kg.frication.add(t0, 0.0);
                        kg.frication.add(t_clos_end - 0.003, 0.0);
                        kg.frication.add(t_clos_end + 0.004, c.fric_amp);
                        kg.frication.add(t1 - 0.010, c.fric_amp);
                        kg.frication.add(t1, 0.0);
                        set_fric_formants(&mut kg, t_clos_end + 0.004, c.fric_formants);
                        set_fric_formants(&mut kg, t1 - 0.010, c.fric_formants);

                        set_nasal_coupling(&mut kg, t0 + 0.006, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, t_clos_end, c.f1, c.f2, c.f3, 120.0, 150.0, 200.0);
                        set_oral(&mut kg, t1 - 0.008, c.f1, c.f2, c.f3, 120.0, 150.0, 200.0);
                    }
                    ConsonantKind::Ejective => {
                        let is_affr = matches!(c.symbol, "tsʼ" | "tʃʼ");
                        let t_clos_end = t0 + dur * 0.42;
                        let t_burst_end = t_clos_end + if is_affr { dur * 0.30 } else { 0.014 / rate };
                        kg.voicing.add(t0 + 0.005, 0.0);
                        kg.voicing.add(t1 - 0.004, 0.0);
                        kg.aspiration.add(t0, 0.0);
                        kg.aspiration.add(t1, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.005, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, t_clos_end, c.f1, c.f2, c.f3, 110.0, 140.0, 190.0);

                        kg.frication.add(t0, 0.0);
                        kg.frication.add(t_clos_end - 0.003, 0.0);
                        kg.frication.add(t_clos_end, c.fric_amp);
                        kg.frication.add(t_burst_end, c.fric_amp * if is_affr { 0.85 } else { 0.5 });
                        kg.frication.add((t_burst_end + 0.005).min(t1), 0.0);
                        kg.frication.add(t1, 0.0);
                        set_fric_formants(&mut kg, t_clos_end, c.fric_formants);
                        set_fric_formants(&mut kg, t_burst_end, c.fric_formants);
                    }
                    ConsonantKind::Implosive => {
                        let t_clos_end = t0 + dur * 0.72;
                        let t_burst_end = (t_clos_end + 0.012 / rate).min(t1 - 0.005);
                        kg.voicing.add(t0 + 0.006, 54.0);
                        kg.voicing.add(t_clos_end - 0.004, 68.0);
                        kg.voicing.add(t1 - 0.004, 72.0);
                        kg.pitch.add(t0 + 0.006, f0_c * 0.96);
                        kg.pitch.add(t_clos_end - 0.006, f0_c * 0.85);
                        kg.pitch.add(t1, f0_c * 1.00);
                        kg.aspiration.add(t0, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.006, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, t0 + 0.010, c.f1, c.f2, c.f3, 110.0, 140.0, 190.0);
                        set_oral(&mut kg, t_clos_end, c.f1, c.f2, c.f3, 110.0, 140.0, 190.0);

                        kg.frication.add(t0, 0.0);
                        kg.frication.add(t_clos_end - 0.003, 0.0);
                        kg.frication.add(t_clos_end, c.fric_amp);
                        kg.frication.add(t_burst_end, 0.0);
                        set_fric_formants(&mut kg, t_clos_end, c.fric_formants);
                    }
                    ConsonantKind::Click => {
                        let t_burst_start = t0 + dur * 0.25;
                        let burst_len = if c.lateral { 0.026 } else { 0.016 } / rate;
                        let t_burst_end = (t_burst_start + burst_len).min(t1 - 0.010);
                        kg.voicing.add(t0 + 0.004, 0.0);
                        kg.voicing.add(t1 - 0.004, 0.0);
                        kg.aspiration.add(t0, 0.0);
                        set_nasal_coupling(&mut kg, t0 + 0.004, 280.0, 280.0, 100.0, 100.0);
                        set_oral(&mut kg, t_burst_start, c.f1, c.f2, c.f3, 120.0, 150.0, 200.0);

                        kg.frication.add(t0, 0.0);
                        kg.frication.add(t_burst_start - 0.002, 0.0);
                        kg.frication.add(t_burst_start, c.fric_amp);
                        kg.frication.add(t_burst_end, c.fric_amp * 0.5);
                        kg.frication.add((t_burst_end + 0.004).min(t1), 0.0);
                        kg.frication.add(t1, 0.0);
                        set_fric_formants(&mut kg, t_burst_start, c.fric_formants);
                        set_fric_formants(&mut kg, t_burst_end, c.fric_formants);
                    }
                }
            }

            t = t1;
        }
    }

    let total_dur = t + 0.08;
    kg.voicing.add(t + 0.015, 0.0);
    kg.voicing.add(total_dur, 0.0);
    kg.aspiration.add(t + 0.010, 0.0);
    kg.aspiration.add(total_dur, 0.0);
    kg.frication.add(t + 0.010, 0.0);
    kg.frication.add(total_dur, 0.0);
    kg.duration = total_dur;

    Ok(kg)
}
