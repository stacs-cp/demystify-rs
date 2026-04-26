use std::collections::{BTreeMap, BTreeSet};

use itertools::Itertools;
use rayon::iter::{ParallelBridge, ParallelIterator};
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

#[derive(Copy, Clone, Debug, Default, PartialEq, clap::ValueEnum)]
pub enum MusMethod {
    /// Raw SAT cores only: get cores for all lits, minimise the smallest.
    Core,
    /// Standard MUS search (no core pre-pass).
    Mus,
    /// Hybrid: size-1 pass, then cores, then full MUS if needed.
    #[default]
    #[value(name = "core+mus")]
    CorePlusMus,
}

#[derive(Copy, Clone)]
pub struct PlannerConfig {
    pub mus_config: MusConfig,
    pub merge_small_threshold: i64,
    pub skip_small_threshold: i64,
    pub expand_to_all_deductions: bool,
    /// Stop after this many solve steps. `None` means run to completion.
    pub max_steps: Option<usize>,
    /// Which MUS generation algorithm to use.
    pub mus_method: MusMethod,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            mus_config: MusConfig::default(),
            merge_small_threshold: 1,
            skip_small_threshold: 0,
            expand_to_all_deductions: true,
            max_steps: None,
            mus_method: MusMethod::default(),
        }
    }
}

/// The `PuzzlePlanner` struct represents a puzzle planner that can be used to solve puzzles.
pub struct PuzzlePlanner {
    psolve: PuzzleSolver,
    config: PlannerConfig,
    /// Cross-step MUS cache. Adding known lits only tightens the SAT problem, so a MUS found
    /// in an earlier step is still a valid unsatisfiable subset in later steps (though it may
    /// no longer be minimal). We carry it forward so we can skip re-searching lits whose cached
    /// MUS size already meets the current search target.
    mus_cache: MusDict,
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
            mus_cache: MusDict::new(),
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
        let mut pp = PuzzlePlanner {
            psolve,
            config,
            mus_cache: MusDict::new(),
        };
        pp.mark_trivial_lits_as_deduced();
        pp
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle,
    pub fn all_smallish_muses(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let full_result = self.psolve.get_many_vars_small_mus_quick(
            &varlits,
            &self.config.mus_config,
            Some(self.mus_cache.clone()),
        );
        self.update_mus_cache(&full_result);
        // Return only entries for the current varlits — the full_result may also contain
        // stale entries from earlier steps that must not be seen by callers.
        Self::filter_musdict_to_lits(full_result, &varlits)
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle.
    ///
    /// The returned dict keeps every MUS the search finds per literal, including strictly
    /// larger ones. Use this when analysing whether a literal has alternative explanations
    /// of different sizes (e.g. whether a deduction that needs a size-2 MUS in one model
    /// still has a size-1 MUS via another path).
    pub fn all_muses_with_larger(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let mut conf_clone = self.config.mus_config;
        conf_clone.find_bigger = true;
        conf_clone.find_one = false;
        conf_clone.keep_all_muses = true;
        self.psolve
            .get_many_vars_small_mus_quick(&varlits, &conf_clone, None)
    }

    /// Core-guided MUS search: get raw SAT cores for all provable lits,
    /// then minimise only the cores of smallest size into true MUSes.
    fn core_guided_muses(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let cores = self.psolve.get_all_cores(&varlits);

        if cores.is_empty() {
            return MusDict::new();
        }

        let min_size = cores.iter().map(|(_, core)| core.len()).min().unwrap();

        let smallest: Vec<_> = cores
            .into_iter()
            .filter(|(_, core)| core.len() == min_size)
            .collect();

        self.psolve.minimise_cores(&smallest)
    }

    /// Hybrid MUS search: size-1 pass, then cores, then full MUS if needed.
    fn core_plus_mus_muses(&mut self) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();

        // Phase 1: size-1 scan
        let size1_results: Vec<_> = varlits
            .iter()
            .par_bridge()
            .filter_map(|&lit| {
                let t0 = std::time::Instant::now();
                let ret = self.psolve.get_var_mus_size_1(lit, Some(1));
                let elapsed = t0.elapsed();
                let outcome = match &ret {
                    Ok(v) if !v.is_empty() => crate::stats::MusOutcome::Found(1),
                    Ok(_) => crate::stats::MusOutcome::NotFound,
                    Err(_) => crate::stats::MusOutcome::Timeout,
                };
                crate::stats::record_mus_search(elapsed, outcome, crate::stats::MusFunction::Size1);
                match ret {
                    Ok(v) if !v.is_empty() => {
                        let bts: BTreeSet<Lit> = v[0].iter().copied().collect();
                        Some((lit, bts))
                    }
                    _ => None,
                }
            })
            .collect();

        if !size1_results.is_empty() {
            let mut dict = MusDict::new();
            for (lit, mus) in size1_results {
                dict.add_mus(lit, mus);
            }
            self.update_mus_cache(&dict);
            return dict;
        }

        // Phase 2: get raw cores
        let cores = self.psolve.get_all_cores(&varlits);
        if cores.is_empty() {
            return MusDict::new();
        }

        let min_core_size = cores.iter().map(|(_, core)| core.len()).min().unwrap();
        info!(target: "cores", "core+mus: min core size = {min_core_size}");

        // Phase 3: if smallest core <= 2, minimise those and done
        if min_core_size <= 2 {
            let smallest: Vec<_> = cores
                .into_iter()
                .filter(|(_, core)| core.len() == min_core_size)
                .collect();
            let dict = self.psolve.minimise_cores(&smallest);
            self.update_mus_cache(&dict);
            return dict;
        }

        // Phase 4: full MUS search
        self.all_smallish_muses()
    }

