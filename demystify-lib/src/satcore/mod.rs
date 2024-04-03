use std::sync::{Arc, Mutex};

use itertools::Itertools;
use rustsat::instances::Cnf;
use rustsat::solvers::{Solve, SolveIncremental};
use rustsat::types::{Assignment, Lit};
use tracing::info;

pub type Solver = rustsat_glucose::core::Glucose;

pub struct SatCore {
    pub solver: Arc<Mutex<Solver>>,
}

impl SatCore {
    pub fn new(cnf: Cnf) -> anyhow::Result<SatCore> {
        let mut solver = Solver::default();
        solver.add_cnf(cnf.clone())?;

        Ok(SatCore {
            solver: Arc::new(Mutex::new(solver)),
        })
    }

    pub fn assumption_solve(&self, lits: &[Lit]) -> bool {
        let mut solver = self.solver.lock().unwrap();
        let solve = solver.solve_assumps(lits).unwrap();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => true,
            rustsat::solvers::SolverResult::Unsat => false,
            rustsat::solvers::SolverResult::Interrupted => panic!(),
        };
        info!(target: "solver", "Solution to {:?} is {:?}", lits, result);
        result
    }

    pub fn assumption_solve_solution(&self, lits: &[Lit]) -> Option<Assignment> {
        let mut solver = self.solver.lock().unwrap();
        let solve = solver.solve_assumps(lits).unwrap();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Some(solver.full_solution().unwrap()),
            rustsat::solvers::SolverResult::Unsat => None,
            rustsat::solvers::SolverResult::Interrupted => panic!(),
        };
        info!(target: "solver", "Solution to {:?} is {:?}", lits, result);
        result
    }

    #[must_use]
    pub fn assumption_solve_with_core(&self, lits: &[Lit]) -> Option<Vec<Lit>> {
        let mut solver = self.solver.lock().unwrap();
        let solve = solver.solve_assumps(lits).unwrap();
        match solve {
            rustsat::solvers::SolverResult::Sat => None,
            rustsat::solvers::SolverResult::Unsat => {
                Some(solver.core().unwrap().into_iter().map(|l| !l).collect())
            }
            rustsat::solvers::SolverResult::Interrupted => panic!(),
        }
    }

    #[must_use]
    pub fn quick_mus(&self, known: &[Lit], in_lits: &[Lit]) -> Option<Vec<Lit>> {
        let lits = [known, in_lits].concat();
        let mut core = self.assumption_solve_with_core(&lits)?;
        // Need to make a copy for actually searching over
        let core_clone = core.clone();
        for &lit in in_lits {
            let location = core.iter().position(|&x| x == lit);
            if let Some(location) = location {
                let mut check_core = core.clone();
                check_core.remove(location);
                let candidate = self.assumption_solve_with_core(&check_core);
                if let Some(found) = candidate {
                    core = found;
                }
            }
        }
        Some(
            core.into_iter()
                .filter(|x| in_lits.contains(x))
                .collect_vec(),
        )
    }
}
