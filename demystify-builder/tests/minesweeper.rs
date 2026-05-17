//! Build a tiny deductive 3×3 minesweeper puzzle in Rust, run it
//! through demystify's MUS pipeline, and verify it deduces the mine and
//! all safe cells.
//!
//! Layout (`.` = unknown, digits = revealed safe cells with neighbour
//! mine counts):
//!
//! ```text
//!   0 0 .
//!   0 1 .
//!   . . .
//! ```
//!
//! Propagation:
//!   - (1,1)=0 → (1,2), (2,1), (2,2) safe.
//!   - (1,2)=0 → (1,3), (2,2), (2,3) safe.
//!   - (2,1)=0 → (2,2), (3,1), (3,2) safe.
//!   - (2,2)=1 → exactly 1 of {(1,1)…(3,3)\(2,2)} mine; everything
//!     except (3,3) is already safe ⇒ (3,3) is the mine.

use std::sync::Arc;

use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify_builder::{PuzzleBuilder, PuzzleParse, ShowRole};

fn build_minesweeper_3x3() -> PuzzleParse {
    let n: i64 = 3;
    let mut b = PuzzleBuilder::new();
    b.kind("minesweeper");

    // grid[i,j] = true iff (i,j) is a mine.
    let grid = b.var_bool_matrix("grid", &[1..=n, 1..=n]);
    b.show("grid", ShowRole::Main);

    let sumcheck = b.con_bool_matrix("sumcheck", &[1..=n, 1..=n]);

    // (row, col, clue) — revealed cells.
    let clues: &[(i64, i64, i64)] = &[(1, 1, 0), (1, 2, 0), (2, 1, 0), (2, 2, 1)];

    // Pin revealed cells to "not a mine" via raw unit clauses.
    for &(r, c, _) in clues {
        b.add_clause([!grid.get(&[r, c]).lit()]);
    }

    // For each revealed cell, post a guarded sum_eq over its on-board
    // neighbours equal to the clue value.
    for &(r, c, n_mines) in clues {
        let mut neighbours = Vec::new();
        for dr in -1..=1_i64 {
            for dc in -1..=1_i64 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let nr = r + dr;
                let nc = c + dc;
                if (1..=n).contains(&nr) && (1..=n).contains(&nc) {
                    neighbours.push(grid.get(&[nr, nc]).pos());
                }
            }
        }
        b.sum_eq(
            sumcheck.get(&[r, c]),
            &neighbours,
            n_mines,
            "sumcheck",
            format!("exactly {n_mines} mines around ({r}, {c})"),
        )
        .unwrap();
    }

    b.build().unwrap()
}

#[test]
fn demystify_deduces_minesweeper_3x3() {
    let puzzle = Arc::new(build_minesweeper_3x3());
    let solver = PuzzleSolver::new(puzzle.clone()).unwrap();
    let mut planner = PuzzlePlanner::new(solver);

    let steps = planner.quick_solve();
    assert!(!steps.is_empty(), "expected at least one solve step");

    // Collect all deduced `grid[i,j] = v` puzlits.
    use std::collections::BTreeMap;
    let mut deduced: BTreeMap<(i64, i64), i64> = BTreeMap::new();
    for step in &steps {
        for mus in step {
            for pl in &mus.lits {
                if pl.var().name() == "grid" && pl.sign() {
                    let var = pl.var();
                    let idx = var.indices();
                    deduced.insert((idx[0], idx[1]), pl.val());
                }
            }
        }
    }

    // Expected: (3,3) is a mine; the other five unrevealed cells are safe.
    let expected: &[((i64, i64), i64)] = &[
        ((1, 3), 0),
        ((2, 3), 0),
        ((3, 1), 0),
        ((3, 2), 0),
        ((3, 3), 1),
    ];
    for &(cell, want) in expected {
        let got = deduced
            .get(&cell)
            .copied()
            .unwrap_or_else(|| panic!("demystify did not deduce a value for cell {cell:?}"));
        assert_eq!(got, want, "cell {cell:?}: expected {want}, got {got}");
    }
}
