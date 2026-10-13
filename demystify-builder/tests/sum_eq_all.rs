//! `sum_eq_all` posts a conjunction of sums as one `$#CON`.  Exercised on the
//! clue it was added for: Bloomsweeper's twilight clue, an unlabelled
//! multiset `{x, y}` of herb counts over some cells, written with an
//! auxiliary atom `t` choosing which herb has which count:
//!
//!   n1 + (y - x)·t = y  and  n2 + (y - x)·¬t = y
//!
//! Every deduction demystify makes must match brute force, for every clue
//! and every way of pinning two of the cells.

// `dims` is a slice of ranges; a one-dimensional matrix has exactly one.
#![allow(clippy::single_range_in_vec_init)]

use std::collections::BTreeMap;
use std::sync::Arc;

use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify_builder::{PuzzleBuilder, PuzzleParse, ShowRole, Signed};

const N: usize = 4;

fn build(x: i64, y: i64, pins: &[(usize, i64)]) -> PuzzleParse {
    assert!(x <= y);
    let mut b = PuzzleBuilder::new();
    b.kind("sum-eq-all-twilight");
    let cell = b.var_int_matrix("cell", &[0..=N as i64 - 1], &[0, 1, 2]);
    b.show("cell", ShowRole::Main);
    let clue = b.con_bool("clue");
    for &(i, v) in pins {
        b.sum_eq_unguarded(&[cell.cell(&[i as i64]).eq(v)], 1)
            .unwrap();
    }
    let n = |h: i64| -> Vec<Signed> { (0..N as i64).map(|i| cell.cell(&[i]).eq(h)).collect() };
    let g = b.guard(clue, "clue", "the multiset clue").unwrap();
    if x == y {
        b.sum_eq_all(g, &[(&n(1), x), (&n(2), x)]).unwrap();
    } else {
        let t = b.aux_bool("t");
        let mut one = n(1);
        let mut two = n(2);
        for _ in x..y {
            one.push(t.pos());
            two.push(t.neg());
        }
        b.sum_eq_all(g, &[(&one, y), (&two, y)]).unwrap();
    }
    b.build().unwrap()
}

/// Cells (outside `pins`) holding one value in every assignment that meets
/// the clue and the pins; `None` when no assignment does.
fn forced(x: i64, y: i64, pins: &[(usize, i64)]) -> Option<BTreeMap<usize, i64>> {
    let mut seen: Vec<Vec<i64>> = vec![Vec::new(); N];
    for code in 0..3_i64.pow(N as u32) {
        let vals: Vec<i64> = (0..N).map(|i| code / 3_i64.pow(i as u32) % 3).collect();
        if pins.iter().any(|&(i, v)| vals[i] != v) {
            continue;
        }
        let mut counts = [
            vals.iter().filter(|&&v| v == 1).count() as i64,
            vals.iter().filter(|&&v| v == 2).count() as i64,
        ];
        counts.sort_unstable();
        if counts != [x, y] {
            continue;
        }
        for (i, &v) in vals.iter().enumerate() {
            if !seen[i].contains(&v) {
                seen[i].push(v);
            }
        }
    }
    if seen[0].is_empty() {
        return None;
    }
    Some(
        (0..N)
            .filter(|i| !pins.iter().any(|&(p, _)| p == *i) && seen[*i].len() == 1)
            .map(|i| (i, seen[i][0]))
            .collect(),
    )
}

fn deduced(puzzle: PuzzleParse) -> BTreeMap<usize, i64> {
    let solver = PuzzleSolver::new(Arc::new(puzzle)).unwrap();
    let mut planner = PuzzlePlanner::new(solver);
    let mut out = BTreeMap::new();
    for step in planner.quick_solve() {
        for mus in &step {
            assert!(
                mus.constraints.len() <= 1,
                "one clue, but a step cites {:?}",
                mus.constraints
            );
            for pl in &mus.lits {
                if pl.var().name() == "cell" && pl.sign() {
                    let i = pl.var().indices()[0] as usize;
                    assert!(out.insert(i, pl.val()).is_none(), "cell {i} deduced twice");
                }
            }
        }
    }
    out
}

#[test]
fn twilight_clue_deduces_exactly_what_brute_force_forces() {
    let mut checked = 0;
    for y in 0..=N as i64 {
        for x in 0..=y.min(N as i64 - y) {
            let mut pinnings: Vec<Vec<(usize, i64)>> = vec![vec![]];
            for a in 0..3 {
                pinnings.push(vec![(0, a)]);
                for b in 0..3 {
                    pinnings.push(vec![(0, a), (1, b)]);
                }
            }
            for pins in pinnings {
                let Some(want) = forced(x, y, &pins) else {
                    continue;
                };
                let got = deduced(build(x, y, &pins));
                // Pinned cells are givens, not deductions.
                let got: BTreeMap<usize, i64> = got
                    .into_iter()
                    .filter(|(i, _)| !pins.iter().any(|&(p, _)| p == *i))
                    .collect();
                assert_eq!(got, want, "clue {{{x},{y}}} with pins {pins:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 50, "only {checked} cases checked");
}

#[test]
fn sum_eq_all_rejects_no_parts() {
    let mut b = PuzzleBuilder::new();
    let _cell = b.var_int_matrix("cell", &[0..=0], &[0, 1]);
    let clue = b.con_bool("clue");
    let g = b.guard(clue, "clue", "empty").unwrap();
    assert!(b.sum_eq_all(g, &[]).is_err());
}
