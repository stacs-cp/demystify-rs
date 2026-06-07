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

    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");
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
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");
    let _ = planner.quick_solve().expect("quick_solve");
    assert!(
        planner.is_solved(),
        "negation-encoded puzzle should also fully solve"
    );
}

#[wasm_bindgen_test]
fn aux_int_matrix_drives_table_constraint() {
    // Build a tiny puzzle where a VAR-int cell is tied to an AUX-int
    // cell via a forbidden-tuples table, and a sum_eq pins the VAR.
    // The whole thing should have a unique solution; the AUX-int value
    // should be deduced (by SAT) but stay out of the user-facing
    // provableLiterals / unfixedVars surface because it's AUX.
    use serde_wasm_bindgen::to_value;
    use std::collections::BTreeSet;

    let b = WasmBuilder::new();
    let dims = to_value(&vec![vec![1_i64, 1]]).expect("encode dims");
    let pick = b
        .var_int_matrix("pick", dims.clone(), vec![1, 2, 3])
        .expect("var_int_matrix");
    let aux = b
        .aux_int_matrix("aux", dims, vec![10, 20, 30])
        .expect("aux_int_matrix");

    // Table: only (pick,aux) ∈ {(1,10), (2,20), (3,30)} are allowed.
    let bind = b.con_bool("bind").expect("con_bool bind");
    let bind_guard = b
        .guard(&bind, "bind", "pick determines aux")
        .expect("guard bind");
    let mut forbidden_tuples: Vec<Vec<i64>> = Vec::new();
    for p in &[1_i64, 2, 3] {
        for a in &[10_i64, 20, 30] {
            if !((*p == 1 && *a == 10) || (*p == 2 && *a == 20) || (*p == 3 && *a == 30)) {
                forbidden_tuples.push(vec![*p, *a]);
            }
        }
    }
    let forbidden = to_value(&forbidden_tuples).expect("encode forbidden");
    let cells = vec![
        pick.cell(vec![1]).expect("pick[1]"),
        aux.cell(vec![1]).expect("aux[1]"),
    ];
    b.table(bind_guard, cells, forbidden).expect("table");

    // sum_eq on the AUX-int value-lit: pin aux = 20.  Forces pick = 2
    // via the table, even though the pin targets the AUX side.
    let pin = b.con_bool("pin").expect("con_bool pin");
    let pin_guard = b.guard(&pin, "pin", "aux is 20").expect("guard pin");
    let aux_eq_20 = aux.cell(vec![1]).expect("aux[1]").eq(20).expect("eq 20");
    b.sum_eq(pin_guard, vec![aux_eq_20], 1).expect("sum_eq");

    let puzzle = b.build().expect("build");
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");

    // Uniqueness: only the AUX side was pinned, but the table forces
    // the VAR side too — so the puzzle should classify as unique.
    let uniq: serde_json::Value = serde_wasm_bindgen::from_value(
        planner
            .check_uniqueness(wasm_bindgen::JsValue::NULL)
            .expect("check_uniqueness"),
    )
    .expect("deserialise checkUniqueness");
    assert_eq!(uniq["status"], "unique", "got {uniq:?}");

    // Drive the planner; pick=2 lands in currentState (it's the VAR
    // side).  AUX cells stay out of currentState by design — the SAT
    // solver pins their values internally but the planner doesn't
    // surface them, matching aux_bool semantics — so we don't assert
    // on `aux` here.
    let _ = planner.quick_solve().expect("quick_solve");
    assert!(planner.is_solved(), "planner should be solved");

    let state: serde_json::Value =
        serde_wasm_bindgen::from_value(planner.current_state().expect("current_state"))
            .expect("deserialise current_state");
    assert_eq!(state["pick"]["1"], 2, "pick=2 should be deduced: {state}");

    // Sanity: the AUX-int variable shows up in auxVariables(), not
    // among the user-facing VARs.
    let aux_vars: BTreeSet<String> =
        serde_wasm_bindgen::from_value(puzzle.aux_variables().expect("aux_variables"))
            .expect("decode aux_variables");
    assert!(
        aux_vars.contains("aux"),
        "aux should be in auxVariables(): {aux_vars:?}"
    );
    let vars: BTreeSet<String> =
        serde_wasm_bindgen::from_value(puzzle.variables().expect("variables"))
            .expect("decode variables");
    assert!(vars.contains("pick"), "pick should be in variables()");
    assert!(
        !vars.contains("aux"),
        "aux must not appear in variables(): {vars:?}"
    );
}
