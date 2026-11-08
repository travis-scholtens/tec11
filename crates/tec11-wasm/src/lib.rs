use std::sync::atomic::{AtomicU32, Ordering};
use wasm_bindgen::prelude::*;

static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

#[wasm_bindgen]
pub fn hello(name: &str) -> String {
    let count = CALL_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    format!("Hello, {}! Greetings from Tec11 Rust/Wasm backend (call #{})", name, count)
}
