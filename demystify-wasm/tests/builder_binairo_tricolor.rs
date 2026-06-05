//! WASM port of demystify-builder/tests/binairo_tricolor.rs.
//!
//! Three-colour binairo (Takuzu) built with multi-valued `varIntMatrix`
//! cells and the `table` constraint: colour balance is `sumEq`, while
//! "no three in a row" and the `mod`/sequence rule are `table`s.
//!
//! 3x3 instance with grid[1,1] = 0 solves to the addition table
//! grid[i,j] = (i-1)+(j-1) mod 3.

use serde_wasm_bindgen::{from_value, to_value};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmBuilder, WasmPlanner, WasmPuzzle};

const COLOURS: [i64; 3] = [0, 1, 2];

fn dims_2d(h: i64, w: i64) -> JsValue {
    to_value(&vec![vec![1_i64, h], vec![1, w]]).expect("encode dims")
}

fn con_dims(a: i64, b: i64) -> JsValue {
    to_value(&vec![vec![1_i64, a], vec![1, b]]).expect("encode con dims")
}

fn mod_forbidden() -> JsValue {
    to_value(&vec![vec![0_i64, 2], vec![1, 0], vec![2, 1]]).expect("encode mod forbidden")
}

fn three_same_forbidden() -> JsValue {
    let tuples: Vec<Vec<i64>> = COLOURS.iter().map(|&k| vec![k, k, k]).collect();
    to_value(&tuples).expect("encode three-same forbidden")
}

fn build_binairo_tricolor(h: i64, w: i64, givens: &[(i64, i64, i64)]) -> WasmPuzzle {
    let third_w = w / 3;
    let third_h = h / 3;

    let b = WasmBuilder::new();
    b.kind("binairo-tricolor-wasm").expect("kind");

    let grid = b
        .var_int_matrix("grid", dims_2d(h, w), COLOURS.to_vec())
        .expect("grid int matrix");
    b.show("grid", "main").expect("show grid main");

    // Row colour balance.
    for &k in &COLOURS {
        let fam = format!("rowcolor{k}");
        let con = b
            .con_bool_matrix(&fam, con_dims(h, 1))
            .expect("rowcolor con");
        for i in 1..=h {
            let lits: Vec<_> = (1..=w)
                .map(|j| grid.cell(vec![i, j]).expect("cell").eq(k).expect("eq"))
                .collect();
            let g = b
                .guard(
                    &con.get(vec![i, 1]).expect("rowcolor atom"),
                    &fam,
                    &format!("row {i} has exactly {third_w} cells of colour {k}"),
                )
                .expect("guard");
            b.sum_eq(g, lits, third_w).expect("rowcolor sum_eq");
        }
    }

    // Column colour balance.
    for &k in &COLOURS {
        let fam = format!("colcolor{k}");
        let con = b
            .con_bool_matrix(&fam, con_dims(w, 1))
            .expect("colcolor con");
        for j in 1..=w {
            let lits: Vec<_> = (1..=h)
                .map(|i| grid.cell(vec![i, j]).expect("cell").eq(k).expect("eq"))
                .collect();
            let g = b
                .guard(
                    &con.get(vec![j, 1]).expect("colcolor atom"),
                    &fam,
                    &format!("col {j} has exactly {third_h} cells of colour {k}"),
                )
                .expect("guard");
            b.sum_eq(g, lits, third_h).expect("colcolor sum_eq");
        }
    }

    // No three consecutive identical colours: rows then columns.
    let rowmatch = b
        .con_bool_matrix("rowmatch", con_dims(h, w - 2))
        .expect("rowmatch con");
    for i in 1..=h {
        for j in 1..=w - 2 {
            let cells = vec![
                grid.cell(vec![i, j]).expect("c0"),
                grid.cell(vec![i, j + 1]).expect("c1"),
                grid.cell(vec![i, j + 2]).expect("c2"),
            ];
            let g = b
                .guard(
                    &rowmatch.get(vec![i, j]).expect("rowmatch atom"),
                    "rowmatch",
                    &format!("row {i} has no three identical colours from column {j}"),
                )
                .expect("guard");
            b.table(g, cells, three_same_forbidden())
                .expect("rowmatch table");
        }
    }
    let colmatch = b
        .con_bool_matrix("colmatch", con_dims(w, h - 2))
        .expect("colmatch con");
    for j in 1..=w {
        for i in 1..=h - 2 {
            let cells = vec![
                grid.cell(vec![i, j]).expect("c0"),
                grid.cell(vec![i + 1, j]).expect("c1"),
                grid.cell(vec![i + 2, j]).expect("c2"),
            ];
            let g = b
                .guard(
                    &colmatch.get(vec![j, i]).expect("colmatch atom"),
                    "colmatch",
                    &format!("col {j} has no three identical colours from row {i}"),
                )
                .expect("guard");
            b.table(g, cells, three_same_forbidden())
                .expect("colmatch table");
        }
    }

    // mod / sequence rule: rows then columns.
    let rowseq = b
        .con_bool_matrix("rowseq", con_dims(h, w - 1))
        .expect("rowseq con");
    for i in 1..=h {
        for j in 1..=w - 1 {
            let cells = vec![
                grid.cell(vec![i, j]).expect("c0"),
                grid.cell(vec![i, j + 1]).expect("c1"),
            ];
            let g = b
                .guard(
                    &rowseq.get(vec![i, j]).expect("rowseq atom"),
                    "rowseq",
                    &format!("row {i} colour at column {j} stays or advances by one"),
                )
                .expect("guard");
            b.table(g, cells, mod_forbidden()).expect("rowseq table");
        }
    }
    let colseq = b
        .con_bool_matrix("colseq", con_dims(w, h - 1))
        .expect("colseq con");
    for j in 1..=w {
        for i in 1..=h - 1 {
            let cells = vec![
                grid.cell(vec![i, j]).expect("c0"),
                grid.cell(vec![i + 1, j]).expect("c1"),
            ];
            let g = b
                .guard(
                    &colseq.get(vec![j, i]).expect("colseq atom"),
                    "colseq",
                    &format!("col {j} colour at row {i} stays or advances by one"),
                )
                .expect("guard");
            b.table(g, cells, mod_forbidden()).expect("colseq table");
        }
    }

    // Givens.
    for &(r, c, v) in givens {
        let lit = vec![
            grid.cell(vec![r, c])
                .expect("given cell")
                .eq(v)
                .expect("given eq"),
        ];
        b.sum_eq_unguarded(lit, 1).expect("pin given");
    }

    b.build().expect("build binairo-tricolor")
}

#[wasm_bindgen_test]
fn planner_deduces_binairo_tricolor_3x3() {
    let puzzle = build_binairo_tricolor(3, 3, &[(1, 1, 0)]);
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");
    let _ = planner.quick_solve().expect("quick_solve");
    assert!(
        planner.is_solved(),
        "binairo-tricolor 3x3 should solve fully"
    );

    let state: serde_json::Value =
        from_value(planner.current_state().expect("current_state")).expect("decode state");
    let grid = state
        .get("grid")
        .unwrap_or_else(|| panic!("state missing 'grid' field: {state}"));

    for i in 1..=3_i64 {
        for j in 1..=3_i64 {
            let want = (i - 1 + j - 1).rem_euclid(3);
            let got = grid
                .get(i.to_string())
                .and_then(|row| row.get(j.to_string()))
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| panic!("grid[{i},{j}] missing in state: {state}"));
            assert_eq!(got, want, "grid[{i},{j}]: expected {want}, got {got}");
        }
    }
}
