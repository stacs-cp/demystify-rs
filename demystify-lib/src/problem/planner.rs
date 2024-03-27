use std::collections::HashSet;

use rustsat::types::Lit;

use super::solver::PuzzleSolver;

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
}
