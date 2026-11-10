use tec11_geology::{LanguageSandbox, SoundChangeKind};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmLanguageSession {
    sim: LanguageSandbox,
}

#[wasm_bindgen]
impl WasmLanguageSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: LanguageSandbox::new(42),
        }
    }

    pub fn generate(&mut self) {
        self.sim.generate();
    }

    pub fn borrow(&mut self) {
        self.sim.borrow();
    }

    pub fn apply_sound_change(&mut self, kind_id: u32) {
        self.sim
            .apply_sound_change(SoundChangeKind::from_u32(kind_id));
    }

    pub fn source_phonemes_text(&self) -> String {
        self.sim.source_phonemes_text()
    }

    pub fn dest_phonemes_text(&self) -> String {
        self.sim.dest_phonemes_text()
    }

    pub fn word_count(&self) -> usize {
        self.sim.word_count()
    }

    pub fn concept_label(&self, idx: usize) -> String {
        self.sim.concept_label(idx)
    }

    pub fn source_orth(&self, idx: usize) -> String {
        self.sim.source_orth(idx)
    }

    pub fn source_ipa(&self, idx: usize) -> String {
        self.sim.source_ipa(idx)
    }

    pub fn dest_orth(&self, idx: usize) -> String {
        self.sim.dest_orth(idx)
    }

    pub fn dest_ipa(&self, idx: usize) -> String {
        self.sim.dest_ipa(idx)
    }

    pub fn dest_etymology(&self, idx: usize) -> String {
        self.sim.dest_etymology(idx)
    }
}
