//! WASM port of demystify-builder/tests/sudoku_4x4.rs.
//!
//! Builds a 4x4 sudoku via the wasm Builder bindings and checks the planner
//! deduces the unique reference solution.  Mirrors the Lua version.

use serde_wasm_bindgen::from_value;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmBuilder, WasmPlanner, WasmPuzzle};

const N: i64 = 4;

const SOLUTION: [[i64; 4]; 4] = [[1, 2, 3, 4], [3, 4, 1, 2], [2, 1, 4, 3], [4, 3, 2, 1]];

const GIVENS: &[(i64, i64, i64)] = &[
    (1, 1, 1),
    (1, 4, 4),
    (2, 1, 3),
    (2, 4, 2),
    (3, 2, 1),
    (4, 3, 2),
];

const EXCLUSIONS: &[(i64, i64, i64)] = &[(1, 2, 1), (1, 2, 4), (4, 4, 3)];

fn cells_in_box(b: i64) -> Vec<(i64, i64)> {
    let br = ((b - 1) / 2) * 2 + 1;
    let bc = ((b - 1) % 2) * 2 + 1;
    let mut out = Vec::new();
    for dr in 0..2_i64 {
        for dc in 0..2_i64 {
            out.push((br + dr, bc + dc));
        }
    }
    out
}

fn dims_3d() -> JsValue {
    serde_wasm_bindgen::to_value(&vec![vec![1_i64, N], vec![1, N], vec![1, N]])
        .expect("encode 3-D dims")
}

fn dims_2d() -> JsValue {
    serde_wasm_bindgen::to_value(&vec![vec![1_i64, N], vec![1, N]]).expect("encode 2-D dims")
}

fn build_sudoku_4x4() -> WasmPuzzle {
    let b = WasmBuilder::new();
    b.kind("sudoku-4x4").expect("kind");

    let cell = b
        .var_bool_matrix("cell", dims_3d())
        .expect("declare cell matrix");

    // 1. Cell-exactly-one-value (unguarded).
    for r in 1..=N {
        for c in 1..=N {
            let lits: Vec<_> = (1..=N)
                .map(|v| cell.get(vec![r, c, v]).expect("cell index in range").pos())
                .collect();
            b.sum_eq_unguarded(lits, 1).expect("cell-exactly-one");
        }
    }

    // 2. Row uniqueness.
    let row_uniq = b
        .con_bool_matrix("row_uniq", dims_2d())
        .expect("row_uniq family");
    for r in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = (1..=N)
                .map(|c| cell.get(vec![r, c, v]).expect("cell").pos())
                .collect();
            let g = b
                .guard(
                    &row_uniq.get(vec![r, v]).expect("row_uniq atom"),
                    "row_uniq",
                    &format!("value {v} appears exactly once in row {r}"),
                )
                .expect("row_uniq guard");
            b.sum_eq(g, lits, 1).expect("row_uniq sum_eq");
        }
    }

    // 3. Column uniqueness.
    let col_uniq = b
        .con_bool_matrix("col_uniq", dims_2d())
        .expect("col_uniq family");
    for c in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = (1..=N)
                .map(|r| cell.get(vec![r, c, v]).expect("cell").pos())
                .collect();
            let g = b
                .guard(
                    &col_uniq.get(vec![c, v]).expect("col_uniq atom"),
                    "col_uniq",
                    &format!("value {v} appears exactly once in column {c}"),
                )
                .expect("col_uniq guard");
            b.sum_eq(g, lits, 1).expect("col_uniq sum_eq");
        }
    }

    // 4. Box uniqueness.
    let box_uniq = b
        .con_bool_matrix("box_uniq", dims_2d())
        .expect("box_uniq family");
    for bi in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = cells_in_box(bi)
                .into_iter()
                .map(|(r, c)| cell.get(vec![r, c, v]).expect("cell").pos())
                .collect();
            let g = b
                .guard(
                    &box_uniq.get(vec![bi, v]).expect("box_uniq atom"),
                    "box_uniq",
                    &format!("value {v} appears exactly once in box {bi}"),
                )
                .expect("box_uniq guard");
            b.sum_eq(g, lits, 1).expect("box_uniq sum_eq");
        }
    }

    // 5. Givens — single positive literal forced via sum_eq_unguarded.
    for &(r, c, v) in GIVENS {
        let lits = vec![cell.get(vec![r, c, v]).expect("given cell").pos()];
        b.sum_eq_unguarded(lits, 1).expect("given");
    }

    // 6. Exclusions — single negated literal via sum_eq_unguarded.
    for &(r, c, v) in EXCLUSIONS {
        let lits = vec![cell.get(vec![r, c, v]).expect("exclusion cell").neg()];
        b.sum_eq_unguarded(lits, 1).expect("exclusion");
    }

    b.build().expect("build")
}

