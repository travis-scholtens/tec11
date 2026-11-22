use tec11_speech::{synthesize_ipa, SynthOptions};
use wasm_bindgen::prelude::*;

/// Sample rate (Hz) used by [`synthesize_ipa_pcm`].
#[wasm_bindgen]
pub fn speech_sample_rate() -> u32 {
    SynthOptions::default().sample_rate
}

/// Synthesize an IPA string into mono `f32` PCM samples in `[-1.0, 1.0]` for Web Audio.
#[wasm_bindgen]
pub fn synthesize_ipa_pcm(ipa: &str, base_f0: f64, rate: f64) -> Result<Vec<f32>, JsValue> {
    let opts = SynthOptions {
        base_f0: if base_f0 > 0.0 { base_f0 } else { 125.0 },
        rate: if rate > 0.0 { rate } else { 1.0 },
        ..SynthOptions::default()
    };
    synthesize_ipa(ipa, opts)
        .map(|buf| buf.samples)
        .map_err(|e| JsValue::from_str(&e))
}
