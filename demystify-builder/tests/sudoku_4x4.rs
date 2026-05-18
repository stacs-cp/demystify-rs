//! 4×4 sudoku encoded as one-hot booleans: `cell[r,c,v]` is true iff
//! cell (r,c) contains value v.  The cell-exactly-one-value rule is an
//! unguarded `sum_eq_unguarded == 1`; the row / col / box rules are
//! guarded `sum_eq == 1` ($#CON entries).  Givens use
//! `sum_eq_unguarded` over a single positive literal; "this cell isn't
//! value k" exclusions use `sum_eq_unguarded` over a single negated
//! literal, exercising the negation path through cardinality encoding.
//!
//! Reference solution (the test verifies the puzzle solves uniquely to
//! this):
//!
//! ```text
//!   1 2 3 4
//!   3 4 1 2
//!   2 1 4 3
//!   4 3 2 1
//! ```

use std::sync::Arc;

use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify_builder::{PuzzleBuilder, PuzzleParse};

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

/// (r, c, v) we declare to be impossible.  These are intentionally
/// values that the unique solution doesn't place at (r,c) anyway, so
/// the exclusions are consistent with the puzzle and exercise the
/// "single cell negated = 1" code path.
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

fn build_sudoku_4x4() -> PuzzleParse {
    let mut b = PuzzleBuilder::new();
    b.kind("sudoku-4x4");

    // cell[r,c,v]: true iff cell (r,c) contains value v.
    let cell = b.var_bool_matrix("cell", &[1..=N, 1..=N, 1..=N]);
    // No $#SHOW directive: cell is a 3-D one-hot encoding, not what
    // demystify's grid renderer expects.  Solving works fine without it.

    // 1. Each cell holds exactly one value — structural / unguarded.
    for r in 1..=N {
        for c in 1..=N {
            let lits: Vec<_> = (1..=N).map(|v| cell.get(&[r, c, v]).pos()).collect();
            b.sum_eq_unguarded(&lits, 1).unwrap();
        }
    }

    // 2. Row uniqueness — each value appears exactly once per row.
    let row_uniq = b.con_bool_matrix("row_uniq", &[1..=N, 1..=N]);
    for r in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = (1..=N).map(|c| cell.get(&[r, c, v]).pos()).collect();
            let g = b
                .guard(
                    row_uniq.get(&[r, v]),
                    "row_uniq",
                    format!("value {v} appears exactly once in row {r}"),
                )
                .unwrap();
            b.sum_eq(g, &lits, 1).unwrap();
        }
    }

    // 3. Column uniqueness.
    let col_uniq = b.con_bool_matrix("col_uniq", &[1..=N, 1..=N]);
    for c in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = (1..=N).map(|r| cell.get(&[r, c, v]).pos()).collect();
            let g = b
                .guard(
                    col_uniq.get(&[c, v]),
                    "col_uniq",
                    format!("value {v} appears exactly once in column {c}"),
                )
                .unwrap();
            b.sum_eq(g, &lits, 1).unwrap();
        }
    }

    // 4. Box uniqueness.
    let box_uniq = b.con_bool_matrix("box_uniq", &[1..=N, 1..=N]);
    for b_idx in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = cells_in_box(b_idx)
                .iter()
                .map(|&(r, c)| cell.get(&[r, c, v]).pos())
                .collect();
            let g = b
                .guard(
                    box_uniq.get(&[b_idx, v]),
                    "box_uniq",
                    format!("value {v} appears exactly once in box {b_idx}"),
                )
                .unwrap();
            b.sum_eq(g, &lits, 1).unwrap();
        }
    }

    // 5. Givens — "this cell holds this value" via single-positive sum_eq.
    for &(r, c, v) in GIVENS {
        b.sum_eq_unguarded(&[cell.get(&[r, c, v]).pos()], 1)
            .unwrap();
    }

    // 6. Exclusions — "this cell doesn't hold this value" via
    // single-negated sum_eq, exercising the Signed::neg() path.
    for &(r, c, v) in EXCLUSIONS {
        b.sum_eq_unguarded(&[cell.get(&[r, c, v]).neg()], 1)
            .unwrap();
    }

    b.build().unwrap()
}

