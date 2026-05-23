//! Three-colour binairo (Takuzu) built through the builder — the test bed
//! for multi-valued `var_int_matrix` cells and the `table` constraint.
//!
//! Rules (per `ms/examples/binairo-tricolor-rect.eprime`):
//!   - each row has exactly `w/3` of each colour; each column `h/3`;
//!   - no three consecutive identical colours in a row or column;
//!   - `mod`/sequence: along every row and column the next cell stays the
//!     same or advances by one mod 3, i.e. the transitions 0→2, 1→0, 2→1
//!     are forbidden.
//!
//! Balance is a cardinality (`sum_eq`); "no three in a row" and the
//! `mod` rule are `table`s with a small forbidden set — one `$#CON` per
//! rule instance.
//!
//! Test instance: a 3×3 grid with `grid[1,1] = 0` pinned.  At n=3 each row
//! and column holds one of each colour, so the `mod` rule forces every
//! consecutive pair to advance — the grid is the addition table
//! `grid[i,j] = (i-1)+(j-1) mod 3`, uniquely fixed by the single given.

use std::collections::BTreeMap;
use std::sync::Arc;

use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify_builder::{IntCell, Lit, PuzzleBuilder, PuzzleParse, ShowRole};

const COLOURS: [i64; 3] = [0, 1, 2];

/// Forbidden transitions for the `mod` rule: 0→2, 1→0, 2→1.
fn mod_forbidden() -> Vec<Vec<i64>> {
    vec![vec![0, 2], vec![1, 0], vec![2, 1]]
}

/// Forbidden triples for "no three identical in a row": all-same.
fn three_same_forbidden() -> Vec<Vec<i64>> {
    COLOURS.iter().map(|&k| vec![k, k, k]).collect()
}

/// Build an `h`×`w` three-colour binairo with the given pre-filled cells
/// (`(row, col, value)`, all 1-indexed).  `h` and `w` must be multiples of 3.
fn build_binairo_tricolor(h: i64, w: i64, givens: &[(i64, i64, i64)]) -> PuzzleParse {
    assert!(h % 3 == 0 && w % 3 == 0, "grid dims must be multiples of 3");
    let third_w = w / 3;
    let third_h = h / 3;

    let mut b = PuzzleBuilder::new();
    b.kind("binairo-tricolor");

    let grid = b.var_int_matrix("grid", &[1..=h, 1..=w], &COLOURS);
    b.show("grid", ShowRole::Main);

    // ── Row colour balance: row i has exactly w/3 cells of each colour. ──
    let rowcolor: Vec<_> = COLOURS
        .iter()
        .map(|&k| b.con_bool_matrix(&format!("rowcolor{k}"), &[1..=h]))
        .collect();
    for &k in &COLOURS {
        for i in 1..=h {
            let g = b
                .guard(
                    rowcolor[k as usize].get(&[i]),
                    &format!("rowcolor{k}"),
                    format!("row {i} has exactly {third_w} cells of colour {k}"),
                )
                .unwrap();
            let lits: Vec<_> = (1..=w).map(|j| grid.cell(&[i, j]).eq(k)).collect();
            b.sum_eq(g, &lits, third_w).unwrap();
        }
    }

    // ── Column colour balance: col j has exactly h/3 cells of each colour. ──
    let colcolor: Vec<_> = COLOURS
        .iter()
        .map(|&k| b.con_bool_matrix(&format!("colcolor{k}"), &[1..=w]))
        .collect();
    for &k in &COLOURS {
        for j in 1..=w {
            let g = b
                .guard(
                    colcolor[k as usize].get(&[j]),
                    &format!("colcolor{k}"),
                    format!("col {j} has exactly {third_h} cells of colour {k}"),
                )
                .unwrap();
            let lits: Vec<_> = (1..=h).map(|i| grid.cell(&[i, j]).eq(k)).collect();
            b.sum_eq(g, &lits, third_h).unwrap();
        }
    }

    // ── No three consecutive identical colours, in rows then columns. ──
    let rowmatch = b.con_bool_matrix("rowmatch", &[1..=h, 1..=w - 2]);
    for i in 1..=h {
        for j in 1..=w - 2 {
            let g = b
                .guard(
                    rowmatch.get(&[i, j]),
                    "rowmatch",
                    format!("row {i} has no three identical colours from column {j}"),
                )
                .unwrap();
            let cells: Vec<IntCell> = (0..3).map(|o| grid.cell(&[i, j + o])).collect();
            b.table(g, &cells, &three_same_forbidden()).unwrap();
        }
    }
    let colmatch = b.con_bool_matrix("colmatch", &[1..=w, 1..=h - 2]);
    for j in 1..=w {
        for i in 1..=h - 2 {
            let g = b
                .guard(
                    colmatch.get(&[j, i]),
                    "colmatch",
                    format!("col {j} has no three identical colours from row {i}"),
                )
                .unwrap();
            let cells: Vec<IntCell> = (0..3).map(|o| grid.cell(&[i + o, j])).collect();
            b.table(g, &cells, &three_same_forbidden()).unwrap();
        }
    }

    // ── mod / sequence rule, in rows then columns. ──
    let rowseq = b.con_bool_matrix("rowseq", &[1..=h, 1..=w - 1]);
    for i in 1..=h {
        for j in 1..=w - 1 {
            let g = b
                .guard(
                    rowseq.get(&[i, j]),
                    "rowseq",
                    format!("row {i} colour at column {j} stays or advances by one"),
                )
                .unwrap();
            let cells = [grid.cell(&[i, j]), grid.cell(&[i, j + 1])];
            b.table(g, &cells, &mod_forbidden()).unwrap();
        }
    }
    let colseq = b.con_bool_matrix("colseq", &[1..=w, 1..=h - 1]);
    for j in 1..=w {
        for i in 1..=h - 1 {
            let g = b
                .guard(
                    colseq.get(&[j, i]),
                    "colseq",
                    format!("col {j} colour at row {i} stays or advances by one"),
                )
                .unwrap();
            let cells = [grid.cell(&[i, j]), grid.cell(&[i + 1, j])];
            b.table(g, &cells, &mod_forbidden()).unwrap();
        }
    }

    // ── Givens: pin the pre-filled cells. ──
    for &(r, c, v) in givens {
        b.sum_eq_unguarded(&[grid.cell(&[r, c]).eq(v)], 1).unwrap();
    }

    b.build().unwrap()
}

