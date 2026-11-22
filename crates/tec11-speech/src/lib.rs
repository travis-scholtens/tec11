//! Pure-Rust Klatt-88 formant speech synthesizer for the tec11 IPA inventory.
//!
//! Zero external dependencies; compiles identically for native and `wasm32`.
//!
//! - On WASM: pass `audio.samples` and `audio.sample_rate` to Web Audio's `AudioBuffer`.
//! - On native: pass `audio.samples` to `rodio::buffer::SamplesBuffer::new(1, audio.sample_rate, audio.samples)`,
//!   or encode to WAV via [`to_wav_bytes`].

pub mod dsp;
pub mod phonemes;
pub mod synth;

pub use synth::SynthOptions;

/// Mono floating-point PCM buffer in `[-1.0, 1.0]`.
#[derive(Clone, Debug)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

/// Synthesize an IPA string into a mono PCM [`AudioBuffer`].
pub fn synthesize_ipa(ipa: &str, opts: SynthOptions) -> Result<AudioBuffer, String> {
    let mut grid = synth::build_klatt_grid(ipa, opts)?;
    let samples = grid.to_sound(opts.sample_rate);
    Ok(AudioBuffer {
        sample_rate: opts.sample_rate,
        samples,
    })
}

/// Encode an [`AudioBuffer`] as a 16-bit PCM mono WAV byte vector.
pub fn to_wav_bytes(audio: &AudioBuffer) -> Vec<u8> {
    let num_samples = audio.samples.len() as u32;
    let data_bytes = num_samples * 2;
    let riff_size = 36 + data_bytes;
    let byte_rate = audio.sample_rate * 2;

    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1u16.to_le_bytes());  // PCM format
    out.extend_from_slice(&1u16.to_le_bytes());  // mono
    out.extend_from_slice(&audio.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());  // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());

    for &s in &audio.samples {
        let clamped = s.clamp(-1.0, 1.0);
        let q = (clamped * 32767.0).round() as i16;
        out.extend_from_slice(&q.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonemes::{CONSONANTS, TONES, VOWELS};

    fn assert_valid_audio(audio: &AudioBuffer) {
        assert!(!audio.samples.is_empty());
        let mut peak = 0.0_f32;
        for &s in &audio.samples {
            assert!(s.is_finite(), "non-finite sample encountered");
            peak = peak.max(s.abs());
        }
        assert!(peak > 0.1, "synthesized audio peak too low: {peak}");
    }

    #[test]
    fn all_vowels_synthesize() {
        assert_eq!(VOWELS.len(), 32);
        let ipa: String = VOWELS.iter().map(|v| v.symbol).collect::<Vec<_>>().join(" ");
        let audio = synthesize_ipa(&ipa, SynthOptions::default()).unwrap();
        assert_valid_audio(&audio);
    }

    #[test]
    fn all_consonants_synthesize() {
        assert_eq!(CONSONANTS.len(), 63);
        let ipa: String = CONSONANTS
            .iter()
            .map(|c| format!("{}a", c.symbol))
            .collect::<Vec<_>>()
            .join(" ");
        let audio = synthesize_ipa(&ipa, SynthOptions::default()).unwrap();
        assert_valid_audio(&audio);
    }

    #[test]
    fn all_tones_and_suprasegmentals_synthesize() {
        assert_eq!(TONES.len(), 5);
        let ipa = "ˈta.kaː.na ma˥ ma˧ ma˩ ma˩˥ ma˥˩";
        let audio = synthesize_ipa(ipa, SynthOptions::default()).unwrap();
        assert_valid_audio(&audio);
    }

    #[test]
    fn wav_encoding_has_valid_header() {
        let audio = synthesize_ipa("həˈloʊ", SynthOptions::default()).unwrap();
        let wav = to_wav_bytes(&audio);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + audio.samples.len() * 2);
    }

    #[test]
    fn unknown_symbol_returns_error() {
        let err = synthesize_ipa("hello$", SynthOptions::default()).unwrap_err();
        assert!(err.contains("Unknown IPA symbol"));
    }
}
