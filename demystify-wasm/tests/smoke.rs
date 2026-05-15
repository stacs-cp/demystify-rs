//! End-to-end smoke test for the wasm bindings.
//!
//! Runs only under `wasm-pack test --node demystify-wasm` (or `--firefox` etc.).
//! Calls into BatSat through the wasm bindings, exercising the whole wasm
//! pipeline in the same sandbox the browser would use. The methods that return
//! `JsValue` (`bestStep`, `provableLiterals`, etc.) can't be inspected from
//! native cargo test — that's why these are not also `#[test]`.

use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmPlanner, load_puzzle, parse_literal};
use serde_wasm_bindgen::from_value;

const LITTLE1_JSON: &str = include_str!("fixtures/little1.json");

fn load_planner() -> WasmPlanner {
    let puzzle = load_puzzle(LITTLE1_JSON).expect("load_puzzle should succeed");
    WasmPlanner::new(&puzzle).expect("planner construction should succeed")
}

#[wasm_bindgen_test]
fn loads_little1_puzzle() {
    let puzzle = load_puzzle(LITTLE1_JSON).expect("load_puzzle should succeed");
    assert_eq!(puzzle.kind().as_deref(), Some("Tiny"));
    assert!(puzzle.num_clauses() > 0);
    assert!(puzzle.num_var_lits() > 0);
    assert!(puzzle.num_con_lits() > 0);
}

#[wasm_bindgen_test]
fn puzzle_metadata_methods_return_values() {
    let puzzle = load_puzzle(LITTLE1_JSON).expect("load_puzzle should succeed");
    // Each of these returns a JsValue; just check they don't error.  Shape
    // is tested implicitly by the lua suite running the same code path.
    puzzle.info().expect("info");
    puzzle.variables().expect("variables");
    puzzle.aux_variables().expect("aux_variables");
    puzzle.constraints().expect("constraints");
    puzzle.all_domains().expect("all_domains");
    // little1.param sets `letting n be 4`, so n is a known param.
    assert!(puzzle.has_param("n"));
    assert_eq!(puzzle.param_int("n").expect("param_int"), 4);
}

#[wasm_bindgen_test]
fn planner_solves_little1() {
    let planner = load_planner();
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

#[wasm_bindgen_test]
fn provable_literals_nonempty_on_unsolved_puzzle() {
    let planner = load_planner();
    let lits: Vec<String> = from_value(planner.provable_literals().expect("provable_literals"))
        .expect("deserialise provable literals");
    assert!(!lits.is_empty(), "fresh planner should have provable lits");
    // Sanity-check format: every entry should contain "=" or "!=".
    assert!(
        lits.iter().all(|s| s.contains('=')),
        "literals should contain '=': got {lits:?}"
    );
}

#[wasm_bindgen_test]
fn fix_literal_advances_known_set() {
    let planner = load_planner();
    let provable: Vec<String> = from_value(planner.provable_literals().expect("provable_literals"))
        .expect("deserialise provable literals");
    let first = provable
        .first()
        .expect("there must be at least one provable lit")
        .clone();

    let known_before: Vec<String> =
        from_value(planner.known_literals().expect("known_literals")).expect("deserialise known");
    assert!(planner.fix_literal(&first), "fix_literal should succeed");
    let known_after: Vec<String> =
        from_value(planner.known_literals().expect("known_literals")).expect("deserialise known");

    assert!(
        known_after.len() > known_before.len(),
        "known_literals should grow after fix_literal"
    );
    assert!(
        known_after.iter().any(|k| k == &first),
        "fixed literal should now appear in known_literals: {first}"
    );
}

#[wasm_bindgen_test]
fn quick_solve_returns_steps_and_solves_puzzle() {
    let planner = load_planner();
    let result = planner.quick_solve().expect("quick_solve");
    // We don't care about the exact shape — just that it deserialises and
    // that the planner now reports as solved.
    let steps: serde_json::Value =
        from_value(result).expect("quick_solve result must be serialisable");
    assert!(steps.is_array(), "quick_solve returns an array of steps");
    assert!(
        planner.is_solved(),
        "after quick_solve, planner should be solved"
    );
}

#[wasm_bindgen_test]
fn difficulties_returns_a_map() {
    let planner = load_planner();
    let diffs: std::collections::BTreeMap<String, usize> =
        from_value(planner.difficulties().expect("difficulties")).expect("deserialise diffs");
    assert!(
        !diffs.is_empty(),
        "fresh planner should report difficulty for at least one lit"
    );
    assert!(
        diffs.values().all(|&n| n >= 1),
        "difficulty values are MUS sizes, which are >= 1"
    );
}

#[wasm_bindgen_test]
fn parse_literal_roundtrip() {
    #[derive(serde::Deserialize)]
    struct Parsed {
        name: String,
        indices: Vec<i64>,
        value: i64,
        equal: bool,
    }
    let val = parse_literal("grid[1]=2").expect("parse_literal");
    let p: Parsed = from_value(val).expect("deserialise parsed lit");
    assert_eq!(p.name, "grid");
    assert_eq!(p.indices, vec![1]);
    assert_eq!(p.value, 2);
    assert!(p.equal);

    let val = parse_literal("grid[3]!=4").expect("parse_literal !=");
    let p: Parsed = from_value(val).expect("deserialise");
    assert_eq!(p.indices, vec![3]);
    assert_eq!(p.value, 4);
    assert!(!p.equal);

    assert!(
        parse_literal("not a literal").is_err(),
        "malformed input should error"
    );
}

#[wasm_bindgen_test]
fn current_state_returns_a_table_keyed_by_variable_name() {
    let planner = load_planner();
    // current_state returns a nested {var_name: {indices...: value}} table.
    // We don't pin anything specific; just confirm the call works and
    // returns an object (even if empty for an unsolved puzzle).
    let val = planner.current_state().expect("current_state");
    let parsed: serde_json::Value = from_value(val).expect("deserialise current_state");
    assert!(
        parsed.is_object(),
        "current_state should return an object, got {parsed:?}"
    );
}