/// Solve fully, then read back `grid[i,j] = v` for every cell from the
/// planner's known lits — the same positive-assignment view the Lua/wasm
/// `current_state` exposes.
fn solve_to_grid(puzzle: PuzzleParse) -> BTreeMap<(i64, i64), i64> {
    let puzzle = Arc::new(puzzle);
    let solver = PuzzleSolver::new(puzzle).unwrap();
    let mut planner = PuzzlePlanner::new(solver);
    let steps = planner.quick_solve();
    assert!(!steps.is_empty(), "expected at least one solve step");
    let unsolved = planner.unsolved_vars_after_solve();
    assert!(
        unsolved.is_empty(),
        "puzzle not fully solved; unsolved: {unsolved:?}"
    );

    let known: Vec<Lit> = planner.get_all_known_lits().clone();
    let mut grid = BTreeMap::new();
    for lit in &known {
        for pl in planner.solver().lit_to_puzlit(lit) {
            if pl.var().name() == "grid" && pl.sign() {
                let var = pl.var();
                let idx = var.indices();
                grid.insert((idx[0], idx[1]), pl.val());
            }
        }
    }
    grid
}

#[test]
fn deduces_3x3_addition_table_from_one_given() {
    let puzzle = build_binairo_tricolor(3, 3, &[(1, 1, 0)]);
    let grid = solve_to_grid(puzzle);

    // The unique solution is the addition table fixed by grid[1,1] = 0.
    for i in 1..=3_i64 {
        for j in 1..=3_i64 {
            let want = ((i - 1) + (j - 1)).rem_euclid(3);
            let got = grid
                .get(&(i, j))
                .copied()
                .unwrap_or_else(|| panic!("no value deduced for grid[{i},{j}] (got: {grid:?})"));
            assert_eq!(got, want, "grid[{i},{j}]: expected {want}, got {got}");
        }
    }
}

#[test]
fn table_rejects_tuple_with_wrong_arity() {
    let mut b = PuzzleBuilder::new();
    let grid = b.var_int_matrix("grid", &[1..=2], &COLOURS);
    let rule = b.con_bool("rule");
    let g = b.guard(rule, "rule", "two-cell table").unwrap();
    // Two columns but a 3-value tuple.
    let err = b
        .table(g, &[grid.cell(&[1]), grid.cell(&[2])], &[vec![0, 1, 2]])
        .unwrap_err();
    assert!(matches!(
        err,
        demystify_builder::BuildError::TableArityMismatch {
            expected: 2,
            got: 3
        }
    ));
}

#[test]
fn table_rejects_value_outside_domain() {
    let mut b = PuzzleBuilder::new();
    let grid = b.var_int_matrix("grid", &[1..=2], &COLOURS);
    let rule = b.con_bool("rule");
    let g = b.guard(rule, "rule", "value out of domain").unwrap();
    let err = b
        .table(g, &[grid.cell(&[1]), grid.cell(&[2])], &[vec![0, 5]])
        .unwrap_err();
    assert!(matches!(
        err,
        demystify_builder::BuildError::TableValueNotInDomain { value: 5, .. }
    ));
}
