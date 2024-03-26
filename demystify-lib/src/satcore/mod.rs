use std::borrow::BorrowMut;
use std::sync::{Arc, Mutex};

use rustsat::instances::{Cnf, SatInstance};
use rustsat::solvers::{Solve, SolveIncremental};
use rustsat::types::Lit;
use tracing::info; // Import the SolverTrait trait

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

    pub fn assumption_solve_solution(&self, lits: &[Lit]) -> bool {
        let mut solver = self.solver.lock().unwrap();
        let solve = solver.solve_assumps(lits).unwrap();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => true,
            rustsat::solvers::SolverResult::Unsat => false,
            rustsat::solvers::SolverResult::Interrupted => panic!(),
        };
        panic!();
        info!(target: "solver", "Solution to {:?} is {:?}", lits, result);
        result
    }

    pub fn assumption_solve_with_core(&self, lits: &[Lit]) -> Option<Vec<Lit>> {
        let mut solver = self.solver.lock().unwrap();
        let solve = solver.solve_assumps(lits).unwrap();
        match solve {
            rustsat::solvers::SolverResult::Sat => None,
            rustsat::solvers::SolverResult::Unsat => Some(solver.core().unwrap()),
            rustsat::solvers::SolverResult::Interrupted => panic!(),
        }
    }
}
