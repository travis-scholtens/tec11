/// Köppen climate classification codes matching `koeppencolor.py` and `climateclassification.py`.
///
/// Exposed to the frontend as a compact `u8` ID (`0..=13`) so the frontend owns
/// the RGB color palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KoeppenCode {
    Ocean = 0,
    Af = 1,
    Am = 2,
    Aw = 3,
    BS = 4,
    BW = 5,
    Cf = 6,
    Cs = 7,
    Cw = 8,
    Df = 9,
    Ds = 10,
    Dw = 11,
    EF = 12,
    ET = 13,
}

impl KoeppenCode {
    pub fn as_str(self) -> &'static str {
        match self {
            KoeppenCode::Ocean => "",
            KoeppenCode::Af => "Af",
            KoeppenCode::Am => "Am",
            KoeppenCode::Aw => "Aw",
            KoeppenCode::BS => "BS",
            KoeppenCode::BW => "BW",
            KoeppenCode::Cf => "Cf",
            KoeppenCode::Cs => "Cs",
            KoeppenCode::Cw => "Cw",
            KoeppenCode::Df => "Df",
            KoeppenCode::Ds => "Ds",
            KoeppenCode::Dw => "Dw",
            KoeppenCode::EF => "EF",
            KoeppenCode::ET => "ET",
        }
    }
}

/// Summary of a tile's annual climate classification, matching `ClimateClassification`
/// in `climateclassification.py`.
#[derive(Clone, Copy, Debug)]
pub struct TileClimateClassification {
    pub mean_temperature: f64,
    pub mean_precipitation: f64,
    pub koeppen: KoeppenCode,
    pub life: bool,
}

/// Classify a single tile given its seasonal raw temperature `rts` and raw precipitation `rps`
/// arrays, temperature range `templimits`, and whether `life` is enabled.
pub fn classify_tile(
    elevation: f32,
    rts: &[f64],
    rps: &[f64],
    templimits: (f64, f64),
    life_enabled: bool,
) -> TileClimateClassification {
    let n = rts.len();
    let n_f64 = n as f64;

    let ts: Vec<f64> = rts
        .iter()
        .map(|&c| c * (templimits.1 - templimits.0) + templimits.0)
        .collect();
    let ps: Vec<f64> = rps.iter().map(|&c| c * 1800.0 / n_f64).collect();

    let mut thr = (ts.iter().sum::<f64>() / n_f64) * 20.0;

    let mut byt: Vec<usize> = (0..n).collect();
    byt.sort_by(|&i, &j| {
        ts[i]
            .partial_cmp(&ts[j])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| i.cmp(&j))
    });

    let tot: f64 = ps.iter().sum();
    let half = n / 2;
    let warm_half = &byt[(n - half)..];
    let cold_half = &byt[..half];

    let inh: f64 = warm_half.iter().map(|&i| ps[i]).sum();
    if inh >= 0.7 * tot {
        thr += 280.0;
    } else if inh >= 0.3 * tot {
        thr += 140.0;
    }

    let koeppen = if elevation <= 0.0 {
        KoeppenCode::Ocean
    } else {
        koeppen_code(&ts, &ps, warm_half, cold_half, thr, tot)
    };

    let life = life_enabled && ts.iter().any(|&t| t > 18.0);

    TileClimateClassification {
        mean_temperature: rts.iter().sum::<f64>() / n_f64,
        mean_precipitation: rps.iter().sum::<f64>() / n_f64,
        koeppen,
        life,
    }
}

fn koeppen_code(
    ts: &[f64],
    ps: &[f64],
    warm_half: &[usize],
    cold_half: &[usize],
    thr: f64,
    tot: f64,
) -> KoeppenCode {
    let n_f64 = ps.len() as f64;
    let min_t = ts.iter().copied().fold(f64::INFINITY, f64::min);
    let max_t = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    if tot <= thr {
        if tot <= thr / 2.0 {
            KoeppenCode::BW
        } else {
            KoeppenCode::BS
        }
    } else if min_t >= 18.0 {
        if ps.iter().all(|&p| p >= 60.0 * n_f64) {
            KoeppenCode::Af
        } else if ps
            .iter()
            .any(|&p| (100.0 - tot / 25.0) * n_f64 <= p && p < 60.0 * n_f64)
        {
            KoeppenCode::Am
        } else {
            KoeppenCode::Aw
        }
    } else if max_t > 10.0 {
        let is_c = min_t >= -3.0;
        let min_warm_p = warm_half
            .iter()
            .map(|&i| ps[i])
            .fold(f64::INFINITY, f64::min);
        let max_warm_p = warm_half
            .iter()
            .map(|&i| ps[i])
            .fold(f64::NEG_INFINITY, f64::max);
        let min_cold_p = cold_half
            .iter()
            .map(|&i| ps[i])
            .fold(f64::INFINITY, f64::min);
        let max_cold_p = cold_half
            .iter()
            .map(|&i| ps[i])
            .fold(f64::NEG_INFINITY, f64::max);

        if min_warm_p < max_cold_p / 10.0 {
            if is_c {
                KoeppenCode::Cw
            } else {
                KoeppenCode::Dw
            }
        } else if min_cold_p < (30.0 * n_f64).min(max_warm_p / 3.0) {
            if is_c {
                KoeppenCode::Cs
            } else {
                KoeppenCode::Ds
            }
        } else if is_c {
            KoeppenCode::Cf
        } else {
            KoeppenCode::Df
        }
    } else if max_t >= 0.0 {
        KoeppenCode::ET
    } else {
        KoeppenCode::EF
    }
}
