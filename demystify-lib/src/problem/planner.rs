use std::collections::BTreeSet;

use itertools::Itertools;
use rustsat::types::Lit;

use crate::{json::Problem, web::create_html};

use super::{parse::PuzzleParse, solver::PuzzleSolver, PuzLit};

/// Represents a puzzle planner.
pub struct PuzzlePlanner {
    psolve: PuzzleSolver,

    tosolve: BTreeSet<Lit>,
    deduced: BTreeSet<Lit>,

    config: PlannerConfig,
}

#[derive(Default)]
pub struct PlannerConfig {
    pub merge_small_threshold: Option<i64>,
}

impl PuzzlePlanner {
    /// Creates a new `PuzzlePlanner` instance.
    #[must_use]
    pub fn new(psolve: PuzzleSolver) -> PuzzlePlanner {
        let tosolve = psolve.get_unsatisfiable_varlits();
        PuzzlePlanner {
            psolve,
            tosolve,
            deduced: BTreeSet::new(),
            config: PlannerConfig::default(),
        }
    }

    /// Creates a new `PuzzlePlanner` instance.
    #[must_use]
    pub fn new_with_config(psolve: PuzzleSolver, config: PlannerConfig) -> PuzzlePlanner {
        let tosolve = psolve.get_unsatisfiable_varlits();
        PuzzlePlanner {
            psolve,
            tosolve,
            deduced: BTreeSet::new(),
            config,
        }
    }

    pub fn all_muses(&self) -> Vec<(Lit, Vec<Lit>)> {
        self.psolve.get_many_vars_small_mus_quick(&self.tosolve)
    }

    pub fn smallest_muses(&self) -> Vec<(Lit, Vec<Lit>)> {
        let muses = self.all_muses();

        let minmus = muses.iter().map(|(_, x)| x.len()).min().unwrap();
        let muses: Vec<_> = muses
            .into_iter()
            .filter(|(_, x)| x.len() == minmus)
            .collect();

        muses
    }

    pub fn smallest_muses_with_config(&self) -> Vec<(Lit, Vec<Lit>)> {
        let muses = self.smallest_muses();
        if let Some(min) = self.config.merge_small_threshold {
            if muses[0].1.len() as i64 <= min {
                vec![muses[0].clone()]
            } else {
                muses
            }
        } else {
            muses
        }
    }

    pub fn mus_to_user_mus(&self, mus: &(Lit, Vec<Lit>)) -> (BTreeSet<PuzLit>, Vec<String>) {
        let (l, x) = mus;
        (
            self.psolve.puzzleparse().lit_to_vars(l).clone(),
            x.iter()
                .map(|c| self.psolve.puzzleparse().lit_to_con(c))
                .cloned()
                .collect_vec(),
        )
    }

    pub fn mark_lit_as_deduced(&mut self, lit: &Lit) {
        assert!(self.tosolve.contains(lit));
        self.tosolve.remove(lit);
        self.deduced.insert(*lit);
        self.psolve.add_known_lit(!*lit);
    }

    pub fn get_all_deduced_lits(&self) -> &BTreeSet<Lit> {
        &self.deduced
    }

    /// Solves the puzzle quickly and returns a sequence of steps.
    pub fn quick_solve(&mut self) -> Vec<(BTreeSet<PuzLit>, Vec<String>)> {
        let mut solvesteps = vec![];
        while !self.tosolve.is_empty() {
            let muses = self.smallest_muses_with_config();

            for (m, _) in &muses {
                self.mark_lit_as_deduced(m);
            }

            // Map the 'muses' to a user PuzLits
            let muses = muses
                .into_iter()
                .map(|mus| self.mus_to_user_mus(&mus))
                .collect_vec();

            println!(
                "{} steps, just found {} muses of size {}, {} left",
                solvesteps.len(),
                muses.len(),
                muses[0].1.len(),
                self.tosolve.len()
            );

            // Add these muses to the solving steps
            solvesteps.extend(muses);
        }
        solvesteps
    }

    /// Solves the puzzle quickly and returns a sequence of steps.
    pub fn quick_solve_html(&mut self) -> String {
        let mut html = String::new();
        while !self.tosolve.is_empty() {
            let base_muses = self.smallest_muses_with_config();

            // Map the 'muses' to a user PuzLits
            let muses = base_muses
                .iter()
                .map(|mus| self.mus_to_user_mus(mus))
                .collect_vec();

            let tosolve_varvals: BTreeSet<_> = self
                .tosolve
                .iter()
                .flat_map(|x| self.psolve.lit_to_puzlit(x))
                .map(|x| x.varval())
                .collect();

            let known_puzlits: BTreeSet<PuzLit> = self
                .deduced
                .iter()
                .flat_map(|x| self.psolve.lit_to_puzlit(x))
                .cloned()
                .collect();

            let deduced: BTreeSet<_> = muses.iter().flat_map(|x| x.0.clone()).collect();

            let constraints = muses.iter().flat_map(|x| x.1.clone()).collect_vec();

            let nice_deduced: String = deduced.iter().format(", ").to_string();

            let problem = Problem::new_from_puzzle_and_mus(
                &self.psolve,
                &tosolve_varvals,
                &known_puzlits,
                &deduced,
                &constraints,
                &format!(
                    "{:?} because of {} constraints",
                    nice_deduced,
                    &constraints.len()
                ),
            )
            .expect("Cannot make puzzle json");

            for (m, _) in &base_muses {
                self.mark_lit_as_deduced(m);
            }

            html += &create_html(&problem);
            html += "<br/>";
        }
        html
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

    // This test doesn't really do any deep tests,
    // just do a full end-to-end run
    #[test]
    fn test_plan_binairo_essence_html() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/binairo.eprime",
            "./tst/binairo-1.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let _ = plan.quick_solve_html();
    }
}