    /// Returns a [`MusDict`] of all minimal unsatisfiable subsets (MUSes) of the puzzle which satisfy a filter.
    pub fn filtered_muses(&mut self, filter: FilterType) -> MusDict {
        let varlits = self.psolve.get_provable_varlits().clone();
        let varlits: BTreeSet<_> = varlits.into_iter().filter(|l| filter(l, self)).collect();
        let full_result = self.psolve.get_many_vars_small_mus_quick(
            &varlits,
            &self.config.mus_config,
            Some(self.mus_cache.clone()),
        );
        self.update_mus_cache(&full_result);
        Self::filter_musdict_to_lits(full_result, &varlits)
    }

    /// Updates the MUS cache from a search result, skipping size-0 MUSes (trivial deductions).
    fn update_mus_cache(&mut self, result: &MusDict) {
        for (lit, mus_set) in result.muses() {
            for mc in mus_set {
                if !mc.mus.is_empty() {
                    self.mus_cache.add_mus(*lit, mc.mus.clone());
                }
            }
        }
    }

    /// Filters a `MusDict` to only contain entries whose literal is in `lits`.
    fn filter_musdict_to_lits(dict: MusDict, lits: &BTreeSet<Lit>) -> MusDict {
        let mut result = MusDict::new();
        for lit in lits {
            if let Some(mus_set) = dict.muses().get(lit) {
                for mc in mus_set {
                    result.add_mus(*lit, mc.mus.clone());
                }
            }
        }
        result
    }

    fn smallest_muses_from_dict(dict: &MusDict) -> Vec<MusContext> {
        let min = dict.min();
        if min.is_none() {
            return vec![];
        }
        let min = min.unwrap();
        let mut vec = vec![];
        for v in dict.muses().values() {
            if let Some(m) = v.iter().next()
                && m.mus_len() <= min
            {
                vec.push(m.clone());
            }
        }
        vec
    }

    fn apply_config_to_muses(&mut self, muses: Vec<MusContext>) -> Vec<MusContext> {
        if muses.is_empty() {
            return muses;
        }
        let muses = merge_muscontexts(&muses);
        if muses[0].mus_len() as i64 <= self.config.merge_small_threshold {
            return muses;
        }
        if self.config.expand_to_all_deductions {
            vec![self.psolve.get_all_lits_solved_by_mus(&muses[0])]
        } else {
            vec![muses[0].clone()]
        }
    }

    /// Returns a vector of the smallest MUSes of the puzzle.
    pub fn smallest_muses(&mut self) -> Vec<MusContext> {
        let dict = self.all_smallish_muses();
        Self::smallest_muses_from_dict(&dict)
    }

