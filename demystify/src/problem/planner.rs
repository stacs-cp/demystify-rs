use std::collections::{BTreeMap, BTreeSet};

use itertools::Itertools;
use rustsat::types::Lit;
use tracing::info;

use crate::{
    json::{DescriptionStatement, Problem},
    problem::{
        VarValPair,
        musdict::{MusContext, merge_muscontexts},
    },
    satcore::get_solver_calls,
    web::create_html,
};

use super::{
    PuzLit,
    musdict::MusDict,
    parse::PuzzleParse,
    solver::{MusConfig, PuzzleSolver},
};

#[derive(Copy, Clone)]
pub struct PlannerConfig {
    pub mus_config: MusConfig,
    pub merge_small_threshold: i64,
    pub skip_small_threshold: i64,
    pub expand_to_all_deductions: bool,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            mus_config: MusConfig::default(),
            merge_small_threshold: 1,
            skip_small_threshold: 0,
            expand_to_all_deductions: true,
        }
    }
}

/// The `PuzzlePlanner` struct represents a puzzle planner that can be used to solve puzzles.
pub struct PuzzlePlanner {
    psolve: PuzzleSolver,
    config: PlannerConfig,
}

type FilterType = Box<dyn Fn(&Lit, &mut PuzzlePlanner) -> bool>;

