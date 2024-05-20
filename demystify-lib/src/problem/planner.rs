use std::collections::BTreeSet;

use itertools::Itertools;
use rustsat::types::Lit;

use crate::{json::Problem, web::create_html};

use super::{parse::PuzzleParse, solver::PuzzleSolver, PuzLit};

pub struct PlannerConfig {
    pub merge_small_threshold: Option<i64>,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            merge_small_threshold: Some(1),
        }
    }
}

/// The `PuzzlePlanner` struct represents a puzzle planner that can be used to solve puzzles.
pub struct PuzzlePlanner {
    psolve: PuzzleSolver,
    config: PlannerConfig,
}

impl PuzzlePlanner {
    /// Creates a new `PuzzlePlanner` instance.
    ///
    /// # Arguments
    ///
    /// * `psolve` - The `PuzzleSolver` instance used for solving the puzzle.
    ///
    /// # Returns
    ///
    /// A new `PuzzlePlanner` instance.
    #[must_use]
    pub fn new(psolve: PuzzleSolver) -> PuzzlePlanner {
        PuzzlePlanner {
            psolve,
            config: PlannerConfig::default(),
        }
    }

    /// Creates a new `PuzzlePlanner` instance with a custom configuration.
    ///
    /// # Arguments
    ///
    /// * `psolve` - The `PuzzleSolver` instance used for solving the puzzle.
    /// * `config` - The custom configuration for the planner.
    ///
    /// # Returns
    ///
    /// A new `PuzzlePlanner` instance with the specified configuration.
    #[must_use]
    pub fn new_with_config(psolve: PuzzleSolver, config: PlannerConfig) -> PuzzlePlanner {
        PuzzlePlanner { psolve, config }
    }

    /// Returns a vector of all minimal unsatisfiable subsets (MUSes) of the puzzle.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS.
    pub fn all_muses(&mut self) -> Vec<(Lit, Vec<Lit>)> {
        let varlits = self.psolve.get_provable_varlits().clone();
        self.psolve.get_many_vars_small_mus_quick(&varlits)
    }

    /// Returns a vector of the smallest MUSes of the puzzle.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS.
    pub fn smallest_muses(&mut self) -> Vec<(Lit, Vec<Lit>)> {
        let muses = self.all_muses();

        let minmus = muses.iter().map(|(_, x)| x.len()).min().unwrap();
        let muses: Vec<_> = muses
            .into_iter()
            .filter(|(_, x)| x.len() == minmus)
            .collect();

        muses
    }

    /// Returns a vector of the smallest MUSes of the puzzle based on the planner's configuration.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS.
    pub fn smallest_muses_with_config(&mut self) -> Vec<(Lit, Vec<Lit>)> {
        let muses = self.smallest_muses();
        if let Some(min) = self.config.merge_small_threshold {
            if muses[0].1.len() as i64 <= min {
                vec![muses[0].clone()]
            } else {
                muses
            }
        } else {
            vec![muses[0].clone()]
        }
    }

    /// Converts a MUS to a user-friendly MUS representation.
    ///
    /// # Arguments
    ///
    /// * `mus` - The MUS tuple to convert.
    ///
    /// # Returns
    ///
    /// A tuple containing a set of user-friendly literals and a vector of user-friendly constraints.
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

    /// Marks a literal as deduced.
    ///
    /// This method should only be called if there are no solutions with the negation of the literal.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to mark as deduced.
    pub fn mark_lit_as_deduced(&mut self, lit: &Lit) {
        self.psolve.add_known_lit(*lit);
    }

    /// Returns a reference to the vector of all known literals.
    ///
    /// This includes literals that have been marked as deduced and literals from 'REVEAL' statements.
    ///
    /// # Returns
    ///
    /// A reference to the vector of all known literals.
    pub fn get_all_known_lits(&self) -> &Vec<Lit> {
        self.psolve.get_known_lits()
    }

    /// Solves the puzzle quickly and returns a sequence of steps.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a set of user-friendly literals and a vector of user-friendly constraints.
    pub fn quick_solve(&mut self) -> Vec<(BTreeSet<PuzLit>, Vec<String>)> {
        let mut solvesteps = vec![];
        while !self.psolve.get_provable_varlits().is_empty() {
            let muses = self.smallest_muses_with_config();

            for (m, _) in &muses {
                self.mark_lit_as_deduced(m);
            }

            // Map the 'muses' to a user-friendly representation
            let muses = muses
                .into_iter()
                .map(|mus| self.mus_to_user_mus(&mus))
                .collect_vec();

            println!(
                "{} steps, just found {} muses of size {}, {} left",
                solvesteps.len(),
                muses.len(),
                muses[0].1.len(),
                self.psolve.get_provable_varlits().len()
            );

            // Add these muses to the solving steps
            solvesteps.extend(muses);
        }
        solvesteps
    }

    /// Solves the puzzle quickly and returns a sequence of steps in HTML format.
    ///
    /// # Returns
    ///
    /// A string containing the HTML representation of the solution steps.
    pub fn quick_solve_html(&mut self) -> String {
        let mut html = String::new();
        while !self.psolve.get_provable_varlits().is_empty() {
            let base_muses = self.smallest_muses_with_config();

            // Map the 'muses' to a user-friendly representation
            let muses = base_muses
                .iter()
                .map(|mus| self.mus_to_user_mus(mus))
                .collect_vec();

            let varlits = self.psolve.get_provable_varlits().clone();

            let tosolve_varvals: BTreeSet<_> = varlits
                .iter()
                .flat_map(|x| self.psolve.lit_to_puzlit(x))
                .map(|x| x.varval())
                .collect();

            let known_puzlits: BTreeSet<PuzLit> = self
                .get_all_known_lits()
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
    ///
    /// # Returns
    ///
    /// A reference to the `PuzzleParse` instance representing the puzzle being solved.
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
            assert!(cons.len() <= 1);
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
    fn test_plan_minesweeper_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperPrinted.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.len(), 25);

        for (litset, cons) in sequence {
            assert!(!litset.is_empty());
            // If this next line starts failing, it can be commented out.
            assert!(cons.len() <= 2);
        }
    }

    #[test]
    fn test_plan_minesweeper_wall_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperWall.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        // Should only be able to solve 15 steps, due to the 'wall'
        assert_eq!(sequence.len(), 15);

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
