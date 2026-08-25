//! Correctness test for the `parallel` (multi-threaded) build.
//!
//! Only compiled with `--features parallel`, and only runnable in a real
//! browser — `wasm-pack test --node` has no Web `Worker` global, which is what
//! `wasm-bindgen-rayon` spawns.  Run via `make wasm-mt-test`.
//!
//! Runs `run_in_dedicated_worker`, not `run_in_browser`: the parallel build
//! blocks on a mutex during MUS search, which the browser main thread is not
//! permitted to do.  A `run_in_browser` version of this test would hang.
//!
//! The wasm-bindgen test server already sends COOP/COEP, so `SharedArrayBuffer`
//! is available without extra setup.
//!
//! This is a *correctness* test, not a benchmark.  It solves the same 4x4
//! sudoku as `builder_sudoku` and demands the identical unique solution, so a
//! race in the shared `MusDict` or the per-worker `ThreadLocal<SatCore>` shows
//! up as a wrong or incomplete grid.  It is far too small to say anything about
//! speedup — measure that separately on a genuinely hard puzzle.

#![cfg(feature = "parallel")]

use std::collections::BTreeSet;

use serde_wasm_bindgen::from_value;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use demystify_wasm::{WasmPlanner, init_thread_pool};

mod common;
use common::{SOLUTION, build_sudoku_4x4, grid_from_state};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// Deliberately a single test function rather than several.  The pool must be
/// installed before *any* rayon call, and `wasm_bindgen_test` gives no ordering
/// guarantee between test functions sharing a page — so a second test could
/// otherwise race ahead and poison the pool.
#[wasm_bindgen_test]
async fn threaded_pool_runs_and_solves_sudoku() {
    wasm_bindgen_futures::JsFuture::from(init_thread_pool(4))
        .await
        .expect("initThreadPool rejected — is the page cross-origin isolated?");

    let n = rayon::current_num_threads();
    assert_eq!(n, 4, "expected a 4-thread pool, got {n}");

    // `current_num_threads` only reports the *configured* size, which would
    // still read 4 if no worker ever started.  `broadcast` runs its closure
    // exactly once per pool thread, so distinct thread ids prove the workers
    // are real and executing.
    let distinct: BTreeSet<String> =
        rayon::broadcast(|_| format!("{:?}", std::thread::current().id()))
            .into_iter()
            .collect();
    assert!(
        distinct.len() > 1,
        "pool reports {n} threads but only {} distinct thread id(s) ran — still effectively serial",
        distinct.len()
    );

    // Real work: row/column/box constraints, several deduction steps, and a
    // MUS search per step -- the code paths that actually run under rayon.
    let puzzle = build_sudoku_4x4();
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");
    let steps: Vec<serde_json::Value> =
        from_value(planner.quick_solve().expect("quick_solve")).expect("decode steps");

    let state: serde_json::Value =
        from_value(planner.current_state().expect("current_state")).expect("decode current_state");

    assert!(
        planner.is_solved(),
        "threaded planner did not finish solving sudoku-4x4 (state: {state})"
    );
    assert_eq!(
        grid_from_state(&state),
        SOLUTION,
        "threaded planner deduced a different grid from the single-threaded run"
    );
    assert!(
        !steps.is_empty(),
        "expected at least one explained deduction step"
    );
}
