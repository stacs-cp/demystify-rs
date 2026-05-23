//! Minesweeper exercising the `$#REVEAL` cascade properly.
//!
//! The constraint we want, per eprime/minesweeper.eprime:
//!
//! ```text
//! sumcheck[i, j] /\ facts[i, j, 0]
//!     -> ( sum(grid[neighbours of (i,j)]) = clue )
//! ```
//!
//! `facts` is the reveal target of `grid`: it is *unconstrained in the
//! CNF*.  The only way `facts[i, j, d]` becomes known is via the planner's
//! reveal cascade after `grid[i, j] = d` is deduced.  So the constraint
//! can only fire after the player's revealed-cell value cascades through
//! REVEAL — which is precisely what makes this a real test of the
//! mechanism.
//!
//! The constraint is registered into the `sumcheck` family via
//! [`PuzzleBuilder::guard`], with `facts[r,c,0]` attached as an extra
//! gate via [`Guard::gated_by`].  (The earlier and_atom-based encoding
//! was bidirectional, so assuming `c=true` forced `facts=true` and
//! silently bypassed the reveal cascade.)
//!
//! Same 3×3 layout as `minesweeper.rs`:
//!
//! ```text
//!   0 0 .
//!   0 1 .
//!   . . .
//! ```

use std::sync::Arc;

use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify_builder::{PuzzleBuilder, PuzzleParse, ShowRole};

fn build_minesweeper_3x3_with_reveal() -> PuzzleParse {
    let n: i64 = 3;
    let mut b = PuzzleBuilder::new();
    b.kind("minesweeper-reveal");

    let grid = b.var_bool_matrix("grid", &[1..=n, 1..=n]);
    b.show("grid", ShowRole::Main);

    // facts is the reveal target — unconstrained in CNF; only becomes
    // known via the planner's reveal cascade.
    let facts = b.reveal_bool_matrix("facts", &[1..=n, 1..=n, 0..=1]);
    b.set_reveal("grid", "facts").unwrap();

    let sumcheck = b.con_bool_matrix("sumcheck", &[1..=n, 1..=n]);

    let clues: &[(i64, i64, i64)] = &[(1, 1, 0), (1, 2, 0), (2, 1, 0), (2, 2, 1)];

    // Pin revealed cells to "not a mine".  This is what triggers the
    // reveal cascade: `grid[r, c] = 0` makes `facts[r, c, 0]` known.
    for &(r, c, _) in clues {
        b.sum_eq_unguarded(&[grid.get(&[r, c]).neg()], 1).unwrap();
    }

    // Per-clue neighbour-count constraint, gated by sumcheck ∧ facts.
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
        let g = b
            .guard(
                sumcheck.get(&[r, c]),
                "sumcheck",
                format!("exactly {n_mines} mines around ({r}, {c}) given safe"),
            )
            .unwrap()
            .gated_by(&facts.get(&[r, c, 0]));
        b.sum_eq(g, &neighbours, n_mines).unwrap();
    }

    b.build().unwrap()
}

