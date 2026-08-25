//! Verifies the `parallel` build refuses to run serially by accident.
//!
//! Lives in its own test binary — and therefore its own page — on purpose.
//! Constructing a planner without a worker pool installs rayon's
//! single-threaded fallback process-wide, after which `initThreadPool` can
//! never succeed.  Sharing a page with [`threads_mt`] would poison it.
//!
//! Run via `make wasm-mt-test`.

#![cfg(feature = "parallel")]

use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use demystify_wasm::{WasmBuilder, WasmPlanner};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
fn planner_refuses_to_construct_without_thread_pool() {
    // Deliberately no `initThreadPool` call.
    let b = WasmBuilder::new();
    b.kind("toy-guard-test").expect("kind");
    let dims = serde_wasm_bindgen::to_value(&vec![vec![1_i64, 1]]).expect("encode dims");
    let g = b.var_bool_matrix("g", dims).expect("var_bool_matrix");
    let rule = b.con_bool("rule").expect("con_bool");
    let signed = vec![g.get(vec![1]).expect("g[1]").pos()];
    let guard = b.guard(&rule, "rule", "g[1] must be true").expect("guard");
    b.sum_ge(guard, signed, 1).expect("sum_ge");
    let puzzle = b.build().expect("build");

    assert!(
        WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).is_err(),
        "the parallel build must reject planner construction before initThreadPool, \
         rather than silently running single-threaded"
    );
}
