/* dsp.rs — Pure-Rust port of Praat's KlattGrid and Resonator DSP.
 *
 * Adapted from Praat's `dwtools/Resonator.cpp` and `dwtools/KlattGrid.cpp`:
 *   Resonator.cpp:
 *     Copyright (C) 2008-2019 David Weenink
 *   KlattGrid.cpp:
 *     Copyright (C) 2008-2023 David Weenink, 2015,2017,2022-2026 Paul Boersma
 *
 * This code is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 3 of the License, or (at
 * your option) any later version.
 *
 * This code is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this work. If not, see <http://www.gnu.org/licenses/>.
 */

//! Pure-Rust Klatt-88 cascade + parallel formant synthesizer DSP.
//!
//! Faithfully implements the core acoustic synthesis equations from
//! Praat's `Resonator.cpp` and `KlattGrid.cpp` with zero external dependencies.

use std::f64::consts::PI;

const TWO_PI: f64 = 2.0 * PI;

#[inline]
fn dbspl_to_a(db: f64) -> f64 {
    if db <= 0.0 {
        0.0
    } else {
        2.0e-5 * 10.0_f64.powf(db / 20.0)
    }
}

#[inline]
fn db_to_a(db: f64) -> f64 {
    if db <= 0.0 {
        0.0
    } else {
        10.0_f64.powf(db / 20.0)
    }
}

/// Deterministic xorshift64* PRNG for aspiration and frication noise.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform float in [-1.0, 1.0].
    #[inline]
    fn uniform_bipolar(&mut self) -> f64 {
        let u = (self.next_u64() >> 11) as f64 * (1.0 / ((1u64 << 53) as f64));
        2.0 * u - 1.0
    }
}

/// Piecewise-linear time-value tier (matching Praat's `RealTier`).
#[derive(Clone, Debug, Default)]
pub struct Tier {
    points: Vec<(f64, f64)>,
}

impl Tier {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    pub fn add(&mut self, time: f64, value: f64) {
        self.points.push((time, value));
    }

