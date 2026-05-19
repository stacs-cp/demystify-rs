//! Reproducer for the empty-`difficulties()` bug reported by the
//! bloomsweep-web embedder.  Mirrors `scripts/difficulties-repro/`:
//! a 4×4 minesweeper-shaped puzzle (the "bloomsweeper Tutorial" model)
//! with sum-checks gated by `andAtom(sumcheck, facts)` plus row + column
//! cardinality `$#CON`s.
//!
//! In the browser, `WasmPlanner.difficulties()` returns `{}` on this
//! puzzle even though `bestStep()` finds size-1 MUSes.  The Rust control
//! test below should *also* fail if the bug is reachable on the native
//! side; if it passes, the bug is wasm-specific (rayon's threadless
//! par_bridge, batsat-on-wasm32, …).

use std::sync::Arc;

use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify::satcore::{SolverBackend, set_solver_backend};
use demystify_builder::{PuzzleBuilder, PuzzleParse, ShowRole};

const H: i64 = 4;
const W: i64 = 4;
const M: i64 = 2;

#[rustfmt::skip]
const HERBS: [[i64; 4]; 4] = [
    [1, 2, 1, 1],
    [2, 2, 1, 2],
    [1, 0, 2, 0],
    [2, 1, 1, 0],
];

#[rustfmt::skip]
const KNOWN: [[i64; 4]; 4] = [
    [0, 1, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
];

fn build_puzzle() -> PuzzleParse {
    let mut b = PuzzleBuilder::new();
    b.kind("bloomsweeper-tutorial-repro");

    let cell = b.var_bool_matrix("cell", &[0..=H - 1, 0..=W - 1, 0..=M]);
    b.show("cell", ShowRole::Main);

    let facts = b.reveal_bool_matrix("facts", &[0..=H - 1, 0..=W - 1, 0..=M, 0..=1]);
    b.set_reveal("cell", "facts").unwrap();

    let sumcheck = b.con_bool_matrix("sumcheck", &[0..=H - 1, 0..=W - 1, 0..=M]);
    let rowcheck = b.con_bool_matrix("rowcheck", &[0..=H - 1, 1..=M]);
    let colcheck = b.con_bool_matrix("colcheck", &[0..=W - 1, 1..=M]);

    for r in 0..H {
        for c in 0..W {
            let mut lits = Vec::new();
            for h in 0..=M {
                lits.push(cell.get(&[r, c, h]).pos());
            }
            b.sum_eq_unguarded(&lits, 1).unwrap();
        }
    }

    for r in 0..H {
        for c in 0..W {
            if KNOWN[r as usize][c as usize] == 1 {
                let h = HERBS[r as usize][c as usize];
                b.sum_eq_unguarded(&[cell.get(&[r, c, h]).pos()], 1)
                    .unwrap();
            }
        }
    }

    for r in 0..H {
        for c in 0..W {
            if HERBS[r as usize][c as usize] != 0 {
                continue;
            }
            let mut nbrs = Vec::new();
            for dr in -1..=1_i64 {
                for dc in -1..=1_i64 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let (nr, nc) = (r + dr, c + dc);
                    if (0..H).contains(&nr) && (0..W).contains(&nc) {
                        nbrs.push((nr, nc));
                    }
                }
            }
            for h in 1..=M {
                let mut count = 0_i64;
                let mut atoms = Vec::new();
                for &(nr, nc) in &nbrs {
                    if HERBS[nr as usize][nc as usize] == h {
                        count += 1;
                    }
                    atoms.push(cell.get(&[nr, nc, h]).pos());
                }
                let gate = b.and_atom(&[
                    sumcheck.get(&[r, c, h]).pos(),
                    facts.get(&[r, c, 0, 1]).pos(),
                ]);
                let g = b
                    .guard(gate, "sumcheck", format!("sumcheck[{r},{c}] herb={h}"))
                    .unwrap();
                b.sum_eq(g, &atoms, count).unwrap();
            }
        }
    }

    for r in 0..H {
        for h in 1..=M {
            let mut count = 0_i64;
            let mut atoms = Vec::new();
            for c in 0..W {
                if HERBS[r as usize][c as usize] == h {
                    count += 1;
                }
                atoms.push(cell.get(&[r, c, h]).pos());
            }
            let g = b
                .guard(
                    rowcheck.get(&[r, h]),
                    "rowcheck",
                    format!("rowcheck[{r}] herb={h}"),
                )
                .unwrap();
            b.sum_eq(g, &atoms, count).unwrap();
        }
    }

    for c in 0..W {
        for h in 1..=M {
            let mut count = 0_i64;
            let mut atoms = Vec::new();
            for r in 0..H {
                if HERBS[r as usize][c as usize] == h {
                    count += 1;
                }
                atoms.push(cell.get(&[r, c, h]).pos());
            }
            let g = b
                .guard(
                    colcheck.get(&[c, h]),
                    "colcheck",
                    format!("colcheck[{c}] herb={h}"),
                )
                .unwrap();
            b.sum_eq(g, &atoms, count).unwrap();
        }
    }

    b.build().unwrap()
}

#[test]
fn bloomsweeper_tutorial_difficulties_populated() {
    set_solver_backend(SolverBackend::BatSat);
    // Force rayon to a single thread so par_bridge runs sequentially —
    // matching the wasm runtime, which has no thread pool.  Building the
    // global pool is one-shot; tolerate the case where another test in
    // this binary already configured it.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global();

    let puzzle = Arc::new(build_puzzle());
    let mut solver = PuzzleSolver::new(puzzle.clone()).unwrap();
    let provable = solver.get_provable_varlits().clone();
    eprintln!("provable_varlits: {}", provable.len());

    let mut planner = PuzzlePlanner::new(solver);
    let dict = planner.all_muses_with_larger();
    eprintln!("all_muses_with_larger -> {} entries", dict.muses().len());

    let muses = planner.smallest_muses_with_config();
    eprintln!(
        "smallest_muses_with_config -> {} muses, sizes={:?}",
        muses.len(),
        muses.iter().map(|m| m.mus_len()).collect::<Vec<_>>()
    );

    assert!(
        !dict.muses().is_empty(),
        "all_muses_with_larger returned an empty dict, but smallest_muses \
         found {} muses with sizes {:?}",
        muses.len(),
        muses.iter().map(|m| m.mus_len()).collect::<Vec<_>>(),
    );
}