    /// Returns a vector of the smallest MUSes of the puzzle based on the planner's configuration.
    pub fn smallest_muses_with_config(&mut self) -> Vec<MusContext> {
        let muses = self.smallest_muses();
        self.apply_config_to_muses(muses)
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
    pub fn quick_solve(&mut self) -> Vec<Vec<(BTreeSet<PuzLit>, Vec<String>)>> {
        let mut solvesteps = vec![];
        'litloop: while !self.psolve.get_provable_varlits().is_empty() {
            if self.config.max_steps.is_some_and(|n| solvesteps.len() >= n) {
                break;
            }
            let _step_timer = crate::stats::PhaseTimer::solve_step();

            let cores_enabled = tracing::enabled!(target: "cores", tracing::Level::INFO);

            let (core_min, core_count_1) =
                if cores_enabled && self.config.mus_method == MusMethod::Mus {
                    let varlits = self.psolve.get_provable_varlits().clone();
                    self.psolve.core_size_summary(&varlits)
                } else {
                    (None, 0)
                };

            let (muses, mus_count_1) = match self.config.mus_method {
                MusMethod::Core => {
                    let dict = self.core_guided_muses();
                    let raw = Self::smallest_muses_from_dict(&dict);
                    (self.apply_config_to_muses(raw), 0)
                }
                MusMethod::CorePlusMus => {
                    let dict = self.core_plus_mus_muses();
                    let count_1 = if cores_enabled {
                        dict.count_at_size(1)
                    } else {
                        0
                    };
                    let raw = Self::smallest_muses_from_dict(&dict);
                    (self.apply_config_to_muses(raw), count_1)
                }
                MusMethod::Mus if cores_enabled => {
                    let dict = self.all_smallish_muses();
                    let count_1 = dict.count_at_size(1);
                    let raw = Self::smallest_muses_from_dict(&dict);
                    (self.apply_config_to_muses(raw), count_1)
                }
                MusMethod::Mus => (self.smallest_muses_with_config(), 0),
            };

            for mus in &muses {
                for lit in &mus.lits {
                    self.mark_lit_as_deduced(lit);
                }
            }

            if !muses.is_empty() && muses[0].mus_len() as i64 <= self.config.skip_small_threshold {
                info!(target: "cores",
                    "Step {} (skipped): core min={} #1={}, true MUS min={} #1={}",
                    solvesteps.len(),
                    core_min.map_or("none".to_string(), |v| v.to_string()),
                    core_count_1,
                    muses[0].mus_len(),
                    mus_count_1,
                );
                continue 'litloop;
            }
            let muses = muses
                .into_iter()
                .map(|mus| self.mus_to_user_mus(&mus))
                .collect_vec();

            info!(target: "cores",
                "Step {}: core min={} #1={}, true MUS min={} #1={}",
                solvesteps.len(),
                core_min.map_or("none".to_string(), |v| v.to_string()),
                core_count_1,
                muses[0].1.len(),
                mus_count_1,
            );

            info!(target: "progress",
                "{} steps, just found {} muses of size {}, {} left, {} solver calls so far",
                solvesteps.len(),
                muses.len(),
                muses[0].1.len(),
                self.psolve.get_provable_varlits().len(),
                get_solver_calls(),
            );

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

    /// Returns the solution variables that could not be uniquely determined after
    /// exhausting all constraint propagation.
    ///
    /// This is meaningful only after `check_solvability()` or `quick_solve()` has been
    /// called, which exhausts all deductions. Before that call, the result is undefined.
    ///
    /// Each returned `PuzVar` is a variable whose value is not pinned to a single value
    /// by the current set of puzzle clues. Returns an empty set if the puzzle is fully
    /// solvable (all variables determined) or inconsistent (no solution).
    pub fn unsolved_vars_after_solve(&mut self) -> BTreeSet<super::PuzVar> {
        let lits = self.psolve.get_literals_to_try_solving();
        lits.iter()
            .flat_map(|lit| {
                self.psolve
                    .puzzleparse()
                    .lit_to_vars(lit)
                    .iter()
                    .map(|puzlit| puzlit.var())
                    .collect::<Vec<_>>()
            })
            .collect()
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

            all_lits.extend(varlits);
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
        if base_muses.is_empty() {
            return self.quick_display_html_step_impl(None, "There are no more values to deduce");
        }
        self.quick_display_html_step(Some(base_muses))
    }

    pub fn quick_generate_html_difficulties(&mut self) -> String {
        let base_muses = self.all_muses_with_larger();

        let base_difficulties: BTreeMap<Lit, usize> = base_muses
            .muses()
            .iter()
            .filter_map(|(k, v)| v.iter().map(MusContext::mus_len).min().map(|m| (*k, m)))
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
            return self.quick_display_html_step_impl(None, "There are no more values to deduce");
        }

        let min = min.unwrap();

        let mut vec = vec![];

        for v in muses.muses().values() {
            if let Some(m) = v.iter().next()
                && m.mus_len() == min
            {
                vec.push(m.clone());
            }
        }

        //

        self.quick_display_html_step(Some(vec))
    }

    pub fn quick_display_html_step(
        &mut self,
        base_muses: Option<Vec<MusContext>>,
    ) -> (String, Vec<Lit>) {
        self.quick_display_html_step_impl(base_muses, "The initial puzzle state")
    }

    /// Like `quick_display_html_step(None)` but with a description suitable for a refresh.
    pub fn refresh_html_step(&mut self) -> (String, Vec<Lit>) {
        self.quick_display_html_step_impl(None, "Current puzzle state")
    }

    fn build_step_problem(
        &mut self,
        base_muses: Option<Vec<MusContext>>,
        fallback_description: &str,
    ) -> (Problem, Vec<Lit>) {
        if let Some(base_muses) = base_muses {
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
                    constraints: mus.1.iter().map(|s| tera::escape_html(s)).collect(),
                });
            }

