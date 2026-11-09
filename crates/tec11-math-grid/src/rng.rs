/// Portable deterministic 128-bit PCG-XSL-RR (`Pcg64Mcg`) PRNG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcg64Mcg {
    state: u128,
}

impl Pcg64Mcg {
    const MULTIPLIER: u128 = 0x2360_ED05_1FC6_5DA4_4385_DF64_9FCC_F645;

    pub fn new(seed: u128) -> Self {
        // MCG state must be odd
        Self { state: seed | 1 }
    }

    pub fn from_seed_u64(seed: u64) -> Self {
        let s = ((seed as u128) << 64) | (seed as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut rng = Self::new(s);
        let _ = rng.next_u64();
        rng
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        self.state.to_le_bytes()
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self {
            state: u128::from_le_bytes(bytes) | 1,
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER);
        let rot = (self.state >> 122) as u32;
        let xsl = ((self.state >> 64) as u64) ^ (self.state as u64);
        xsl.rotate_right(rot)
    }

    /// Uniform float in `[0.0, 1.0)`.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / ((1u64 << 53) as f64);
        ((self.next_u64() >> 11) as f64) * SCALE
    }

    /// Uniform float in `[low, high)`.
    #[inline]
    pub fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next_f64()
    }

    /// Uniform index in `0..len`.
    #[inline]
    pub fn gen_index(&mut self, len: usize) -> usize {
        (self.next_u64() as usize) % len
    }

    /// Gaussian (normal) distribution with `mean` and `std_dev` via Box-Muller transform.
    #[inline]
    pub fn gauss(&mut self, mean: f64, std_dev: f64) -> f64 {
        let u1 = self.next_f64().max(1e-15);
        let u2 = self.next_f64();
        let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        mean + std_dev * z0
    }
}
