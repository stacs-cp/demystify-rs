//! End-to-end smoke test for the wasm bindings.
//!
//! Runs only under `wasm-pack test --node demystify-wasm` (or `--firefox` etc.).
//! Calls into BatSat through the wasm bindings, exercising the whole wasm
//! pipeline in the same sandbox the browser would use. The methods that return
//! `JsValue` (`bestStep`) can't be inspected from native cargo test — that's
//! why these are not also `#[test]`.

use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmPlanner, load_puzzle};

const LITTLE1_JSON: &str = include_str!("fixtures/little1.json");

#[wasm_bindgen_test]
fn loads_little1_puzzle() {
    let puzzle = load_puzzle(LITTLE1_JSON).expect("load_puzzle should succeed");
    assert_eq!(puzzle.kind().as_deref(), Some("Tiny"));
    assert!(puzzle.num_clauses() > 0);
    assert!(puzzle.num_var_lits() > 0);
}

#[wasm_bindgen_test]
fn planner_solves_little1() {
    let puzzle = load_puzzle(LITTLE1_JSON).expect("load_puzzle should succeed");
    let planner = WasmPlanner::new(&puzzle).expect("planner construction should succeed");

    // Cap the loop so a regression that stops making progress fails fast
    // instead of hanging the test runner.
    for _ in 0..200 {
        if planner.is_solved() {
            return;
        }
        let step = planner.best_step().expect("best_step should not error");
        if step.is_null() {
            break;
        }
    }

    assert!(
        planner.is_solved(),
        "planner did not solve little1 within 200 steps"
    );
}
