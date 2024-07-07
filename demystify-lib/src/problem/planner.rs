use std::collections::BTreeSet;

use itertools::Itertools;
use rustsat::types::Lit;
use tracing::info;

use crate::{json::Problem, web::create_html};

use super::{
    musdict::MusDict,
    parse::PuzzleParse,
    solver::{MusConfig, PuzzleSolver},
    PuzLit,
};

#[derive(Copy, Clone)]
pub struct PlannerConfig {
    pub mus_config: MusConfig,
    pub merge_small_threshold: Option<i64>,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            mus_config: MusConfig::default(),
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

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle.
    pub fn all_muses(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        self.psolve
            .get_many_vars_small_mus_quick(&varlits, &self.config.mus_config)
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle which satisfy a filter.
    pub fn filtered_muses(
        &mut self,
        filter: Box<dyn Fn(&Lit, &mut PuzzlePlanner) -> bool>,
    ) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let varlits: BTreeSet<_> = varlits.into_iter().filter(|l| filter(l, self)).collect();
        self.psolve
            .get_many_vars_small_mus_quick(&varlits, &self.config.mus_config)
    }

    /// Returns a vector of the smallest MUSes of the puzzle.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS.
    pub fn smallest_muses(&mut self) -> Vec<(Lit, Vec<Lit>)> {
        //let mut t = QuickTimer::new("smallest_muses");
        let muses = self.all_muses();

        let min = muses.min();

        if min.is_none() {
            return vec![];
        }

        let min = min.unwrap();

        let mut vec = vec![];

        for (&lit, v) in muses.muses() {
            if let Some(m) = v.iter().next() {
                if m.len() == min {
                    vec.push((lit, m.clone()));
                }
            }
        }

        vec
    }

    /// Returns a vector of the smallest MUSes of the puzzle based on the planner's configuration.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS.
    pub fn smallest_muses_with_config(&mut self) -> Vec<(Lit, Vec<Lit>)> {
        let muses = self.smallest_muses();
        if muses.is_empty() {
            return muses;
        }

        if let Some(min) = self.config.merge_small_threshold {
            if muses[0].1.len() as i64 <= min {
                return muses;
            }
        }

        vec![muses[0].clone()]
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
    pub fn quick_solve(&mut self) -> Vec<Vec<(BTreeSet<PuzLit>, Vec<String>)>> {
        self.quick_solve_impl(false)
    }

    /// Solves the puzzle quickly and returns a sequence of steps, printing info on progress as solving runs
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a set of user-friendly literals and a vector of user-friendly constraints.
    pub fn quick_solve_with_progress(&mut self) -> Vec<Vec<(BTreeSet<PuzLit>, Vec<String>)>> {
        self.quick_solve_impl(true)
    }

    fn quick_solve_impl(&mut self, progress: bool) -> Vec<Vec<(BTreeSet<PuzLit>, Vec<String>)>> {
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

            if progress {
                println!(
                    "{} steps, just found {} muses of size {}, {} left",
                    solvesteps.len(),
                    muses.len(),
                    muses[0].1.len(),
                    self.psolve.get_provable_varlits().len()
                );
            } else {
                info!(target: "planner",
                    "{} steps, just found {} muses of size {}, {} left",
                    solvesteps.len(),
                    muses.len(),
                    muses[0].1.len(),
                    self.psolve.get_provable_varlits().len()
                );
            }
            // Add these muses to the solving steps
            solvesteps.push(muses);
        }
        info!(target: "planner", "solved!");
        solvesteps
    }

    pub fn check_solvability(&mut self) -> Option<i64> {
        while !self.psolve.get_provable_varlits().is_empty() {
            let lits = self.psolve.get_provable_varlits().clone();

            for l in lits {
                self.mark_lit_as_deduced(&l);
            }
        }

        if self.psolve.is_currently_solvable() {
            Some(
                self.psolve
                    .get_literals_to_try_solving()
                    .len()
                    .try_into()
                    .unwrap(),
            )
        } else {
            None
        }
    }

    pub fn get_provable_varlits(&mut self) -> BTreeSet<Lit> {
        self.psolve.get_provable_varlits().clone()
    }

    pub fn get_provable_varlits_including_reveals(&mut self) -> BTreeSet<Lit> {
        let mut all_lits = BTreeSet::new();

        while !self.psolve.get_provable_varlits().is_empty() {
            let varlits = self.psolve.get_provable_varlits().clone();

            for v in &varlits {
                self.mark_lit_as_deduced(v);
            }

            all_lits.extend(varlits.into_iter());
        }

        all_lits
    }