    pub fn finalize(&mut self) {
        if self.points.is_empty() {
            return;
        }
        // Sort by quantized 0.01 ms timestamp, keeping the last point at identical timestamps.
        let mut keyed: Vec<(i64, usize, f64, f64)> = self
            .points
            .iter()
            .enumerate()
            .map(|(i, &(t, v))| ((t * 100_000.0).round() as i64, i, t, v))
            .collect();
        keyed.sort_by_key(|&(k, i, _, _)| (k, i));

        self.points.clear();
        for (k, _, _, v) in keyed {
            let t = k as f64 / 100_000.0;
            if let Some(last) = self.points.last_mut() {
                if (last.0 - t).abs() < 1e-9 {
                    last.1 = v;
                    continue;
                }
            }
            self.points.push((t, v));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Evaluate piecewise-linearly at `t`, clamping to endpoints (like Praat's `RealTier`).
    #[inline]
    pub fn eval(&self, t: f64) -> f64 {
        let n = self.points.len();
        if n == 0 {
            return 0.0;
        }
        if n == 1 || t <= self.points[0].0 {
            return self.points[0].1;
        }
        if t >= self.points[n - 1].0 {
            return self.points[n - 1].1;
        }
        let idx = self.points.partition_point(|&(pt, _)| pt <= t);
        let (t0, v0) = self.points[idx - 1];
        let (t1, v1) = self.points[idx];
        let frac = (t - t0) / (t1 - t0);
        v0 + frac * (v1 - v0)
    }
}

/// 2-pole digital resonator (`Resonator.cpp`).
struct Resonator {
    dt: f64,
    normalise_at_dc: bool,
    a: f64,
    b: f64,
    c: f64,
    p1: f64,
    p2: f64,
}

impl Resonator {
    fn new(dt: f64, normalise_at_dc: bool) -> Self {
        Self {
            dt,
            normalise_at_dc,
            a: 1.0,
            b: 0.0,
            c: 0.0,
            p1: 0.0,
            p2: 0.0,
        }
    }

    #[inline]
    fn set_coefficients(&mut self, freq: f64, bw: f64) {
        let r = (-PI * self.dt * bw.max(1.0)).exp();
        self.c = -(r * r);
        self.b = 2.0 * r * (TWO_PI * freq * self.dt).cos();
        if self.normalise_at_dc {
            self.a = 1.0 - self.b - self.c;
        } else {
            self.a = (1.0 + self.c) * (TWO_PI * freq * self.dt).sin();
        }
    }

    #[inline]
    fn process(&mut self, input: f64) -> f64 {
        let out = self.a * input + self.b * self.p1 + self.c * self.p2;
        self.p2 = self.p1;
        self.p1 = out;
        out
    }
}

/// 2-zero digital anti-resonator (`Resonator.cpp`).
struct AntiResonator {
    dt: f64,
    a: f64,
    b: f64,
    c: f64,
    p1: f64,
    p2: f64,
}

impl AntiResonator {
    fn new(dt: f64) -> Self {
        Self {
            dt,
            a: 1.0,
            b: 0.0,
            c: 0.0,
            p1: 0.0,
            p2: 0.0,
        }
    }

    #[inline]
    fn set_coefficients(&mut self, freq: f64, bw: f64) {
        if freq <= 0.0 && bw <= 0.0 {
            self.a = 1.0;
            self.b = -2.0;
            self.c = 1.0;
        } else {
            let r = (-PI * self.dt * bw.max(1.0)).exp();
            self.c = -(r * r);
            self.b = 2.0 * r * (TWO_PI * freq * self.dt).cos();
            let denom = 1.0 - self.b - self.c;
            self.a = if denom.abs() > 1e-12 { 1.0 / denom } else { 1.0 };
        }
    }

    #[inline]
    fn process(&mut self, input: f64) -> f64 {
        let out = self.a * (input - self.b * self.p1 - self.c * self.p2);
        self.p2 = self.p1;
        self.p1 = input;
        out
    }
}

/// Complete KlattGrid parameter container mirroring the tiers used by `klatt_synth.py`.
pub struct KlattGrid {
    pub duration: f64,
    pub pitch: Tier,
    pub voicing: Tier,
    pub aspiration: Tier,
    pub frication: Tier,
    pub open_phase: Tier,
    pub spectral_tilt: Tier,
    pub oral_f: [Tier; 5],
    pub oral_b: [Tier; 5],
    pub nasal_f: Tier,
    pub nasal_b: Tier,
    pub nasal_af: Tier,
    pub nasal_ab: Tier,
    /// Parallel frication formants (corresponding to Praat slots 2, 3, 4).
    pub fric_f: [Tier; 3],
    pub fric_b: [Tier; 3],
    pub fric_a: [Tier; 3],
}

impl KlattGrid {
    pub fn new(duration: f64) -> Self {
        Self {
            duration,
            pitch: Tier::new(),
            voicing: Tier::new(),
            aspiration: Tier::new(),
            frication: Tier::new(),
            open_phase: Tier::new(),
            spectral_tilt: Tier::new(),
            oral_f: std::array::from_fn(|_| Tier::new()),
            oral_b: std::array::from_fn(|_| Tier::new()),
            nasal_f: Tier::new(),
            nasal_b: Tier::new(),
            nasal_af: Tier::new(),
            nasal_ab: Tier::new(),
            fric_f: std::array::from_fn(|_| Tier::new()),
            fric_b: std::array::from_fn(|_| Tier::new()),
            fric_a: std::array::from_fn(|_| Tier::new()),
        }
    }

    fn finalize_all(&mut self) {
        self.pitch.finalize();
        self.voicing.finalize();
        self.aspiration.finalize();
        self.frication.finalize();
        self.open_phase.finalize();
        self.spectral_tilt.finalize();
        for k in 0..5 {
            self.oral_f[k].finalize();
            self.oral_b[k].finalize();
        }
        self.nasal_f.finalize();
        self.nasal_b.finalize();
        self.nasal_af.finalize();
        self.nasal_ab.finalize();
        for k in 0..3 {
            self.fric_f[k].finalize();
            self.fric_b[k].finalize();
            self.fric_a[k].finalize();
        }
    }

    /// Synthesize mono PCM samples in `[-1.0, 1.0]` at `sample_rate` Hz.
    pub fn to_sound(&mut self, sample_rate: u32) -> Vec<f32> {
        self.finalize_all();
        let fs = sample_rate as f64;
        let dt = 1.0 / fs;
        let n = ((self.duration * fs).round() as usize).max(1);
        let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);

        // 1. Voiced glottal source (Klatt-88 flow derivative + spectral tilt)
        let mut glottal = self.synthesize_voiced(n, dt);
        self.apply_spectral_tilt(&mut glottal, dt);

        // 2. Aspiration noise (low-passed uniform noise)
        if !self.aspiration.is_empty() {
            let mut last = 0.0;
            for i in 0..n {
                let t = (i as f64 + 0.5) * dt;
                let a = dbspl_to_a(self.aspiration.eval(t));
                if a > 0.0 {
                    let mut val = rng.uniform_bipolar();
                    val += 0.75 * last;
                    last = val;
                    glottal[i] += val * a;
                } else {
                    last *= 0.75;
                }
            }
        }

        // 3. Cascade vocal-tract filter: nasal antiformant -> nasal formant -> 5 oral formants
        let mut nasal_anti = AntiResonator::new(dt);
        let mut nasal_res = Resonator::new(dt, true);
        let mut oral_res: [Resonator; 5] = std::array::from_fn(|_| Resonator::new(dt, true));

        let use_nasal = !self.nasal_f.is_empty() && !self.nasal_af.is_empty();
        for i in 0..n {
            let t = (i as f64 + 0.5) * dt;
            let mut x = glottal[i];
            if use_nasal {
                let fn_hz = self.nasal_f.eval(t);
                let bn_hz = self.nasal_b.eval(t);
                let fz_hz = self.nasal_af.eval(t);
                let bz_hz = self.nasal_ab.eval(t);
                nasal_anti.set_coefficients(fz_hz, bz_hz);
                nasal_res.set_coefficients(fn_hz, bn_hz);
                x = nasal_anti.process(x);
                x = nasal_res.process(x);
            }
            for k in 0..5 {
                let f = self.oral_f[k].eval(t);
                let bw = self.oral_b[k].eval(t);
                if f > 0.0 && f < 0.49 * fs {
                    oral_res[k].set_coefficients(f, bw);
                    x = oral_res[k].process(x);
                }
            }
            glottal[i] = x;
        }

        // 4. Parallel frication branch (slots 2, 3, 4 with alternating signs +1, -1, +1)
        if !self.frication.is_empty() {
            let mut fric_src = vec![0.0_f64; n];
            let mut last = 0.0;
            for i in 0..n {
                let t = (i as f64 + 0.5) * dt;
                let a = dbspl_to_a(self.frication.eval(t));
                let mut val = rng.uniform_bipolar();
                val += 0.75 * last;
                last = val;
                fric_src[i] = val * a;
            }

            let mut fric_res: [Resonator; 3] = std::array::from_fn(|_| Resonator::new(dt, false));
            for k in 0..3 {
                if self.fric_f[k].is_empty() || self.fric_a[k].is_empty() {
                    continue;
                }
                let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                for i in 0..n {
                    let t = (i as f64 + 0.5) * dt;
                    let amp = db_to_a(self.fric_a[k].eval(t));
                    let f = self.fric_f[k].eval(t);
                    let bw = self.fric_b[k].eval(t);
                    if f > 0.0 && f < 0.49 * fs {
                        fric_res[k].set_coefficients(f, bw);
                        let y = fric_res[k].process(fric_src[i]);
                        glottal[i] += sign * amp * y;
                    }
                }
            }
        }

        // 5. Peak normalize to 0.99 (matching KlattGrid_to_Sound)
        let peak = glottal
            .iter()
            .fold(0.0_f64, |acc, &x| acc.max(x.abs()));
        let scale = if peak > 1e-12 { 0.99 / peak } else { 0.0 };
        glottal.into_iter().map(|x| (x * scale) as f32).collect()
    }

    fn synthesize_voiced(&self, n: usize, dt: f64) -> Vec<f64> {
        let mut out = vec![0.0_f64; n];
        if self.voicing.is_empty() || self.pitch.is_empty() {
            return out;
        }

        // Generate glottal closing instants by integrating instantaneous F0(t).
        let mut pulse_times: Vec<(f64, f64)> = Vec::new();
        let mut phase_acc = 0.0_f64;
        for i in 0..n {
            let t = (i as f64 + 0.5) * dt;
            let f0 = self.pitch.eval(t).clamp(40.0, 600.0);
            phase_acc += f0 * dt;
            if phase_acc >= 1.0 {
                phase_acc -= 1.0;
                pulse_times.push((t, 1.0 / f0));
            }
        }

        let power1 = 3.0_f64;
        let power2 = 4.0_f64;
        let norm_amp = 1.0
            / ((power1 / power2).powf(1.0 / (power2 / power1 - 1.0)) * (1.0 - power1 / power2));

        for &(t_close, period) in &pulse_times {
            let open_phase = if self.open_phase.is_empty() {
                0.65
            } else {
                self.open_phase.eval(t_close).clamp(0.2, 0.95)
            };
            let te = period * open_phase;
            let mid = ((t_close / dt).floor() as isize).clamp(0, (n as isize) - 1);
            let begin = (mid - (te / dt).floor() as isize).max(0);

            for i in begin..=mid {
                let tsamp = (i as f64 + 0.5) * dt;
                let phase = (tsamp - (t_close - te)) / te;
                if phase > 0.0 && phase <= 1.0 {
                    let flow = norm_amp * (phase.powf(power1) - phase.powf(power2));
                    out[i as usize] += flow;
                }
            }
        }

        // Flow derivative normalized to original extremum (matching KlattGrid.cpp:1128-1138)
        let orig_peak = out.iter().fold(0.0_f64, |acc, &x| acc.max(x.abs()));
        let mut last = 0.0_f64;
        for s in &mut out {
            let val = *s;
            *s = val - last;
            last = val;
        }
        let deriv_peak = out.iter().fold(0.0_f64, |acc, &x| acc.max(x.abs()));
        if deriv_peak > 1e-12 && orig_peak > 1e-12 {
            let s = orig_peak / deriv_peak;
            for x in &mut out {
                *x *= s;
            }
        }

        // Apply voicing amplitude tier in dB SPL
        for i in 0..n {
            let t = (i as f64 + 0.5) * dt;
            out[i] *= dbspl_to_a(self.voicing.eval(t));
        }

        out
    }

    fn apply_spectral_tilt(&self, samples: &mut [f64], dt: f64) {
        if self.spectral_tilt.is_empty() {
            return;
        }
        let cosf = (TWO_PI * 3000.0 * dt).cos();
        let mut ynm1 = 0.0_f64;
        for (i, s) in samples.iter_mut().enumerate() {
            let t = (i as f64 + 0.5) * dt;
            let tilt_db = self.spectral_tilt.eval(t);
            if tilt_db > 0.0 {
                let d = 10.0_f64.powf(-tilt_db / 10.0);
                let q = (1.0 - d * cosf) / (1.0 - d);
                let b = q - (q * q - 1.0).max(0.0).sqrt();
                let a = 1.0 - b;
                let y = a * *s + b * ynm1;
                *s = y;
                ynm1 = y;
            }
        }
    }
}
