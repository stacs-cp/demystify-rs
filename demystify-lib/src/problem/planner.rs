use std::collections::{BTreeSet, HashSet};

use itertools::Itertools;

use super::{parse::PuzzleParse, solver::PuzzleSolver, PuzLit};

/// Represents a puzzle planner.
pub struct PuzzlePlanner {
    psolve: PuzzleSolver,
}

impl PuzzlePlanner {
    /// Creates a new `PuzzlePlanner` instance.
    #[must_use]
    pub fn new(psolve: PuzzleSolver) -> PuzzlePlanner {
        PuzzlePlanner { psolve }
    }

    /// Solves the puzzle quickly and returns a sequence of steps.
    pub fn quick_solve(&mut self) -> Vec<(BTreeSet<PuzLit>, Vec<String>)> {
        let mut tosolve = self.psolve.get_unsatisfiable_varlits();
        let mut solvesteps = vec![];
        while !tosolve.is_empty() {
            let muses = self.psolve.get_many_vars_small_mus_quick(&tosolve);

            let minmus = muses.iter().map(|(_, x)| x.len()).min().unwrap();
            let muses: Vec<_> = muses
                .into_iter()
                .filter(|(_, x)| x.len() == minmus)
                .collect();
            for (m, _) in &muses {
                self.psolve.add_known_lit(!*m);
            }
            let removeset: HashSet<_> = muses.iter().map(|(lit, _)| lit).collect();
            // Remove these literals from those that need to be solved
            tosolve.retain(|x| !removeset.contains(x));

            // Map the 'muses' to a user PuzLits
            let muses = muses
                .into_iter()
                .map(|(l, x)| {
                    (
                        self.psolve.puzzleparse().lit_to_vars(&l).clone(),
                        x.into_iter()
                            .map(|c| self.psolve.puzzleparse().lit_to_con(&c))
                            .cloned()
                            .collect_vec(),
                    )
                })
                .collect_vec();

            println!(
                "{} steps, just found {} muses of size {}, {} left",
                solvesteps.len(),
                muses.len(),
                minmus,
                tosolve.len()
            );

            // Add these muses to the solving steps
            solvesteps.extend(muses);
        }
        solvesteps
    }

    /// Returns a reference to the puzzle being solved.
    fn puzzle(&self) -> &PuzzleParse {
        self.psolve.puzzleparse()
    }
}

#[cfg(test)]
mod tests {
    use crate::problem::{planner::PuzzlePlanner, solver::PuzzleSolver};
    use test_log::test;

    #[test]
    fn test_plan_little_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.len(), 16);

        for (litset, cons) in sequence {
            assert!(!litset.is_empty());
            // It should be trivial to prove we only need one
            // constraint here, but MUS algorithms be tricky, if
            // this next line starts failing, it can be commented out.
            assert_eq!(cons.len(), 1);
        }
    }

    // This test doesn't really do any deep tests,
    // just do a full end-to-end run
    #[test]
    fn test_plan_binairo_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/binairo.eprime",
            "./tst/binairo-1.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.len(), 36);

        for (litset, cons) in sequence {
            assert!(!litset.is_empty());
            // If this next line starts failing, it can be commented out.
            assert!(cons.len() <= 2);
        }
    }
}
