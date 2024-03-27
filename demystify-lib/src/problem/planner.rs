use std::collections::HashSet;

use rustsat::types::Lit;

use super::{parse::PuzzleParse, solver::PuzzleSolver};

pub struct PuzzlePlanner {
    psolve: PuzzleSolver,
}

impl PuzzlePlanner {
    fn new(psolve: PuzzleSolver) -> PuzzlePlanner {
        PuzzlePlanner { psolve }
    }

    fn quick_solve(&mut self) -> Vec<(Lit, Vec<Lit>)> {
        let mut tosolve = self.psolve.get_unsatisfiable_varlits();
        let mut solvesteps = vec![];
        while tosolve.len() > 0 {
            let muses: Vec<_> = tosolve
                .iter()
                .map(|&x| (x, self.psolve.get_var_mus_quick(x).unwrap()))
                .collect();
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
            tosolve = tosolve
                .into_iter()
                .filter(|x| !removeset.contains(x))
                .collect();
            // Add these muses to the solving steps
            solvesteps.extend(muses);
        }
        solvesteps
    }

    fn puzzle(&self) -> &PuzzleParse {
        self.psolve.puzzleparse()
    }
}

#[cfg(test)]
mod tests {
    use crate::problem::{planner::PuzzlePlanner, solver::PuzzleSolver};
    use test_log::test;

    #[test]
    fn test_parse_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.len(), 16);

        for (lit, cons) in sequence {
            assert!(plan.puzzle().lit_is_var(&lit));
            assert!(cons.iter().all(|x| plan.puzzle().lit_is_con(x)));
            println!("{:?}", plan.puzzle().lit_to_vars(&lit));
            // It should be trivial to prove we only need one
            // constraint here, but MUS algorithms be tricky, if
            // this next line starts failing, it can be commented out.
            assert_eq!(cons.len(), 1);
        }
    }
}