    /// Solves the puzzle quickly and returns a sequence of steps in HTML format.
    ///
    /// # Returns
    ///
    /// A string containing the HTML representation of the solution steps.
    pub fn quick_solve_html(&mut self) -> String {
        let mut html = String::new();
        while !self.psolve.get_provable_varlits().is_empty() {
            html += &self.quick_solve_html_step();
            html += "<br/>";
        }
        html
    }

    pub fn quick_solve_html_step(&mut self) -> String {
        let base_muses = self.smallest_muses_with_config();
        self.quick_display_html_step(base_muses)
    }

    pub fn quick_solve_html_step_for_literal(&mut self, lit_def: Vec<i64>) -> String {
        let muses = self.filtered_muses(Box::new(move |lit, planner| {
            let puzlit_list = planner.solver().lit_to_puzlit(lit);
            for puzlit in puzlit_list {
                let mut indices = puzlit.var().indices().clone();
                indices.push(puzlit.val());
                if dbg!(indices) == lit_def {
                    return true;
                }
            }
            false
        }));

        // TEMP CODE
        let min = muses.min();

        if min.is_none() {
            return "No MUS".to_owned();
        }

        let min = min.unwrap();

        let mut vec = vec![];

        for (&lit, v) in muses.muses() {
            if let Some(m) = v.iter().next() {
                if m.len() == min {
                    vec.push((lit, m.clone()));
                }
            }
        }

        //

        self.quick_display_html_step(vec)
    }

    pub fn quick_display_html_step(&mut self, base_muses: Vec<(Lit, Vec<Lit>)>) -> String {
        // Map the 'muses' to a user-friendly representation
        let muses = base_muses
            .iter()
            .map(|mus| self.mus_to_user_mus(mus))
            .collect_vec();

        let varlits = self.psolve.get_provable_varlits().clone();

        let tosolve_varvals: BTreeSet<_> = varlits
            .iter()
            .flat_map(|x| self.psolve.lit_to_puzlit(x))
            .map(super::PuzLit::varval)
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

        create_html(&problem)
    }

    /// Returns a reference to the puzzle being solved.
    ///
    /// # Returns
    ///
    /// A reference to the `PuzzleParse` instance representing the puzzle being solved.
    pub fn puzzle(&self) -> &PuzzleParse {
        self.psolve.puzzleparse()
    }

    /// Returns a mutable reference to the solver. Warning, incorrect use of underlying
    /// solver can result in incorrect answers.
    pub fn solver(&mut self) -> &mut PuzzleSolver {
        &mut self.psolve
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::problem::{planner::PuzzlePlanner, solver::PuzzleSolver};
    use itertools::Itertools;
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

        assert_eq!(sequence.iter().flatten().collect_vec().len(), 16);

        for (litset, cons) in sequence.iter().flatten() {
            assert!(!litset.is_empty());
            // It should be trivial to prove we only need one
            // constraint here, but MUS algorithms be tricky, if
            // this next line starts failing, it can be commented out.
            assert!(cons.len() <= 1);
        }
    }

    #[test]
    fn test_solvability_little_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        assert_eq!(plan.check_solvability(), Some(0));
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

        assert_eq!(sequence.iter().flatten().collect_vec().len(), 36);

        for (litset, cons) in sequence.iter().flatten() {
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

        assert_eq!(sequence.iter().flatten().collect_vec().len(), 25);

        for (litset, cons) in sequence.iter().flatten() {
            assert!(!litset.is_empty());
            // If this next line starts failing, it can be commented out.
            assert!(cons.len() <= 2);
        }
    }

    #[test]
    fn test_varlits_minesweeper_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperPrinted.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let first_step = plan.get_provable_varlits();

        let all_steps = plan.get_provable_varlits_including_reveals();

        let first_step: BTreeSet<_> = first_step
            .into_iter()
            .map(|x| plan.psolve.lit_to_puzlit(&x).clone())
            .collect();

        let all_steps: BTreeSet<_> = all_steps
            .into_iter()
            .map(|x| plan.psolve.lit_to_puzlit(&x).clone())
            .collect();

        insta::assert_debug_snapshot!(first_step);
        insta::assert_debug_snapshot!(all_steps);
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
        assert_eq!(sequence.iter().flatten().collect_vec().len(), 15);

        for (litset, cons) in sequence.iter().flatten() {
            assert!(!litset.is_empty());
            // If this next line starts failing, it can be commented out.
            assert!(cons.len() <= 2);
        }
    }

    #[test]
    fn test_solvability_minesweeper_wall_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperWall.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        assert_eq!(plan.check_solvability(), Some(20));
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
