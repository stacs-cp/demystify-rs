//! Verifies the `parallel` build refuses to run on the browser main thread.
//!
//! This is the regression test for a silent, unrecoverable hang: the parallel
//! MUS search blocks on a `std::sync::Mutex`, and the main thread is a
//! "cannot-block" agent, so a solve started there never returns — no exception,
//! nothing in the console. `WasmPlanner::new` detects the main thread up front
//! and errors instead.
//!
//! Runs `run_in_browser` on purpose; the sibling tests use
//! `run_in_dedicated_worker`. It never constructs a planner successfully, so it
//! never reaches the blocking code and cannot itself hang.
//!
//! Run via `make wasm-mt-test`.

#![cfg(feature = "parallel")]

use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use demystify_wasm::{WasmBuilder, WasmPlanner};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn planner_refuses_to_construct_on_main_thread() {
    let b = WasmBuilder::new();
    b.kind("toy-mainthread-test").expect("kind");
    let dims = serde_wasm_bindgen::to_value(&vec![vec![1_i64, 1]]).expect("encode dims");
    let g = b.var_bool_matrix("g", dims).expect("var_bool_matrix");
    let rule = b.con_bool("rule").expect("con_bool");
    let signed = vec![g.get(vec![1]).expect("g[1]").pos()];
    let guard = b.guard(&rule, "rule", "g[1] must be true").expect("guard");
    b.sum_ge(guard, signed, 1).expect("sum_ge");
    let puzzle = b.build().expect("build");

    let err = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL)
        .err()
        .expect("the parallel build must reject planner construction on the main thread");

    let msg = format!("{:?}", wasm_bindgen::JsValue::from(err));
    assert!(
        msg.contains("Web Worker"),
        "error should tell the caller to use a Web Worker, got: {msg}"
    );
}