#[wasm_bindgen_test]
fn builder_produces_three_con_families() {
    let puzzle = build_sudoku_4x4();
    assert_eq!(puzzle.kind().as_deref(), Some("sudoku-4x4"));

    // Three $#CON families (row_uniq / col_uniq / box_uniq).
    let cons: Vec<String> =
        from_value(puzzle.constraints().expect("constraints")).expect("decode constraints");
    assert_eq!(cons.len(), 3, "expected three families, got {cons:?}");

    // 3 families × N×N descriptions each.
    assert_eq!(puzzle.num_con_lits(), 3 * (N as usize) * (N as usize));
}

#[wasm_bindgen_test]
fn planner_deduces_full_solution() {
    let puzzle = build_sudoku_4x4();
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");
    let _ = planner.quick_solve().expect("quick_solve");

    let known: Vec<String> = from_value(planner.known_literals().expect("known_literals"))
        .expect("decode known_literals");

    let raw = planner.current_state().expect("current_state");
    let state: serde_json::Value = from_value(raw.clone()).expect("decode current_state as Value");

    assert!(
        planner.is_solved(),
        "planner did not finish solving sudoku-4x4 \
         (known lits: {}, state: {state})",
        known.len()
    );

    let cell = state
        .get("cell")
        .unwrap_or_else(|| panic!("current_state has no 'cell' field; got {state}"));

    let mut grid = [[0_i64; 4]; 4];
    for (r_key, row) in cell.as_object().expect("cell is object") {
        let r: usize = r_key.parse::<usize>().expect("row key int") - 1;
        for (c_key, col) in row.as_object().expect("row is object") {
            let c: usize = c_key.parse::<usize>().expect("col key int") - 1;
            for (v_key, val) in col.as_object().expect("col is object") {
                if val.as_i64() == Some(1) {
                    let v: i64 = v_key.parse::<i64>().expect("val key int");
                    grid[r][c] = v;
                }
            }
        }
    }

    assert_eq!(
        grid, SOLUTION,
        "planner did not deduce the unique sudoku-4x4 solution"
    );
}

#[wasm_bindgen_test]
fn check_solvability_returns_solution() {
    let puzzle = build_sudoku_4x4();
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");

    let res: serde_json::Value = from_value(
        planner
            .check_solvability(JsValue::NULL)
            .expect("check_solvability"),
    )
    .expect("decode result");

    assert_eq!(
        res.get("status").and_then(|s| s.as_str()),
        Some("unique"),
        "expected a unique solution; got {res}"
    );

    let solution = res
        .get("fixedVars")
        .and_then(|s| s.as_array())
        .unwrap_or_else(|| panic!("unique result must carry a fixedVars array; got {res}"));

    // Every cell[r,c,v] boolean is decided, so the solution is one equality
    // per cell variable: N^3 entries, of which the N*N "=1" ones spell out
    // the grid.
    let n = N as usize;
    assert_eq!(
        solution.len(),
        n * n * n,
        "expected one assignment per cell variable"
    );

    let mut grid = [[0_i64; 4]; 4];
    let mut ones = 0;
    for entry in solution {
        let s = entry.as_str().expect("solution entry is a string");
        let (var, val) = s.split_once('=').expect("literal has '='");
        let inner = var
            .strip_prefix("cell[")
            .and_then(|x| x.strip_suffix(']'))
            .unwrap_or_else(|| panic!("unexpected literal format: {s}"));
        let nums: Vec<i64> = inner
            .split(',')
            .map(|p| p.trim().parse::<i64>().expect("index int"))
            .collect();
        assert_eq!(nums.len(), 3, "cell literal should have 3 indices: {s}");
        if val == "1" {
            grid[(nums[0] - 1) as usize][(nums[1] - 1) as usize] = nums[2];
            ones += 1;
        }
    }
    assert_eq!(ones, n * n, "expected one placed cell per square");

    assert_eq!(
        grid, SOLUTION,
        "check_solvability solution did not match the unique sudoku-4x4 answer"
    );

    // The call solves a clone, so the caller's planner is untouched.
    assert!(
        !planner.is_solved(),
        "check_solvability should not advance the caller's planner state"
    );
}

