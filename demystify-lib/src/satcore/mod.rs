use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use itertools::Itertools;
use rustsat::instances::Cnf;
use rustsat::solvers::{Solve, SolveIncremental};
use rustsat::types::{Assignment, Lit};
use tracing::info;

pub type Solver = rustsat_glucose::core::Glucose;

/// Represents a SAT solver core.
/// The public interface to the solver is stateless.
/// Internally, we fix some values in the solver (represented by the)
/// 'fixed' set. Whenever we need to remove values from this set,
/// we restart the solver. This is not externally visible.
pub struct SatCore {
    pub solver: Arc<Mutex<Solver>>,
    pub cnf: Arc<Cnf>,
    pub fixed: RefCell<HashSet<Lit>>,
}

impl SatCore {
    /// Creates a new `SatCore` instance.
    ///
    /// # Arguments
    ///
    /// * `cnf` - The CNF formula to solve.
    ///
    /// # Returns
    ///
    /// A `SatCore` instance.
    pub fn new(cnf: Arc<Cnf>) -> anyhow::Result<SatCore> {
        let mut solver = Solver::default();
        solver.add_cnf(cnf.as_ref().clone())?;

        Ok(SatCore {
            solver: Arc::new(Mutex::new(solver)),
            cnf,
            fixed: RefCell::new(HashSet::new()),
        })
    }

    /// Fix the follow list of literals. As search progresses, we often want to fix a list
    /// of literals (the known values), but as solvers are in a threadpool, we want to
    /// treat solvers as memoryless. Therefore, we fix values, and also reboot the solver if
    /// we discover that we need to fix less literals than the already fixed list
    /// (stored in fixed)
    fn fix_values(&self, lits: &[Lit]) {
        let mut fixed = self.fixed.borrow_mut();

        {
            let mut solver = self.solver.lock().unwrap();

            for &l in lits {
                if !fixed.contains(&l) {
                    solver.add_unit(l).expect("FATAL: Solver bug 1");
                    fixed.insert(l);
                }
            }
        }

        // As we added all 'lits' to 'fixed', if there are more things in 'fixed'
        // something we don't want is in fixed.
        if fixed.len() > lits.len() {
            println!("Rebooting solver");
            let mut solver = Solver::default();
            solver
                .add_cnf(self.cnf.as_ref().clone())
                .expect("FATAL: Solver bug 2");
            fixed.clear();
            for &l in lits {
                if !fixed.contains(&l) {
                    solver.add_unit(l).expect("FATAL: Solver bug 3");
                    fixed.insert(l);
                }
            }
            let mut mutex_solver = self.solver.lock().unwrap();
            *mutex_solver = solver;
        }
    }

    /// Solves the CNF formula with the given assumptions.
    ///
    /// # Arguments
    ///
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// `true` if the formula is satisfiable, `false` if it is unsatisfiable.
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

    /// Solves the CNF formula with the given assumptions and returns the full solution.
    ///
    /// # Arguments
    ///
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// The full solution if the formula is satisfiable, `None` if it is unsatisfiable.
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

    /// Solves the CNF formula with the given assumptions and returns the unsatisfiable core.
    ///
    /// # Arguments
    ///
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// The unsatisfiable core if the formula is unsatisfiable, `None` if it is satisfiable.
    #[must_use]
    pub fn assumption_solve_with_core(&self, lits: &[Lit]) -> Option<Vec<Lit>> {
        self.fix_values(&[]);
        self.raw_assumption_solve_with_core(lits)
    }

    /// Solves the CNF formula with the given assumptions and returns the unsatisfiable core.
    /// *Not memoryless*: Uses whatever set of values are already fixed in the solver.
    ///
    /// # Arguments
    ///
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// The unsatisfiable core if the formula is unsatisfiable, `None` if it is satisfiable.
    #[must_use]
    fn raw_assumption_solve_with_core(&self, lits: &[Lit]) -> Option<Vec<Lit>> {
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

    /// Finds a minimal unsatisfiable subset (MUS) of literals given a set of known literals.
    ///
    /// # Arguments
    ///
    /// * `known` - The known literals.
    /// * `lits` - The set of literals to search over.
    ///
    /// # Returns
    ///
    /// The minimal unsatisfiable subset (MUS) of literals, if one exists.
    #[must_use]
    pub fn quick_mus(&self, known: &[Lit], lits: &[Lit], max_size: Option<i32>) -> Option<Vec<Lit>> {
        self.fix_values(known);
        let mut known_size = 0;
        let mut core = self.raw_assumption_solve_with_core(lits)?;
        // Need to make a copy for actually searching over
        for &lit in lits {
            let location = core.iter().position(|&x| x == lit);
            if let Some(location) = location {
                let mut check_core = core.clone();
                check_core.remove(location);
                let candidate = self.raw_assumption_solve_with_core(&check_core);
                if let Some(found) = candidate {
                    core = found;
                } else {
                    known_size += 1;
                    if let Some(max_size) = max_size {
                        if known_size > max_size {
                            return None;
                        }
                    }
                }
            }
        }
        Some(core.into_iter().filter(|x| lits.contains(x)).collect_vec())
    }
}