/// Helper: extract the deduced (r,c,v) one-hot positives.
fn collect_solution(
    puzzle: &PuzzleParse,
    lits: &std::collections::BTreeSet<rustsat::types::Lit>,
) -> [[i64; 4]; 4] {
    let mut grid = [[0_i64; 4]; 4];
    for lit in lits {
        if let Some(puzlits) = puzzle.direct.invlitmap.get(lit) {
            for pl in puzlits {
                if pl.var().name() == "cell" && pl.sign() && pl.val() == 1 {
                    let var = pl.var();
                    let idx = var.indices();
                    let (r, c, v) = (idx[0] as usize - 1, idx[1] as usize - 1, idx[2]);
                    grid[r][c] = v;
                }
            }
        }
    }
    grid
}

#[test]
fn sudoku_4x4_solves_to_reference() {
    let puzzle = Arc::new(build_sudoku_4x4());
    let mut solver = PuzzleSolver::new(puzzle.clone()).unwrap();

    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    let mut rng = ChaCha20Rng::seed_from_u64(0xACE);
    let assignment = solver
        .random_solution(&mut rng, None)
        .expect("sudoku is satisfiable");
    let grid = collect_solution(&puzzle, &assignment);

    // Check well-formedness of one-hot decoding.
    for (r, row) in grid.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            assert!(
                (1..=4).contains(&v),
                "cell ({},{}) decoded to {}",
                r + 1,
                c + 1,
                v
            );
        }
    }

    // Check rows, cols, boxes contain {1,2,3,4}.
    for (r, row_vals) in grid.iter().enumerate() {
        let mut row: Vec<i64> = row_vals.to_vec();
        row.sort();
        assert_eq!(row, vec![1, 2, 3, 4], "row {} not a permutation", r + 1);
    }
    for c in 0..4_usize {
        let mut col: Vec<i64> = grid.iter().map(|row| row[c]).collect();
        col.sort();
        assert_eq!(col, vec![1, 2, 3, 4], "col {} not a permutation", c + 1);
    }
    for b in 1..=4 {
        let mut vs: Vec<i64> = cells_in_box(b)
            .iter()
            .map(|&(r, c)| grid[(r - 1) as usize][(c - 1) as usize])
            .collect();
        vs.sort();
        assert_eq!(vs, vec![1, 2, 3, 4], "box {b} not a permutation");
    }

    // Givens respected.
    for &(r, c, v) in GIVENS {
        assert_eq!(
            grid[(r - 1) as usize][(c - 1) as usize],
            v,
            "given ({r},{c}) = {v} not respected"
        );
    }
    // Exclusions respected.
    for &(r, c, v) in EXCLUSIONS {
        assert_ne!(
            grid[(r - 1) as usize][(c - 1) as usize],
            v,
            "exclusion ({r},{c}) != {v} violated"
        );
    }

    // The givens + exclusions force a unique solution; verify it matches.
    assert_eq!(
        grid, SOLUTION,
        "puzzle solved to a different valid solution than the reference"
    );
}

#[test]
fn demystify_deduces_full_sudoku_4x4() {
    let puzzle = Arc::new(build_sudoku_4x4());
    let solver = PuzzleSolver::new(puzzle.clone()).unwrap();
    let mut planner = PuzzlePlanner::new(solver);

    let _steps = planner.quick_solve();

    // `quick_solve` may skip MUSes ≤ skip_small_threshold from the
    // user-visible step list (default 0 — empty MUSes from unit-clause
    // pinning), but those literals are still marked as known internally.
    // Inspect the planner's known-lit set directly.
    let known = planner.get_all_known_lits().clone();
    let mut deduced_grid = [[0_i64; 4]; 4];
    for lit in &known {
        if let Some(puzlits) = puzzle.direct.invlitmap.get(lit) {
            for pl in puzlits {
                if pl.var().name() == "cell" && pl.sign() && pl.val() == 1 {
                    let var = pl.var();
                    let idx = var.indices();
                    let (r, c, v) = (idx[0] as usize - 1, idx[1] as usize - 1, idx[2]);
                    deduced_grid[r][c] = v;
                }
            }
        }
    }

    assert_eq!(
        deduced_grid, SOLUTION,
        "demystify did not deduce the unique sudoku solution"
    );
}