/// A two-cell puzzle with "exactly one of x[1], x[2] is true": two
/// solutions, so check_solvability can exercise the multiple / partial /
/// unsolvable branches and the literals argument.
fn build_two_cell() -> WasmPuzzle {
    let b = WasmBuilder::new();
    b.kind("two-cell").expect("kind");
    let dims = serde_wasm_bindgen::to_value(&vec![vec![1_i64, 2]]).expect("encode dims");
    let x = b.var_bool_matrix("x", dims).expect("declare x");
    // Guarded so x counts as referenced (unguarded constraints don't), and
    // con guards are active by default, so "exactly one" really holds.
    let g = b
        .guard(
            &b.con_bool("one").expect("con atom"),
            "one",
            "exactly one of x is true",
        )
        .expect("guard");
    b.sum_eq(
        g,
        vec![
            x.get(vec![1]).expect("x[1]").pos(),
            x.get(vec![2]).expect("x[2]").pos(),
        ],
        1,
    )
    .expect("exactly-one");
    b.build().expect("build")
}

fn status_of(res: &serde_json::Value) -> Option<&str> {
    res.get("status").and_then(|s| s.as_str())
}

fn string_array(res: &serde_json::Value, field: &str) -> Vec<String> {
    res.get(field)
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("{field} missing/not an array; got {res}"))
        .iter()
        .map(|e| e.as_str().expect("array entry is a string").to_string())
        .collect()
}

#[wasm_bindgen_test]
fn check_solvability_partial_assignment() {
    let puzzle = build_two_cell();
    let planner = WasmPlanner::new(&puzzle, JsValue::NULL).expect("planner");

    // No clues: two solutions; both cells free, nothing fixed.
    let res: serde_json::Value =
        from_value(planner.check_solvability(JsValue::NULL).expect("check")).expect("decode");
    assert_eq!(status_of(&res), Some("multiple"), "{res}");
    assert!(string_array(&res, "fixedVars").is_empty(), "{res}");
    let mut unfixed = string_array(&res, "unfixedVars");
    unfixed.sort();
    assert_eq!(
        unfixed,
        vec!["x[1]".to_string(), "x[2]".to_string()],
        "{res}"
    );

    // Pin x[1]=1: x[2]=0 follows, so the completion is now unique.
    let lits = serde_wasm_bindgen::to_value(&vec!["x[1]=1".to_string()]).expect("encode lits");
    let res: serde_json::Value =
        from_value(planner.check_solvability(lits).expect("check")).expect("decode");
    assert_eq!(status_of(&res), Some("unique"), "{res}");
    let mut fixed = string_array(&res, "fixedVars");
    fixed.sort();
    assert_eq!(
        fixed,
        vec!["x[1]=1".to_string(), "x[2]=0".to_string()],
        "{res}"
    );

    // Pin both true: contradicts exactly-one, so unsolvable.
    let lits = serde_wasm_bindgen::to_value(&vec!["x[1]=1".to_string(), "x[2]=1".to_string()])
        .expect("encode lits");
    let res: serde_json::Value =
        from_value(planner.check_solvability(lits).expect("check")).expect("decode");
    assert_eq!(status_of(&res), Some("unsolvable"), "{res}");

    // A literal that doesn't resolve is an error, not a silent miss.
    let lits = serde_wasm_bindgen::to_value(&vec!["nope[9]=9".to_string()]).expect("encode lits");
    assert!(
        planner.check_solvability(lits).is_err(),
        "an unknown literal should error"
    );
}