#[test]
fn demystify_deduces_minesweeper_3x3_via_reveal() {
    let puzzle = Arc::new(build_minesweeper_3x3_with_reveal());

    // Sanity: reveal_map has one entry per (grid lit, polarity) for each
    // of the 9 cells.
    assert_eq!(
        puzzle.reveal_map.len(),
        18,
        "reveal_map should cover both polarities of every grid cell"
    );

    let solver = PuzzleSolver::new(puzzle.clone()).unwrap();
    let mut planner = PuzzlePlanner::new(solver);
    let steps = planner.quick_solve();
    assert!(!steps.is_empty(), "expected at least one solve step");

    let mut deduced: std::collections::BTreeMap<(i64, i64), i64> =
        std::collections::BTreeMap::new();
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

#[test]
fn all_muses_with_larger_populates_on_minesweeper_reveal() {
    // Repro for an embedder-reported bug: difficulties() in the wasm
    // bindings returns empty on this puzzle, even though bestStep /
    // quickSolve solve it.  difficulties() is just a thin wrapper around
    // PuzzlePlanner::all_muses_with_larger(), so the bug must be visible
    // here too if it's in the planner / solver layer.
    let puzzle = Arc::new(build_minesweeper_3x3_with_reveal());
    let solver = PuzzleSolver::new(puzzle.clone()).unwrap();
    let mut planner = PuzzlePlanner::new(solver);

    let provable = {
        // get_provable_varlits requires &mut self; clone the result so
        // we don't hold the borrow across the all_muses_with_larger call.
        let mut tmp_solver = PuzzleSolver::new(puzzle.clone()).unwrap();
        tmp_solver.get_provable_varlits().clone()
    };
    assert!(
        !provable.is_empty(),
        "minesweeper-via-REVEAL should have provable lits before any deduction"
    );

    let dict = planner.all_muses_with_larger();
    let n_lits = dict.muses().len();
    assert!(
        n_lits > 0,
        "all_muses_with_larger returned an empty dict for a puzzle with \
         {} provable lits — difficulties() relies on this being populated",
        provable.len()
    );

    // Mirror what difficulties() does: confirm at least one entry has a
    // non-zero min MUS size (otherwise the JS-side BTreeMap would be
    // populated, but every entry would be 0 — still useless for "how
    // hard is this lit").
    let n_with_sized_mus = dict
        .muses()
        .values()
        .filter(|mus_set| mus_set.iter().any(|mc| mc.mus_len() >= 1))
        .count();
    assert!(
        n_with_sized_mus > 0,
        "every dict entry had only size-0 MUSes — difficulties() would \
         report 0s, which isn't useful",
    );
}

#[test]
fn set_reveal_rejects_unknown_source() {
    let mut b = PuzzleBuilder::new();
    let _facts = b.reveal_bool_matrix("facts", &[1..=2, 0..=1]);
    let err = b.set_reveal("nope", "facts").unwrap_err();
    assert!(matches!(
        err,
        demystify_builder::BuildError::UnknownRevealSource(name) if name == "nope"
    ));
}

#[test]
fn set_reveal_rejects_unknown_target() {
    let mut b = PuzzleBuilder::new();
    let _grid = b.var_bool_matrix("grid", &[1..=2]);
    let err = b.set_reveal("grid", "facts").unwrap_err();
    assert!(matches!(
        err,
        demystify_builder::BuildError::UnknownRevealTarget(name) if name == "facts"
    ));
}

#[test]
fn unused_reveal_target_rejected_at_build() {
    let mut b = PuzzleBuilder::new();
    let grid = b.var_bool_matrix("grid", &[1..=2]);
    let _facts = b.reveal_bool_matrix("facts", &[1..=2, 0..=1]);
    let rule = b.con_bool("rule");
    let g = b.guard(rule, "rule", "force grid[1] true").unwrap();
    b.sum_ge(g, &[grid.get(&[1]).pos()], 1).unwrap();
    // No set_reveal — the build should reject the dangling target.
    let err = b.build().unwrap_err();
    assert!(matches!(
        err,
        demystify_builder::BuildError::UnusedRevealTarget(name) if name == "facts"
    ));
}

#[test]
fn and_atom_round_trips_for_a_two_input_gate() {
    // Sanity check: and_atom(a, b) is a fresh SAT lit that's true iff
    // both inputs are true.  We test by building a tiny puzzle where the
    // gate gates a forced literal, and check it solves the way we expect.
    use rustsat::solvers::{Solve, SolveIncremental, SolverResult};
    let mut b = PuzzleBuilder::new();
    let v = b.var_bool_matrix("v", &[1..=2]);
    let rule = b.con_bool("rule");
    let gate = b.and_atom(&[v.get(&[1]).pos(), v.get(&[2]).pos()]);
    let g = b.guard(rule, "rule", "v[1] under gate").unwrap();
    // rule -> v[1] = 1, but we also force the gate's value to be observed.
    // (The body of the sum doesn't reference gate; we're just testing the
    // and-atom's CNF directly.)
    b.sum_ge(g, &[v.get(&[1]).pos()], 1).unwrap();

    let puzzle = b.build().unwrap();
    let cnf = puzzle.cnf.as_ref().unwrap().as_ref().clone();
    // Under the assumption "gate = true", both v[1] and v[2] must hold.
    let mut solver = rustsat_batsat::BasicSolver::default();
    for cl in cnf.iter() {
        solver.add_clause(cl.clone()).unwrap();
    }
    let v1_lit = v.get(&[1]).lit();
    let v2_lit = v.get(&[2]).lit();
    assert_eq!(
        solver.solve_assumps(&[gate.lit(), !v1_lit]).unwrap(),
        SolverResult::Unsat,
        "gate=true must imply v[1]=true"
    );
    assert_eq!(
        solver.solve_assumps(&[gate.lit(), !v2_lit]).unwrap(),
        SolverResult::Unsat,
        "gate=true must imply v[2]=true"
    );
    // Under "gate = false", at least one of v[1] / v[2] must be false.
    assert_eq!(
        solver
            .solve_assumps(&[!gate.lit(), v1_lit, v2_lit])
            .unwrap(),
        SolverResult::Unsat,
        "gate=false must imply !(v[1] /\\ v[2])"
    );
}
