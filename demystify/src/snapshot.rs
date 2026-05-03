//! Mid-solve snapshot import/export.
//!
//! A `SessionSnapshot` captures a puzzle and the per-step diffs of deduced
//! literals so a partially-solved state can be persisted to JSON and replayed
//! later without re-running MUS search. The format is `puzzle + planner_config
//! + solver_config + steps[StepSnapshot{deduced_lits}]`.
//!
//! Used by the GUI (`demystify-web`) for "save / restore session" and by
//! corpus-scale analyses (in `mystify`) that need to revisit a specific
//! mid-solve state to enumerate alternative MUSes.

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result};
use rustsat::types::Lit;
use serde::{Deserialize, Serialize};

use crate::{
    named_strategy::Database,
    problem::{
        parse::PuzzleParse,
        planner::{PlannerConfig, PuzzlePlanner},
        serialize::SerializablePuzzleParse,
        solver::{PuzzleSolver, SolverConfig},
    },
};

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionSnapshot {
    pub puzzle: SerializablePuzzleParse,
    pub planner_config: PlannerConfig,
    pub solver_config: SolverConfig,
    pub steps: Vec<StepSnapshot>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StepSnapshot {
    pub deduced_lits: Vec<i32>,
}

/// Build a `SessionSnapshot` from a chronological list of planner snapshots.
///
/// `history[0]` is the initial state (after trivial-lit deduction); each
/// subsequent entry adds the lits deduced in that step. The first step in the
/// snapshot contains all initial known literals; later steps contain only the
/// diff against their predecessor.
pub fn export_snapshot(history: &[PuzzlePlanner]) -> Result<SessionSnapshot> {
    let first = history.first().context("history is empty")?;
    let puzzle = SerializablePuzzleParse::try_from(first.puzzle())?;
    let planner_config = first.planner_config();
    let solver_config = first.solver_config();

    let mut steps = Vec::with_capacity(history.len());

    let step0_lits: Vec<i32> = history[0]
        .get_all_known_lits()
        .iter()
        .map(|lit| lit.to_ipasir())
        .collect();
    steps.push(StepSnapshot {
        deduced_lits: step0_lits,
    });

    for i in 1..history.len() {
        let prev: BTreeSet<Lit> = history[i - 1]
            .get_all_known_lits()
            .iter()
            .copied()
            .collect();
        let deduced: Vec<i32> = history[i]
            .get_all_known_lits()
            .iter()
            .filter(|lit| !prev.contains(lit))
            .map(|lit| lit.to_ipasir())
            .collect();
        steps.push(StepSnapshot {
            deduced_lits: deduced,
        });
    }

    Ok(SessionSnapshot {
        puzzle,
        planner_config,
        solver_config,
        steps,
    })
}

/// Replay a snapshot into a chronological list of `PuzzlePlanner` snapshots.
///
/// Returns `history` such that `history[i]` is the planner state after
/// applying `snapshot.steps[0..=i]`. The deduced literals of step 0 are
/// already present after `PuzzlePlanner::new_with_config`'s trivial-lit
/// pass, so they are not re-applied; subsequent steps are applied via
/// `add_not_provable_known_lit`.
pub fn import_snapshot(
    snapshot: SessionSnapshot,
    strategy_db: Arc<Database>,
) -> Result<Vec<PuzzlePlanner>> {
    let puzzle: PuzzleParse = snapshot.puzzle.try_into()?;
    let puzzle = Arc::new(puzzle);

    let solver = PuzzleSolver::new_with_config(puzzle, snapshot.solver_config)?;
    let mut planner =
        PuzzlePlanner::new_with_config(solver, snapshot.planner_config).with_database(strategy_db);

    let mut history = Vec::with_capacity(snapshot.steps.len());
    history.push(planner.fork()?);

    for (i, step) in snapshot.steps.iter().enumerate().skip(1) {
        let lits: Vec<Lit> = step
            .deduced_lits
            .iter()
            .map(|&ipasir| Lit::from_ipasir(ipasir))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context(format!("Invalid literal in step {i}"))?;

        for lit in &lits {
            planner.solver().add_not_provable_known_lit(*lit);
        }
        history.push(planner.fork()?);
    }

    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::util::test_utils::build_puzzleparse;

    fn load_sudoku() -> PuzzlePlanner {
        let parse = build_puzzleparse(
            "../eprime/sudoku.eprime",
            "../eprime/sudoku/sudokuwiki/hiddensingles/hiddensingles.param",
        );
        let solver = PuzzleSolver::new(Arc::new(parse)).unwrap();
        PuzzlePlanner::new(solver)
    }

    #[test]
    fn roundtrip_initial_state() {
        let planner = load_sudoku();
        let history = vec![planner.fork().unwrap()];

        let snap = export_snapshot(&history).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        let snap2: SessionSnapshot = serde_json::from_str(&json).unwrap();

        let db = Arc::new(Database::empty());
        let history2 = import_snapshot(snap2, db).unwrap();

        assert_eq!(history2.len(), 1);
        let initial_lits: BTreeSet<Lit> = history[0].get_all_known_lits().iter().copied().collect();
        let restored_lits: BTreeSet<Lit> =
            history2[0].get_all_known_lits().iter().copied().collect();
        assert_eq!(initial_lits, restored_lits);
    }

    #[test]
    fn roundtrip_mid_solve() {
        // Run a few steps of solving, snapshot, restore, and check the
        // restored history matches the original both for the initial state
        // and for each intermediate step.
        let mut planner = load_sudoku();
        let mut history = vec![planner.fork().unwrap()];

        for _ in 0..3 {
            let muses = planner.smallest_muses_with_config();
            if muses.is_empty() {
                break;
            }
            for mc in &muses {
                for lit in &mc.lits {
                    planner.mark_lit_as_deduced(lit);
                }
            }
            history.push(planner.fork().unwrap());
        }

        assert!(history.len() >= 2, "need at least one mid-solve step");

        let snap = export_snapshot(&history).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        let snap2: SessionSnapshot = serde_json::from_str(&json).unwrap();

        let db = Arc::new(Database::empty());
        let history2 = import_snapshot(snap2, db).unwrap();

        assert_eq!(history.len(), history2.len());
        for (i, (orig, restored)) in history.iter().zip(history2.iter()).enumerate() {
            let orig_lits: BTreeSet<Lit> = orig.get_all_known_lits().iter().copied().collect();
            let restored_lits: BTreeSet<Lit> =
                restored.get_all_known_lits().iter().copied().collect();
            assert_eq!(
                orig_lits, restored_lits,
                "step {i} known-lits diverge after roundtrip"
            );
        }
    }
}