/// A `PuzzlePlanner` is responsible for finding minimal unsatisfiable subsets (MUSes) in a puzzle
/// and using them to generate solution steps.
///
/// The planner works by identifying the smallest sets of constraints that lead to logical deductions,
/// allowing it to generate human-understandable solution steps. It can also analyze the difficulty
/// of different parts of the puzzle and present solutions in various formats including HTML.
///
///
/// The planner can find different types of MUSes:
/// - Smallish MUSes (more efficient)
/// - All MUSes including larger ones
/// - Filtered MUSes that match specific criteria
///
/// It can also track the puzzle's state by marking literals as deduced and
/// checking overall solvability.
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
        let mut pp = PuzzlePlanner {
            psolve,
            config: PlannerConfig::default(),
        };
        pp.mark_trivial_lits_as_deduced();
        pp
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
        let mut pp = PuzzlePlanner { psolve, config };
        pp.mark_trivial_lits_as_deduced();
        pp
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle,
    pub fn all_smallish_muses(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        self.psolve
            .get_many_vars_small_mus_quick(&varlits, &self.config.mus_config, None)
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle.
    pub fn all_muses_with_larger(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let mut conf_clone = self.config.mus_config;
        conf_clone.find_bigger = true;
        self.psolve
            .get_many_vars_small_mus_quick(&varlits, &conf_clone, None)
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle which satisfy a filter.
    pub fn filtered_muses(&mut self, filter: FilterType) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let varlits: BTreeSet<_> = varlits.into_iter().filter(|l| filter(l, self)).collect();
        self.psolve
            .get_many_vars_small_mus_quick(&varlits, &self.config.mus_config, None)
    }

    /// Returns a vector of the smallest MUSes of the puzzle.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS.
    pub fn smallest_muses(&mut self) -> Vec<MusContext> {
        //let mut t = QuickTimer::new("smallest_muses");
        let muses = self.all_smallish_muses();

        let min = muses.min();

        if min.is_none() {
            return vec![];
        }

        let min = min.unwrap();
        let mut vec = vec![];

        for v in muses.muses().values() {
            if let Some(m) = v.iter().next() {
                if m.mus_len() <= min {
                    vec.push(m.clone());
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
    pub fn smallest_muses_with_config(&mut self) -> Vec<MusContext> {
        let muses = self.smallest_muses();
        if muses.is_empty() {
            return muses;
        }

        // Merge identical MUSes
        let muses = merge_muscontexts(&muses);

        // Return all MUSes if they are small enough
        if muses[0].mus_len() as i64 <= self.config.merge_small_threshold {
            return muses;
        }

        // Todo: Try to pick a 'good' MUS, instead of the first one?

        if self.config.expand_to_all_deductions {
            vec![self.psolve.get_all_lits_solved_by_mus(&muses[0])]
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
    pub fn mus_to_user_mus(&self, mc: &MusContext) -> (BTreeSet<PuzLit>, Vec<String>) {
        let lits = &mc.lits;
        let x = &mc.mus;
        (
            lits.iter()
                .flat_map(|l| self.psolve.puzzleparse().lit_to_vars(l))
                .cloned()
                .collect(),
            x.iter()
                .map(|c| self.psolve.puzzleparse().lit_to_con(c))
                .cloned()
                .collect_vec(),
        )
    }

    /// Deal with MUSes of 0 (which mean the puzzle has deduction that can be made without
    /// any 'user' constraints. These often arise from initial setup.
    pub fn mark_trivial_lits_as_deduced(&mut self) {
        let varlits = self.psolve.get_provable_varlits().clone();
        let trivial_lits = self.psolve.get_many_vars_mus_size_0(&varlits);
        for l in trivial_lits {
            self.mark_lit_as_deduced(&l);
        }
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

    /// Marks multiple literals as deduced.
    ///
    /// This method should only be called if there are no solutions with the negation of the literals.
    ///
    /// # Arguments
    ///
    /// * `lits` - A slice of literals to mark as deduced.
    pub fn mark_lits_as_deduced(&mut self, lits: &[Lit]) {
        for lit in lits {
            self.psolve.add_known_lit(*lit);
        }
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
        'litloop: while !self.psolve.get_provable_varlits().is_empty() {
            let muses = self.smallest_muses_with_config();

            for mus in &muses {
                for lit in &mus.lits {
                    self.mark_lit_as_deduced(lit);
                }
            }

            if !muses.is_empty() && muses[0].mus_len() as i64 <= self.config.skip_small_threshold {
                continue 'litloop;
            }
            // Map the 'muses' to a user-friendly representation
            let muses = muses
                .into_iter()
                .map(|mus| self.mus_to_user_mus(&mus))
                .collect_vec();

            if progress {
                eprintln!(
                    "{} steps, just found {} muses of size {}, {} left, {} solver calls so far",
                    solvesteps.len(),
                    muses.len(),
                    muses[0].1.len(),
                    self.psolve.get_provable_varlits().len(),
                    get_solver_calls(),
                );
            } else {
                info!(target: "planner",
                    "{} steps, just found {} muses of size {}, {} left, {} solver calls so far",
                    solvesteps.len(),
                    muses.len(),
                    muses[0].1.len(),
                    self.psolve.get_provable_varlits().len(),
                    get_solver_calls(),
                );
            }
            // Add these muses to the solving steps
            solvesteps.push(muses);
        }
        info!(target: "planner", "solved!");
        solvesteps
    }

    /// Checks the solvability of the current problem state. This can be used
    /// to both check if a problem is inconsistent, or how much of the problem
    /// does not have a unique solution
    ///
    /// # Returns
    /// - `Some(i64)`: If the problem is not inconsistent, return the number of literals
    ///   which are not fixed to a single value.
    /// - `None`: If the problem is has no solution.
    pub fn check_solvability(&mut self) -> Option<i64> {
        while !self.psolve.get_provable_varlits().is_empty() {
            let lits = self.psolve.get_provable_varlits().clone();

            for l in lits {
                self.mark_lit_as_deduced(&l);
            }
        }

        if self.psolve.is_currently_solvable() {
            let lits = self.psolve.get_literals_to_try_solving();

            for l in &lits {
                self.solver().lit_to_puzlit(l);
            }

            Some(lits.len().try_into().unwrap())
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
            let (new_html, lits) = self.quick_solve_html_step();
            html += &new_html;
            self.mark_lits_as_deduced(&lits);
            html += "<br/>";
        }
        html
    }

    pub fn quick_solve_html_step(&mut self) -> (String, Vec<Lit>) {
        let base_muses = self.smallest_muses_with_config();
        self.quick_display_html_step(Some(base_muses))
    }

    pub fn quick_generate_html_difficulties(&mut self) -> String {
        let base_muses = self.all_muses_with_larger();

        let base_difficulties: BTreeMap<Lit, usize> = base_muses
            .muses()
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (*k, v.iter().next().unwrap().mus_len()))
            .collect();

        self.quick_display_difficulty_step(base_difficulties)
    }

    pub fn quick_solve_html_step_for_literal(&mut self, lit_def: Vec<i64>) -> (String, Vec<Lit>) {
        let muses = self.filtered_muses(Box::new(move |lit, planner| {
            let puzlit_list = planner.solver().lit_to_puzlit(lit);
            for puzlit in puzlit_list {
                let mut indices = puzlit.var().indices().clone();
                indices.push(puzlit.val());
                if indices == lit_def {
                    return true;
                }
            }
            false
        }));

        // TEMP CODE
        let min = muses.min();

        if min.is_none() {
            return ("No MUS".to_owned(), vec![]);
        }

        let min = min.unwrap();

        let mut vec = vec![];

        for v in muses.muses().values() {
            if let Some(m) = v.iter().next() {
                if m.mus_len() == min {
                    vec.push(m.clone());
                }
            }
        }

        //

        self.quick_display_html_step(Some(vec))
    }

    pub fn quick_display_html_step(
        &mut self,
        base_muses: Option<Vec<MusContext>>,
    ) -> (String, Vec<Lit>) {
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

        if let Some(base_muses) = base_muses {
            // Map the 'muses' to a user-friendly representation
            let muses = base_muses
                .iter()
                .map(|mus| self.mus_to_user_mus(mus))
                .collect_vec();

            let all_deduced: BTreeSet<_> = muses.iter().flat_map(|x| x.0.clone()).collect();

            let pre_string = if base_muses.len() > 1 {
                format!(
                    "{} simple deductions are being shown here in a single step. <br/>",
                    base_muses.len()
                )
            } else {
                "Made the following deductions:<br/>".to_owned()
            };

            let mut description_list: Vec<DescriptionStatement> = Vec::new();

            for mus in &muses {
                let deduced = PuzLit::nice_puzlit_list_html(&mus.0);
                description_list.push(DescriptionStatement {
                    result: deduced,
                    constraints: mus.1.clone(),
                });
            }

            let problem = Problem::new_from_puzzle_and_mus(
                &self.psolve,
                &tosolve_varvals,
                &known_puzlits,
                &all_deduced,
                &description_list,
                &pre_string,
            )
            .expect("Cannot make puzzle json");

            let v = base_muses
                .iter()
                .flat_map(|mc| &mc.lits)
                .copied()
                .collect_vec();
            for m in &v {
                self.mark_lit_as_deduced(m);
            }

            (create_html(&problem), v)
        } else {
            let deduced = BTreeSet::new();
            let description = "The initial puzzle state".to_string();

            let problem = Problem::new_from_puzzle_and_state(
                &self.psolve,
                &tosolve_varvals,
                &known_puzlits,
                &deduced,
                &description,
            )
            .expect("Cannot make puzzle json");

            (create_html(&problem), vec![])
        }
    }

    pub fn quick_display_difficulty_step(
        &mut self,
        base_difficulties: BTreeMap<Lit, usize>,
    ) -> String {
        // Make a nicer map

        let mut vvpmap: BTreeMap<VarValPair, usize> = BTreeMap::new();

        for (lit, &val) in &base_difficulties {
            for puzlit in self.psolve.puzzleparse().lit_to_vars(lit) {
                let vvp = puzlit.varval();
                vvpmap.insert(vvp, val);
            }
        }

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

        let problem = Problem::new_from_puzzle_and_difficulty(
            &self.psolve,
            &tosolve_varvals,
            &known_puzlits,
            &vvpmap,
            "The difficulty of the problem",
        )
        .expect("Cannot make puzzle json");

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
    use std::{collections::BTreeSet, sync::Arc};

    use crate::problem::{planner::PuzzlePlanner, solver::PuzzleSolver};
    use itertools::Itertools;
    use test_log::test;

    #[test]
    fn test_plan_little_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let result = Arc::new(result);

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.iter().flatten().collect_vec().len(), 8);

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

        let result = Arc::new(result);

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

        let result = Arc::new(result);

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.iter().flatten().collect_vec().len(), 21);

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

        let result = Arc::new(result);

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        assert_eq!(sequence.iter().flatten().collect_vec().len(), 9);

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

        let result = Arc::new(result);

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

        let result = Arc::new(result);

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let sequence = plan.quick_solve();

        // Warning: This number may change as MUS detection / merging improves.
        // Changes should be sanity checked by printing out the sequence.
        assert_eq!(sequence.iter().flatten().collect_vec().len(), 8);

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

        let result = Arc::new(result);

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

        let result = Arc::new(result);

        let puz = PuzzleSolver::new(result).unwrap();

        let mut plan = PuzzlePlanner::new(puz);

        let _ = plan.quick_solve_html();
    }
}
