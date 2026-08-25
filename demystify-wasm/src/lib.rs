//! WebAssembly bindings for `demystify`.
//!
//! Mirrors the API surface of `demystify-lua`. Two entry points:
//!
//! - **Load a pre-parsed puzzle** with [`load_puzzle`], then drive a
//!   [`WasmPlanner`] through `bestStep` / `quickSolve` / `currentState`.
//!   Use this when Conjure has already compiled a `.eprime` / `.param` pair
//!   to JSON on the native side.
//! - **Build a puzzle in-process** with [`WasmBuilder`] — boolean matrices,
//!   cardinality constraints, `$#REVEAL` cascades.  No Conjure required;
//!   this is the path for embedders that need to construct puzzles at
//!   runtime in the browser (minesweeper, star battle, sudoku, …).
//!
//! Build with `wasm-pack build --target web demystify-wasm`.

use wasm_bindgen::prelude::*;

mod builder;
mod planner;
mod puzzle;
mod serde_helpers;

pub use builder::*;
pub use planner::*;
pub use puzzle::*;

/// Exposed to JS as `initThreadPool(numThreads)` by the `parallel` feature.
///
/// **Must be awaited before constructing a [`WasmPlanner`]** — see the note
/// on [`WasmPlanner::new`].  `rayon` installs a single-threaded fallback pool
/// the first time any of its APIs is touched, and the real pool can only be
/// installed if that has not happened yet.
#[cfg(feature = "parallel")]
pub use wasm_bindgen_rayon::init_thread_pool;

/// Installed once on module load. Routes Rust panics to readable JS errors.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}
