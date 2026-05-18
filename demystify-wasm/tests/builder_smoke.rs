//! Smoke test for the wasm Builder bindings.
//!
//! Runs under `wasm-pack test --node demystify-wasm`. Builds a tiny 2x2
//! puzzle whose only constraint forces every cell true, then checks that
//! the planner fully deduces it.

use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmBuilder, WasmPlanner};

#[wasm_bindgen_test]
fn builder_builds_and_solves_tiny_puzzle() {
    let b = WasmBuilder::new();
    b.kind("toy-builder-test").expect("kind");

    let dims =
        serde_wasm_bindgen::to_value(&vec![vec![1_i64, 2], vec![1, 2]]).expect("encode dims");
    let g = b.var_bool_matrix("g", dims).expect("var_bool_matrix");
    assert_eq!(g.name(), "g");
    assert_eq!(g.atoms().len(), 4);

    let rule = b.con_bool("rule").expect("con_bool");

    let signed = vec![
        g.get(vec![1, 1]).expect("g[1,1]").pos(),
        g.get(vec![1, 2]).expect("g[1,2]").pos(),
        g.get(vec![2, 1]).expect("g[2,1]").pos(),
        g.get(vec![2, 2]).expect("g[2,2]").pos(),
    ];
    let guard = b
        .guard(&rule, "rule", "all four cells must be true")
        .expect("guard");
    b.sum_ge(guard, signed, 4).expect("sum_ge");

    let puzzle = b.build().expect("build");
    assert_eq!(puzzle.kind().as_deref(), Some("toy-builder-test"));

    let planner = WasmPlanner::new(&puzzle).expect("planner");
    let _steps = planner.quick_solve().expect("quick_solve");
    assert!(planner.is_solved(), "planner should solve the tiny puzzle");
}

#[wasm_bindgen_test]
fn builder_rejects_repeated_build() {
    let b = WasmBuilder::new();
    let dims = serde_wasm_bindgen::to_value(&vec![vec![1_i64, 1]]).expect("encode dims");
    let g = b.var_bool_matrix("g", dims).expect("var_bool_matrix");
    let rule = b.con_bool("rule").expect("con_bool");
    let signed = vec![g.get(vec![1]).expect("g[1]").pos()];
    let guard = b.guard(&rule, "rule", "force g[1] true").expect("guard");
    b.sum_ge(guard, signed, 1).expect("sum_ge");
    b.build().expect("first build");
    assert!(b.build().is_err(), "second build() should error");
}

#[wasm_bindgen_test]
fn builder_handles_negated_literals() {
    let b = WasmBuilder::new();
    let dims =
        serde_wasm_bindgen::to_value(&vec![vec![1_i64, 2], vec![1, 2]]).expect("encode dims");
    let g = b.var_bool_matrix("g", dims).expect("var_bool_matrix");
    let rule = b.con_bool("rule").expect("con_bool");

    // sum(¬g) <= 0 ⇒ every cell true.
    let signed = vec![
        g.get(vec![1, 1]).expect("g[1,1]").neg(),
        g.get(vec![1, 2]).expect("g[1,2]").neg(),
        g.get(vec![2, 1]).expect("g[2,1]").neg(),
        g.get(vec![2, 2]).expect("g[2,2]").neg(),
    ];
    let guard = b.guard(&rule, "rule", "no cells false").expect("guard");
    b.sum_le(guard, signed, 0).expect("sum_le");
    let puzzle = b.build().expect("build");
    let planner = WasmPlanner::new(&puzzle).expect("planner");
    let _ = planner.quick_solve().expect("quick_solve");
    assert!(
        planner.is_solved(),
        "negation-encoded puzzle should also fully solve"
    );
}
