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

use crate::model::TemperatureRange;

/// Classify a single tile given its seasonal raw temperature `rts` and raw precipitation `rps`
/// arrays, temperature range `templimits`, and whether `life` is enabled.
pub fn classify_tile(
    elevation: f32,
    rts: &[f64],
    rps: &[f64],
    templimits: TemperatureRange,
    life_enabled: bool,
) -> TileClimateClassification {
    let n = rts.len();
    let n_f64 = n as f64;

    let ts: Vec<f64> = rts.iter().map(|&c| templimits.to_celsius(c)).collect();
    let ps: Vec<f64> = rps.iter().map(|&c| c * 1800.0 / n_f64).collect();

    let base_thr = (ts.iter().sum::<f64>() / n_f64) * 20.0;

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
    let thr = base_thr
        + if inh >= 0.7 * tot {
            280.0
        } else if inh >= 0.3 * tot {
            140.0
        } else {
            0.0
        };

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
    // `ps` holds per-season totals (`tot / n` on average), whereas Köppen thresholds
    // (`60 mm`, `100 - tot/25 mm`, `30 mm`) are monthly (`1/12` of a year).
    let month_to_season = 12.0 / n_f64;
    let min_t = ts.iter().copied().fold(f64::INFINITY, f64::min);
    let max_t = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    // Polar Group E (`max_t <= 10 °C`) takes precedence over Arid Group B (Peel et al. 2007, Table 1)
    // so cold dry tundra / polar desert tiles are never misclassified as `BW` or `BS`.
    if max_t <= 10.0 {
        if max_t >= 0.0 {
            KoeppenCode::ET
        } else {
            KoeppenCode::EF
        }
    } else if tot <= thr {
        if tot <= thr / 2.0 {
            KoeppenCode::BW
        } else {
            KoeppenCode::BS
        }
    } else if min_t >= 18.0 {
        let min_p = ps.iter().copied().fold(f64::INFINITY, f64::min);
        if min_p >= 60.0 * month_to_season {
            KoeppenCode::Af
        } else if min_p >= (100.0 - tot / 25.0) * month_to_season {
            KoeppenCode::Am
        } else {
            KoeppenCode::Aw
        }
    } else {
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

        let dry_winter = min_cold_p < max_warm_p / 10.0;
        let dry_summer = min_warm_p < (30.0 * month_to_season).min(max_cold_p / 3.0);
        let warm_tot: f64 = warm_half.iter().map(|&i| ps[i]).sum();
        let cold_tot: f64 = cold_half.iter().map(|&i| ps[i]).sum();

        if dry_summer && (!dry_winter || warm_tot < cold_tot) {
            if is_c {
                KoeppenCode::Cs
            } else {
                KoeppenCode::Ds
            }
        } else if dry_winter {
            if is_c {
                KoeppenCode::Cw
            } else {
                KoeppenCode::Dw
            }
        } else if is_c {
            KoeppenCode::Cf
        } else {
            KoeppenCode::Df
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper that converts physical seasonal temperatures (°C) and seasonal precipitations (mm)
    /// into normalized `rts` and `rps` inputs for `classify_tile`.
    fn classify_from_seasonal_c_mm(ts_c: &[f64], ps_mm: &[f64]) -> KoeppenCode {
        let range = TemperatureRange::new(0.0, 1.0);
        let n = ps_mm.len() as f64;
        let rps: Vec<f64> = ps_mm.iter().map(|&p| p * n / 1800.0).collect();
        classify_tile(1.0, ts_c, &rps, range, true).koeppen
    }

    #[test]
    fn eight_season_wet_equatorial_tile_classifies_as_af() {
        // Uniform 26 °C and 225 mm per 1/8-year season (1800 mm/yr = 150 mm/month >= 60 mm/month):
        let ts = [26.0; 8];
        let ps = [225.0; 8];
        assert_eq!(classify_from_seasonal_c_mm(&ts, &ps), KoeppenCode::Af);
    }

    #[test]
    fn dry_summer_and_dry_winter_subtypes_match_warm_and_cold_halves() {
        // Mediterranean profile: warm dry summer (min_warm = 15 mm < 45 mm & < 180/3), mild wet winter:
        let ts_c = [8.0, 10.0, 15.0, 22.0, 26.0, 23.0, 16.0, 11.0];
        let ps_med = [180.0, 150.0, 120.0, 35.0, 15.0, 25.0, 110.0, 160.0];
        assert_eq!(classify_from_seasonal_c_mm(&ts_c, &ps_med), KoeppenCode::Cs);

        // Monsoon-influenced temperate profile: wet warm summer (max_warm = 240 mm), dry cold winter (min_cold = 12 mm < 24):
        let ps_monsoon = [12.0, 20.0, 50.0, 180.0, 240.0, 200.0, 80.0, 18.0];
        assert_eq!(classify_from_seasonal_c_mm(&ts_c, &ps_monsoon), KoeppenCode::Cw);

        // Continental counterparts (`min_t < -3 °C`):
        let ts_d = [-12.0, -8.0, 4.0, 16.0, 21.0, 17.0, 5.0, -6.0];
        assert_eq!(classify_from_seasonal_c_mm(&ts_d, &ps_med), KoeppenCode::Ds);
        assert_eq!(classify_from_seasonal_c_mm(&ts_d, &ps_monsoon), KoeppenCode::Dw);
    }

    #[test]
    fn tropical_monsoon_checks_driest_season_rather_than_any_season() {
        let ts = [25.0, 26.0, 28.0, 29.0, 27.0, 26.0, 25.0, 24.0];
        // Total = 1250 mm -> `100 - 1250/25 = 50 mm/mo` = `75 mm` per 1/8-year season (`< 90 mm` Af threshold).
        // Savanna with a bone-dry season (`0 mm`) AND a shoulder season (`80 mm` in `[75, 90)`) must be `Aw`, not `Am`:
        let ps_savanna = [0.0, 10.0, 80.0, 320.0, 360.0, 300.0, 150.0, 30.0];
        assert_eq!(classify_from_seasonal_c_mm(&ts, &ps_savanna), KoeppenCode::Aw);

        // True monsoon where even the driest season (`78 mm`) exceeds `75 mm` (`< 90 mm`) must be `Am`:
        let ps_monsoon = [78.0, 82.0, 110.0, 240.0, 280.0, 230.0, 140.0, 90.0];
        assert_eq!(classify_from_seasonal_c_mm(&ts, &ps_monsoon), KoeppenCode::Am);
    }

    #[test]
    fn polar_group_takes_precedence_over_arid_threshold() {
        // Cold subpolar tundra (`mean_t = +3 °C`, `max_t = +7 °C <= 10 °C`) with summer-concentrated
        // 150 mm/yr precip (`thr = 3*20 + 280 = 340 mm > 150 mm`) must be `ET`, not `BW`:
        let ts = [-1.0, 0.0, 2.0, 6.0, 7.0, 6.0, 3.0, 1.0];
        let ps = [5.0, 5.0, 10.0, 35.0, 45.0, 30.0, 15.0, 5.0];
        assert_eq!(classify_from_seasonal_c_mm(&ts, &ps), KoeppenCode::ET);
    }
}
