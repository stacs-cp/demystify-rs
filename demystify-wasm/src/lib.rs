//! WebAssembly bindings for `demystify`.
//!
//! Mirrors the API surface of `demystify-lua`: load a pre-parsed puzzle JSON,
//! build a planner, step through deductions, fix literals manually.
//!
//! Build with `wasm-pack build --target web demystify-wasm`.

use wasm_bindgen::prelude::*;

mod planner;
mod puzzle;

pub use planner::*;
pub use puzzle::*;

/// Installed once on module load. Routes Rust panics to readable JS errors.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}