            let v = base_muses
                .iter()
                .flat_map(|mc| &mc.lits)
                .copied()
                .collect_vec();

            // Snapshot tosolve BEFORE marking deduced, so eliminated
            // candidates remain visible (rendered with litneg).
            let varlits = self.psolve.get_provable_varlits().clone();
            let tosolve_varvals: BTreeSet<_> = varlits
                .iter()
                .flat_map(|x| self.psolve.lit_to_puzlit(x))
                .map(super::PuzLit::varval)
                .collect();

            for m in &v {
                self.mark_lit_as_deduced(m);
            }

            let known_lits = self.get_all_known_lits().clone();
            let known_puzlits: BTreeSet<PuzLit> = known_lits
                .iter()
                .flat_map(|x| self.psolve.lit_to_puzlit(x))
                .cloned()
                .collect();

            let problem = Problem::new_from_puzzle_and_mus(
                &self.psolve,
                &tosolve_varvals,
                &known_puzlits,
                &all_deduced,
                &description_list,
                &pre_string,
            )
            .expect("Cannot make puzzle json");

            (problem, v)
        } else {
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

            let deduced = BTreeSet::new();

            let problem = Problem::new_from_puzzle_and_state(
                &self.psolve,
                &tosolve_varvals,
                &known_puzlits,
                &deduced,
                fallback_description,
            )
            .expect("Cannot make puzzle json");

            (problem, vec![])
        }
    }

    fn quick_display_html_step_impl(
        &mut self,
        base_muses: Option<Vec<MusContext>>,
        fallback_description: &str,
    ) -> (String, Vec<Lit>) {
        let (problem, lits) = self.build_step_problem(base_muses, fallback_description);
        (create_html(&problem), lits)
    }

    pub fn solve_step(&mut self) -> (Problem, Vec<Lit>) {
        let base_muses = self.smallest_muses_with_config();
        if base_muses.is_empty() {
            return self.build_step_problem(None, "There are no more values to deduce");
        }
        self.build_step_problem(Some(base_muses), "Made the following deductions")
    }

    pub fn refresh_problem(&mut self) -> (Problem, Vec<Lit>) {
        self.build_step_problem(None, "Current puzzle state")
    }

    /// Render a single MUS as a `Problem` for display without advancing solver state.
    pub fn preview_mus(&mut self, mus: &MusContext) -> Problem {
        let user_mus = self.mus_to_user_mus(mus);
        let all_deduced: BTreeSet<_> = user_mus.0.clone();

        let description_list = vec![DescriptionStatement {
            result: PuzLit::nice_puzlit_list_html(&user_mus.0),
            constraints: user_mus.1.iter().map(|s| tera::escape_html(s)).collect(),
        }];

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

        let pre_string = format!(
            "Explanation using {} constraint{}:<br/>",
            mus.mus_len(),
            if mus.mus_len() == 1 { "" } else { "s" }
        );

        Problem::new_from_puzzle_and_mus(
            &self.psolve,
            &tosolve_varvals,
            &known_puzlits,
            &all_deduced,
            &description_list,
            &pre_string,
        )
        .expect("Cannot make puzzle json")
    }

    /// Compute all MUSes for a single literal, sorted smallest to largest.
    pub fn all_muses_for_literal(&mut self, lit_def: Vec<i64>) -> Vec<MusContext> {
        let varlits = self.psolve.get_provable_varlits().clone();
        let lit_def_clone = lit_def.clone();
        let varlits: BTreeSet<_> = varlits
            .into_iter()
            .filter(|lit| {
                let puzlit_list = self.psolve.lit_to_puzlit(lit);
                for puzlit in puzlit_list {
                    let mut indices = puzlit.var().indices().clone();
                    indices.push(puzlit.val());
                    if indices == lit_def_clone {
                        return true;
                    }
                }
                false
            })
            .collect();

        let mut conf = self.config.mus_config;
        conf.find_bigger = true;
        conf.find_one = false;
        conf.keep_all_muses = true;

        let result = self
            .psolve
            .get_many_vars_small_mus_quick(&varlits, &conf, None);

        let mut seen = BTreeSet::new();
        let mut all: Vec<MusContext> = result
            .muses()
            .values()
            .flat_map(|set| set.iter().cloned())
            .filter(|mc| seen.insert(mc.mus.clone()))
            .collect();
        all.sort_by_key(|mc| mc.mus_len());
        all
    }

    pub fn solve_step_for_literal(&mut self, lit_def: Vec<i64>) -> (Problem, Vec<Lit>) {
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

        let min = muses.min();
        if min.is_none() {
            return self.build_step_problem(None, "There are no more values to deduce");
        }
        let min = min.unwrap();

        let mut vec = vec![];
        for v in muses.muses().values() {
            if let Some(m) = v.iter().next()
                && m.mus_len() == min
            {
                vec.push(m.clone());
            }
        }

        self.build_step_problem(Some(vec), "Made the following deductions")
    }

    pub fn difficulty_problem(&mut self) -> Problem {
        let base_muses = self.all_muses_with_larger();

        let base_difficulties: BTreeMap<Lit, usize> = base_muses
            .muses()
            .iter()
            .filter_map(|(k, v)| v.iter().map(MusContext::mus_len).min().map(|m| (*k, m)))
            .collect();

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

        Problem::new_from_puzzle_and_difficulty(
            &self.psolve,
            &tosolve_varvals,
            &known_puzlits,
            &vvpmap,
            "The difficulty of the problem",
        )
        .expect("Cannot make puzzle json")
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

    use crate::problem::{
        planner::{PlannerConfig, PuzzlePlanner},
        solver::{MusConfig, PuzzleSolver},
    };
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

    /// `all_muses_with_larger` must return a dict configured to retain larger MUSes.
    /// Whether the parallel search happens to *find* multi-size MUSes on a given
    /// instance is nondeterministic, so we check the wiring rather than a specific
    /// search outcome (the MusDict-level unit tests cover retention semantics).
    #[test]
    fn test_all_muses_with_larger_uses_keep_all() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/binairo.eprime",
            "./tst/binairo-1.param",
        );
        let result = Arc::new(result);
        let puz = PuzzleSolver::new(result).unwrap();
        let mut plan = PuzzlePlanner::new(puz);

        let muses = plan.all_muses_with_larger();
        assert!(
            muses.keep_all(),
            "all_muses_with_larger must return a keep_all MusDict"
        );
    }

    /// find_one=true (the new default) must produce the same set of deduced literals as find_one=false.
    #[test]
    fn test_find_one_same_deductions_as_find_all() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperWall.param",
        );
        let result = Arc::new(result);

        let run_solve = |find_one: bool| {
            let puz = PuzzleSolver::new(result.clone()).unwrap();
            let config = PlannerConfig {
                mus_config: MusConfig {
                    find_one,
                    ..MusConfig::default()
                },
                ..PlannerConfig::default()
            };
            let mut plan = PuzzlePlanner::new_with_config(puz, config);
            let seq = plan.quick_solve();
            // Collect the flat list of deduced literal sets across all steps.
            seq.into_iter()
                .flatten()
                .map(|(lits, _)| lits)
                .collect_vec()
        };

        let with_find_one = run_solve(true);
        let without_find_one = run_solve(false);

        // Both runs must deduce the same number of steps (deductions are deterministic).
        assert_eq!(
            with_find_one.len(),
            without_find_one.len(),
            "find_one changed the number of deduction steps"
        );
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
